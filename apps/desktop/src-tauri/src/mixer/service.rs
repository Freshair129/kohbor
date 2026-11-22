//! One host thread owns the mixer and, on Windows, its Core Audio objects (COM, multithreaded apartment). Commands
//! send jobs to it and wait for the reply with a bound, so a stuck audio stack can never hang a command forever.

use super::Mixer;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

type Job = Box<dyn FnOnce(&mut Mixer) + Send>;

const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const EXIT_RESTORE_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) struct MixerService {
    jobs: Mutex<Option<Sender<Job>>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl MixerService {
    /// Start the mixer thread. `factory` runs on that thread, after COM initialisation, so the backend it creates
    /// never crosses threads. A startup restore of originals left by an abnormal exit runs before any job.
    pub(crate) fn start(factory: impl FnOnce() -> Mixer + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let join = std::thread::Builder::new()
            .name("mixer-com".into())
            .spawn(move || run(factory, rx));
        match join {
            Ok(join) => Self {
                jobs: Mutex::new(Some(tx)),
                join: Mutex::new(Some(join)),
            },
            Err(_) => {
                crate::log_warn!(
                    "mixer",
                    "thread_unavailable",
                    "mixer thread could not start"
                );
                Self {
                    jobs: Mutex::new(None),
                    join: Mutex::new(None),
                }
            }
        }
    }

    pub(crate) fn call<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Mixer) -> R + Send + 'static,
    ) -> Result<R, String> {
        self.call_within(job, REPLY_TIMEOUT)
    }

    fn call_within<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Mixer) -> R + Send + 'static,
        timeout: Duration,
    ) -> Result<R, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        let sender = lock(&self.jobs)
            .as_ref()
            .cloned()
            .ok_or_else(|| "MIXER_UNAVAILABLE".to_owned())?;
        sender
            .send(Box::new(move |mixer: &mut Mixer| {
                let _ = reply_tx.send(job(mixer));
            }))
            .map_err(|_| "MIXER_UNAVAILABLE".to_owned())?;
        reply_rx
            .recv_timeout(timeout)
            .map_err(|_| "MIXER_TIMEOUT".to_owned())
    }

    /// Restore every original level, then stop the thread. Called on application exit.
    pub(crate) fn shutdown(&self) {
        let summary = self.call_within(|mixer| mixer.restore_all("exit"), EXIT_RESTORE_TIMEOUT);
        if summary.is_err() {
            crate::log_warn!(
                "mixer",
                "exit_restore_unavailable",
                "exit restore did not finish; recovery data kept for the next start"
            );
        }
        lock(&self.jobs).take();
        if let Some(join) = lock(&self.join).take() {
            let _ = join.join();
        }
    }
}

fn run(factory: impl FnOnce() -> Mixer, jobs: Receiver<Job>) {
    #[cfg(windows)]
    let com = super::native::ComApartment::enter();
    let mut mixer = factory();
    if mixer.recovery_pending() > 0 {
        let summary = mixer.restore_all("startup");
        crate::log_info!(
            "mixer",
            "startup_restore",
            "startup restore after an earlier run",
            restored = summary.restored,
            pending = summary.pending
        );
    }
    while let Ok(job) = jobs.recv() {
        job(&mut mixer);
    }
    drop(mixer);
    #[cfg(windows)]
    drop(com);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::super::{
        BackendError, DeviceRead, Mixer, Original, OriginalStore, SessionBackend, SessionRead,
    };
    use super::MixerService;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Shared {
        level: Arc<Mutex<f32>>,
        stored: Arc<Mutex<Vec<Original>>>,
    }

    struct Backend(Shared);

    impl SessionBackend for Backend {
        fn read(&mut self) -> Result<DeviceRead, BackendError> {
            Ok(DeviceRead {
                device: "dev".into(),
                sessions: vec![SessionRead::Readable {
                    exe: "dota2.exe".into(),
                    id: Some("d1".into()),
                    level: *self.0.level.lock().unwrap(),
                    muted: false,
                }],
            })
        }
        fn apply(
            &mut self,
            _: &str,
            _: &str,
            _: Option<&str>,
            level: Option<f32>,
            _: Option<bool>,
        ) -> Result<usize, BackendError> {
            if let Some(level) = level {
                *self.0.level.lock().unwrap() = level;
            }
            Ok(1)
        }
    }

    struct Store(Shared);

    impl OriginalStore for Store {
        fn load(&self) -> Result<Vec<Original>, String> {
            Ok(self.0.stored.lock().unwrap().clone())
        }
        fn save(&self, entries: &[Original]) -> Result<(), String> {
            *self.0.stored.lock().unwrap() = entries.to_vec();
            Ok(())
        }
    }

    fn service(shared: &Shared) -> MixerService {
        let shared = shared.clone();
        MixerService::start(move || {
            Mixer::new(Box::new(Backend(shared.clone())), Box::new(Store(shared)))
        })
    }

    #[test]
    fn jobs_run_on_the_mixer_thread_and_exit_restores() {
        let shared = Shared::default();
        *shared.level.lock().unwrap() = 0.9;
        let service = service(&shared);
        let reply = service
            .call(|mixer| mixer.set_level("game", 30).map_err(|(code, _)| code))
            .unwrap()
            .unwrap();
        assert_eq!(reply["channels"][0]["id"], "game");
        assert_eq!(reply["channels"][0]["percent"], 30);
        service.shutdown();
        assert!((*shared.level.lock().unwrap() - 0.9).abs() < 1e-6);
        assert!(shared.stored.lock().unwrap().is_empty());
        assert_eq!(service.call(|_| ()).unwrap_err(), "MIXER_UNAVAILABLE");
    }

    #[test]
    fn startup_restores_originals_left_by_a_crash() {
        let shared = Shared::default();
        *shared.level.lock().unwrap() = 0.2;
        shared.stored.lock().unwrap().push(Original {
            device: "dev".into(),
            exe: "dota2.exe".into(),
            session: None,
            level: 0.75,
            muted: false,
        });
        let service = service(&shared);
        // The first job runs after the startup restore.
        service.call(|_| ()).unwrap();
        assert!((*shared.level.lock().unwrap() - 0.75).abs() < 1e-6);
        assert!(shared.stored.lock().unwrap().is_empty());
        service.shutdown();
    }
}

use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VolumeStep {
    Increase,
    Decrease,
}

pub(crate) fn step_volume(current: u8, step: VolumeStep) -> u8 {
    let current = current.min(100);
    match step {
        VolumeStep::Increase => current.saturating_add(10).min(100),
        VolumeStep::Decrease => current.saturating_sub(10),
    }
}

const MOD_ALT_BITS: u32 = 0x0001;
const MOD_NOREPEAT_BITS: u32 = 0x4000;
const VK_UP_VALUE: u32 = 0x26;
const VK_DOWN_VALUE: u32 = 0x28;

fn decode_hotkey(modifiers: u32, virtual_key: u32) -> Option<VolumeStep> {
    if modifiers & MOD_ALT_BITS == 0 || modifiers & !(MOD_ALT_BITS | MOD_NOREPEAT_BITS) != 0 {
        return None;
    }
    match virtual_key {
        VK_UP_VALUE => Some(VolumeStep::Increase),
        VK_DOWN_VALUE => Some(VolumeStep::Decrease),
        _ => None,
    }
}

fn stop_and_join(worker: std::thread::JoinHandle<()>, request_stop: impl FnOnce()) -> bool {
    request_stop();
    worker.join().is_ok()
}

#[derive(Debug)]
pub(crate) enum StartFailure {
    Registration(u32),
    Thread(std::io::Error),
}

#[derive(Default)]
pub(crate) struct Controller {
    registered: bool,
    #[cfg(windows)]
    worker: Option<std::thread::JoinHandle<()>>,
    #[cfg(windows)]
    thread_id: u32,
}

impl Controller {
    pub(crate) fn is_registered(&self) -> bool {
        self.registered
    }

    pub(crate) fn set_enabled(
        &mut self,
        enabled: bool,
        callback: Arc<dyn Fn(VolumeStep) + Send + Sync + 'static>,
    ) -> Result<bool, StartFailure> {
        if enabled {
            self.start(callback)
        } else {
            self.stop();
            Ok(false)
        }
    }

    fn start(
        &mut self,
        callback: Arc<dyn Fn(VolumeStep) + Send + Sync + 'static>,
    ) -> Result<bool, StartFailure> {
        if self.registered {
            return Ok(true);
        }
        #[cfg(windows)]
        {
            self.start_windows(callback)
        }
        #[cfg(not(windows))]
        {
            let _ = callback;
            Ok(false)
        }
    }

    #[cfg(windows)]
    fn start_windows(
        &mut self,
        callback: Arc<dyn Fn(VolumeStep) + Send + Sync + 'static>,
    ) -> Result<bool, StartFailure> {
        use std::sync::mpsc;
        use std::thread;
        use windows::Win32::Foundation::GetLastError;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_DOWN, VK_UP,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY,
        };

        const HOTKEY_UP_ID: i32 = 1;
        const HOTKEY_DOWN_ID: i32 = 2;
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("audio-hotkeys".to_owned())
            .spawn(move || unsafe {
                let thread_id = GetCurrentThreadId();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);

                let modifiers = MOD_ALT | MOD_NOREPEAT;
                let up_result = RegisterHotKey(None, HOTKEY_UP_ID, modifiers, u32::from(VK_UP.0));
                let up_error = if up_result.is_err() {
                    GetLastError().0
                } else {
                    0
                };
                let down_result =
                    RegisterHotKey(None, HOTKEY_DOWN_ID, modifiers, u32::from(VK_DOWN.0));
                let down_error = if down_result.is_err() {
                    GetLastError().0
                } else {
                    0
                };
                if up_result.is_err() || down_result.is_err() {
                    if up_result.is_ok() {
                        let _ = UnregisterHotKey(None, HOTKEY_UP_ID);
                    }
                    if down_result.is_ok() {
                        let _ = UnregisterHotKey(None, HOTKEY_DOWN_ID);
                    }
                    let code = if up_error != 0 {
                        up_error
                    } else if down_error != 0 {
                        down_error
                    } else {
                        up_result
                            .as_ref()
                            .err()
                            .or_else(|| down_result.as_ref().err())
                            .map(|error| (error.code().0 as u32) & 0xffff)
                            .unwrap_or_default()
                    };
                    let _ = startup_tx.send(Err(code));
                    return;
                }
                if startup_tx.send(Ok(thread_id)).is_err() {
                    let _ = UnregisterHotKey(None, HOTKEY_UP_ID);
                    let _ = UnregisterHotKey(None, HOTKEY_DOWN_ID);
                    return;
                }

                loop {
                    let result = GetMessageW(&mut message, None, 0, 0).0;
                    if result <= 0 {
                        break;
                    }
                    let hotkey_id = message.wParam.0 as i32;
                    if message.message == WM_HOTKEY
                        && (hotkey_id == HOTKEY_UP_ID || hotkey_id == HOTKEY_DOWN_ID)
                    {
                        let modifiers = (message.lParam.0 as u32) & 0xffff;
                        let virtual_key = ((message.lParam.0 as u32) >> 16) & 0xffff;
                        if let Some(step) = decode_hotkey(modifiers, virtual_key) {
                            callback(step);
                        }
                    }
                }

                let _ = UnregisterHotKey(None, HOTKEY_UP_ID);
                let _ = UnregisterHotKey(None, HOTKEY_DOWN_ID);
            })
            .map_err(StartFailure::Thread)?;

        match startup_rx.recv() {
            Ok(Ok(thread_id)) => {
                self.thread_id = thread_id;
                self.worker = Some(worker);
                self.registered = true;
                Ok(true)
            }
            Ok(Err(error_code)) => {
                let _ = worker.join();
                self.registered = false;
                Err(StartFailure::Registration(error_code))
            }
            Err(_) => {
                let _ = worker.join();
                self.registered = false;
                Err(StartFailure::Thread(std::io::Error::other(
                    "hotkey thread ended before registration completed",
                )))
            }
        }
    }

    pub(crate) fn stop(&mut self) {
        #[cfg(windows)]
        if let Some(worker) = self.worker.take() {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            let _ = stop_and_join(worker, || unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            });
            self.thread_id = 0;
        }
        self.registered = false;
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_hotkey, step_volume, stop_and_join, VolumeStep};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    const MOD_ALT: u32 = 0x0001;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_UP: u32 = 0x26;
    const VK_DOWN: u32 = 0x28;

    #[test]
    fn volume_step_increases_by_ten_points() {
        assert_eq!(step_volume(55, VolumeStep::Increase), 65);
    }

    #[test]
    fn volume_step_decreases_by_ten_points() {
        assert_eq!(step_volume(55, VolumeStep::Decrease), 45);
    }

    #[test]
    fn volume_step_increase_clamps_at_one_hundred() {
        assert_eq!(step_volume(100, VolumeStep::Increase), 100);
    }

    #[test]
    fn volume_step_decrease_clamps_at_zero() {
        assert_eq!(step_volume(0, VolumeStep::Decrease), 0);
    }

    #[test]
    fn decode_recognizes_alt_up() {
        assert_eq!(
            decode_hotkey(MOD_ALT | MOD_NOREPEAT, VK_UP),
            Some(VolumeStep::Increase)
        );
    }

    #[test]
    fn decode_recognizes_alt_down() {
        assert_eq!(
            decode_hotkey(MOD_ALT | MOD_NOREPEAT, VK_DOWN),
            Some(VolumeStep::Decrease)
        );
    }

    #[test]
    fn decode_rejects_missing_alt_and_extra_modifiers() {
        assert_eq!(decode_hotkey(0, VK_UP), None);
        assert_eq!(decode_hotkey(MOD_ALT | 0x0002, VK_UP), None);
    }

    #[test]
    fn stop_requests_and_joins_worker_before_timeout() {
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            stop_rx.recv().unwrap();
        });
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let shutdown = thread::spawn(move || {
            let joined = stop_and_join(worker, || stop_tx.send(()).unwrap());
            shutdown_tx.send(joined).unwrap();
        });

        assert!(shutdown_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("shutdown joined before timeout"));
        shutdown.join().unwrap();
    }
}

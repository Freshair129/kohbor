//! Process containment and resource sampling (CORE-002, QUALITY-002, SDD §14).
//!
//! The job is created and the host assigned to it before any WebView or helper
//! process exists; children inherit membership (no breakaway flags are set).

use std::sync::OnceLock;

/// CPU hard cap in 1/100 percent units: 1000 = 10% of total system CPU.
pub const CPU_RATE_HARD_CAP: u32 = 1000;

/// Operations required to establish the process Job Object containment boundary.
///
/// Configuration is split into its two Win32 operations so setup can preserve
/// their existing failure identifiers (`cpu-rate` and `limits`).
///
/// A step reports only success or failure: the caller maps it to a fixed, safe
/// identifier and must not surface OS error text.
#[allow(clippy::result_unit_err)]
pub trait JobBackend {
    fn create(&mut self) -> Result<(), ()>;
    fn configure_cpu_rate(&mut self) -> Result<(), ()>;
    fn configure_limits(&mut self) -> Result<(), ()>;
    fn assign(&mut self) -> Result<(), ()>;
}

/// Establish containment through a backend, returning the failed setup step.
pub fn establish_with<B: JobBackend>(backend: &mut B) -> Result<(), &'static str> {
    backend.create().map_err(|_| "create")?;
    backend.configure_cpu_rate().map_err(|_| "cpu-rate")?;
    backend.configure_limits().map_err(|_| "limits")?;
    backend.assign().map_err(|_| "assign")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Normalized CPU over the last interval, 0..=100, or None if unavailable.
    pub cpu_percent: Option<f64>,
    /// Sum of full Working Set of job processes, or None if unavailable.
    pub working_set_bytes: Option<u64>,
    pub process_count: Option<u32>,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::*;
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct Job(HANDLE);
    // The job handle is only used for queries after setup; HANDLE is a plain value.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    static JOB: OnceLock<Job> = OnceLock::new();

    #[derive(Default)]
    struct RealBackend {
        job: Option<Job>,
    }

    impl JobBackend for RealBackend {
        fn create(&mut self) -> Result<(), ()> {
            let job = unsafe { CreateJobObjectW(None, PCWSTR::null()).map_err(|_| ())? };
            self.job = Some(Job(job));
            Ok(())
        }

        fn configure_cpu_rate(&mut self) -> Result<(), ()> {
            let job = self.job.as_ref().ok_or(())?.0;
            unsafe {
                let mut rate = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                    ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE
                        | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
                    ..Default::default()
                };
                rate.Anonymous.CpuRate = CPU_RATE_HARD_CAP;
                SetInformationJobObject(
                    job,
                    JobObjectCpuRateControlInformation,
                    &rate as *const _ as *const c_void,
                    size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                )
                .map_err(|_| ())?;
            }
            Ok(())
        }

        fn configure_limits(&mut self) -> Result<(), ()> {
            let job = self.job.as_ref().ok_or(())?.0;
            unsafe {
                let mut ext = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                ext.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &ext as *const _ as *const c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
                .map_err(|_| ())?;
            }
            Ok(())
        }

        fn assign(&mut self) -> Result<(), ()> {
            let job = self.job.as_ref().ok_or(())?.0;
            unsafe {
                AssignProcessToJobObject(job, GetCurrentProcess()).map_err(|_| ())?;
            }
            Ok(())
        }
    }

    /// Create, configure and join the job. Must run before the WebView is created.
    pub fn establish() -> Result<(), &'static str> {
        let mut backend = RealBackend::default();
        establish_with(&mut backend)?;
        if let Some(job) = backend.job.take() {
            let _ = JOB.set(job);
        }
        Ok(())
    }

    fn job() -> Option<HANDLE> {
        JOB.get().map(|j| j.0)
    }

    /// Total user+kernel CPU time of the job in 100 ns units.
    pub fn cpu_time_100ns() -> Option<u64> {
        let job = job()?;
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(job),
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut c_void,
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
            .ok()?;
        }
        Some((info.TotalUserTime + info.TotalKernelTime) as u64)
    }

    /// Process IDs currently in the job.
    pub fn process_ids() -> Option<Vec<u32>> {
        let job = job()?;
        // Header (2 x u32) followed by up to 256 ULONG_PTR ids.
        let mut buf = vec![0usize; 2 + 256];
        unsafe {
            QueryInformationJobObject(
                Some(job),
                JobObjectBasicProcessIdList,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * size_of::<usize>()) as u32,
                None,
            )
            .ok()?;
            let list = &*(buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST);
            let n = list.NumberOfProcessIdsInList as usize;
            let ids = std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), n);
            Some(ids.iter().map(|id| *id as u32).collect())
        }
    }

    /// Sum of full Working Set for the given processes.
    pub fn working_set(pids: &[u32]) -> Option<u64> {
        let mut total = 0u64;
        for pid in pids {
            unsafe {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, *pid).ok()?;
                let mut pmc = PROCESS_MEMORY_COUNTERS {
                    cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                    ..Default::default()
                };
                let ok = K32GetProcessMemoryInfo(h, &mut pmc, pmc.cb).as_bool();
                let _ = CloseHandle(h);
                if !ok {
                    return None;
                }
                total += pmc.WorkingSetSize as u64;
            }
        }
        Some(total)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn establish() -> Result<(), &'static str> {
        Err("unsupported-platform")
    }
    pub fn cpu_time_100ns() -> Option<u64> {
        None
    }
    pub fn process_ids() -> Option<Vec<u32>> {
        None
    }
    pub fn working_set(_: &[u32]) -> Option<u64> {
        None
    }
}

pub use imp::establish;

/// Per-second sampler state: normalized CPU from job accounting deltas.
pub struct Sampler {
    last: Option<(std::time::Instant, u64)>,
    logical_cpus: f64,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        Self {
            last: None,
            logical_cpus: cpus as f64,
        }
    }

    pub fn sample(&mut self) -> Sample {
        let now = std::time::Instant::now();
        let cpu_now = imp::cpu_time_100ns();
        let cpu_percent = match (self.last, cpu_now) {
            (Some((t0, c0)), Some(c1)) if c1 >= c0 => {
                let wall_100ns = now.duration_since(t0).as_nanos() as f64 / 100.0;
                (wall_100ns > 0.0).then(|| {
                    ((c1 - c0) as f64 / (wall_100ns * self.logical_cpus) * 100.0).min(100.0)
                })
            }
            _ => None,
        };
        self.last = cpu_now.map(|c| (now, c));
        let pids = imp::process_ids();
        let working_set_bytes = pids.as_deref().and_then(imp::working_set);
        Sample {
            cpu_percent,
            working_set_bytes,
            process_count: pids.map(|p| p.len() as u32),
        }
    }
}

/// Soft throttle with hysteresis (SDD §14): enter at ≥9.0%, metric loss or memory
/// over budget; exit after three consecutive samples ≤8.0% with memory in budget.
#[derive(Debug, Default)]
pub struct Throttle {
    pub throttled: bool,
    calm_samples: u8,
}

impl Throttle {
    pub fn update(&mut self, s: &Sample) -> bool {
        // Memory is measured and reported, never limited: no RAM limit exists until the owner sets one (DEC-RESOURCE-BUDGET).
        match s.cpu_percent {
            None => {
                self.throttled = true;
                self.calm_samples = 0;
            }
            Some(c) if c >= 9.0 => {
                self.throttled = true;
                self.calm_samples = 0;
            }
            Some(c) if self.throttled && c <= 8.0 => {
                self.calm_samples += 1;
                if self.calm_samples >= 3 {
                    self.throttled = false;
                    self.calm_samples = 0;
                }
            }
            Some(_) => self.calm_samples = 0,
        }
        self.throttled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(cpu: Option<f64>, mem: u64) -> Sample {
        Sample {
            cpu_percent: cpu,
            working_set_bytes: Some(mem),
            process_count: Some(1),
        }
    }

    #[test]
    fn throttle_enters_at_9_and_needs_three_calm_samples_to_exit() {
        let mut t = Throttle::default();
        assert!(!t.update(&s(Some(8.9), 1)));
        assert!(t.update(&s(Some(9.0), 1)));
        assert!(t.update(&s(Some(8.0), 1)));
        assert!(t.update(&s(Some(7.0), 1)));
        assert!(
            t.update(&s(Some(8.5), 1)),
            "8.5 is above the exit threshold and resets"
        );
        assert!(t.update(&s(Some(1.0), 1)));
        assert!(t.update(&s(Some(1.0), 1)));
        assert!(!t.update(&s(Some(1.0), 1)));
    }

    #[test]
    fn metric_loss_throttles_and_memory_alone_never_does() {
        let mut t = Throttle::default();
        assert!(t.update(&s(None, 1)));
        let mut t = Throttle::default();
        assert!(
            !t.update(&s(Some(1.0), u64::MAX)),
            "no memory limit exists: a large Working Set must not throttle"
        );
    }
}

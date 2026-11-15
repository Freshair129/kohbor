use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod wasapi_backend;

static MASTER_VOLUME_PERCENT: AtomicU32 = AtomicU32::new(100);

/// Set the process-wide output gain target. The render callback reads this atomic directly.
pub fn set_master_volume_percent(percent: u8) {
    MASTER_VOLUME_PERCENT.store(u32::from(percent.min(100)), Ordering::Relaxed);
}

/// Return the current process-wide output gain target.
pub fn master_volume_percent() -> u8 {
    MASTER_VOLUME_PERCENT.load(Ordering::Relaxed).min(100) as u8
}

#[cfg(windows)]
#[cfg(feature = "latency-probe")]
pub use engine::FirstFillProbe;
#[cfg(windows)]
pub use wasapi_backend::{StartError, WasapiSink};

#[cfg(test)]
mod tests {
    use super::{master_volume_percent, set_master_volume_percent};

    #[test]
    fn master_volume_target_is_clamped_to_one_hundred() {
        set_master_volume_percent(u8::MAX);

        assert_eq!(master_volume_percent(), 100);

        set_master_volume_percent(100);
    }
}

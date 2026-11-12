#![allow(dead_code)]

use std::time::{Duration, Instant};

const MAX_SAFE_INTEGER_MS: u64 = 9_007_199_254_740_991;

/// Monotonic elapsed time relative to the intake runtime's origin.
#[derive(Clone, Copy)]
pub struct MonoClock {
    origin: Instant,
}

impl MonoClock {
    pub fn from_origin(origin: Instant) -> Self {
        Self { origin }
    }

    pub fn ms_at(&self, t: Instant) -> u64 {
        duration_ms(t.saturating_duration_since(self.origin))
    }

    pub fn now_ms(&self) -> u64 {
        self.ms_at(Instant::now())
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(MAX_SAFE_INTEGER_MS)) as u64
}

#[cfg(test)]
mod tests {
    use super::{duration_ms, MonoClock};
    use std::time::{Duration, Instant};

    const MAX_SAFE_INTEGER_MS: u64 = 9_007_199_254_740_991;

    #[test]
    fn elapsed_milliseconds_follow_the_supplied_monotonic_instants() {
        let origin = Instant::now();
        let clock = MonoClock::from_origin(origin);

        assert_eq!(clock.ms_at(origin), 0);
        assert_eq!(clock.ms_at(origin + Duration::from_millis(4)), 4);
        assert_eq!(clock.ms_at(origin + Duration::from_millis(9)), 9);
    }

    #[test]
    fn elapsed_time_saturates_before_the_origin() {
        let origin = Instant::now();
        let clock = MonoClock::from_origin(origin);

        assert_eq!(clock.ms_at(origin - Duration::from_millis(1)), 0);
    }

    #[test]
    fn elapsed_milliseconds_are_capped_at_the_safe_integer_limit() {
        assert_eq!(
            duration_ms(Duration::from_millis(MAX_SAFE_INTEGER_MS + 5)),
            MAX_SAFE_INTEGER_MS
        );
    }

    #[test]
    fn now_ms_is_monotonic() {
        let clock = MonoClock::from_origin(Instant::now());
        let first = clock.now_ms();
        let second = clock.now_ms();
        assert!(second >= first);
    }
}

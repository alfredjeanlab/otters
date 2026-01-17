//! Clock abstraction for testable time handling

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A clock that provides the current time
pub trait Clock: Clone + Send + Sync {
    fn now(&self) -> Instant;
}

/// Real system clock
#[derive(Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Fake clock for testing with controllable time
#[derive(Clone)]
pub struct FakeClock {
    current: Arc<Mutex<Instant>>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Advance the clock by the given duration
    pub fn advance(&self, duration: Duration) {
        let mut current = self.current.lock().unwrap();
        *current += duration;
    }

    /// Set the clock to a specific instant
    pub fn set(&self, instant: Instant) {
        let mut current = self.current.lock().unwrap();
        *current = instant;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        *self.current.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_increasing_time() {
        let clock = SystemClock;
        let t1 = clock.now();
        std::thread::sleep(Duration::from_millis(1));
        let t2 = clock.now();
        assert!(t2 > t1);
    }

    #[test]
    fn fake_clock_can_be_advanced() {
        let clock = FakeClock::new();
        let t1 = clock.now();
        clock.advance(Duration::from_secs(60));
        let t2 = clock.now();
        assert!(t2.duration_since(t1) >= Duration::from_secs(60));
    }

    #[test]
    fn fake_clock_is_cloneable_and_shared() {
        let clock1 = FakeClock::new();
        let clock2 = clock1.clone();
        let t1 = clock1.now();
        clock2.advance(Duration::from_secs(30));
        let t2 = clock1.now();
        assert!(t2.duration_since(t1) >= Duration::from_secs(30));
    }
}

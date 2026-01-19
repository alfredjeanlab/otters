// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

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
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        *current += duration;
    }

    /// Set the clock to a specific instant
    pub fn set(&self, instant: Instant) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
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
        *self.current.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Periodic maintenance task for coordination primitives
//!
//! Provides background task for stale resource reclaim and health checks.

use super::manager::CoordinationManager;
use crate::clock::Clock;
use crate::effect::Effect;
use std::time::Duration;

/// Configuration for maintenance task
#[derive(Clone, Debug)]
pub struct MaintenanceConfig {
    /// How often to run maintenance
    pub interval: Duration,
    /// Whether to reclaim stale resources
    pub reclaim_stale: bool,
    /// Whether to emit stale warnings before reclaiming
    pub emit_warnings: bool,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            reclaim_stale: true,
            emit_warnings: true,
        }
    }
}

impl MaintenanceConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn with_reclaim_stale(mut self, enabled: bool) -> Self {
        self.reclaim_stale = enabled;
        self
    }

    pub fn with_emit_warnings(mut self, enabled: bool) -> Self {
        self.emit_warnings = enabled;
        self
    }
}

/// Background maintenance task for coordination primitives
pub struct MaintenanceTask<C: Clock> {
    config: MaintenanceConfig,
    clock: C,
}

impl<C: Clock> MaintenanceTask<C> {
    pub fn new(config: MaintenanceConfig, clock: C) -> Self {
        Self { config, clock }
    }

    /// Run a single maintenance cycle
    ///
    /// Returns effects that should be processed (e.g., stale warnings, reclaim events)
    pub fn tick(&self, coordination: &mut CoordinationManager) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Emit warnings for stale resources
        if self.config.emit_warnings {
            effects.extend(coordination.tick(&self.clock));
        }

        // Reclaim stale resources if configured
        if self.config.reclaim_stale {
            effects.extend(coordination.reclaim_stale(&self.clock));
        }

        effects
    }

    /// Get the maintenance interval
    pub fn interval(&self) -> Duration {
        self.config.interval
    }

    /// Get the clock reference
    pub fn clock(&self) -> &C {
        &self.clock
    }
}

/// Statistics about coordination resources
#[derive(Clone, Debug, Default)]
pub struct CoordinationStats {
    pub total_locks: usize,
    pub held_locks: usize,
    pub stale_locks: usize,
    pub total_semaphores: usize,
    pub total_semaphore_holders: usize,
    pub stale_semaphore_holders: usize,
    pub total_guards: usize,
}

impl CoordinationStats {
    /// Collect statistics from a coordination manager
    pub fn collect(manager: &CoordinationManager, clock: &impl Clock) -> Self {
        let mut stats = CoordinationStats::default();

        // Lock stats
        for name in manager.lock_names() {
            if let Some(lock) = manager.get_lock(&name) {
                stats.total_locks += 1;
                if !lock.is_free() {
                    stats.held_locks += 1;
                    if lock.is_stale(clock) {
                        stats.stale_locks += 1;
                    }
                }
            }
        }

        // Semaphore stats
        for name in manager.semaphore_names() {
            if let Some(sem) = manager.get_semaphore(&name) {
                stats.total_semaphores += 1;
                stats.total_semaphore_holders += sem.holders.len();
                stats.stale_semaphore_holders += sem.stale_holders(clock).len();
            }
        }

        // Guard stats
        stats.total_guards = manager.guard_ids().len();

        stats
    }
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;

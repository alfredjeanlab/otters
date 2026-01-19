// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Storage serialization for coordination primitives
//!
//! Provides serializable versions of Lock and Semaphore that handle
//! non-serializable Instant fields.

use super::lock::{HolderId, Lock, LockConfig, LockState};
use super::manager::CoordinationManager;
use super::semaphore::{Semaphore, SemaphoreConfig, SemaphoreHolder};
use crate::clock::{Clock, SystemClock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Serializable version of LockState
#[derive(Debug, Clone, Serialize, Deserialize)]
enum StorableLockState {
    Free,
    Held {
        holder: String,
        metadata: Option<String>,
        /// Microseconds since lock was acquired (used to detect stale on load)
        age_micros: u64,
    },
}

/// Serializable version of Lock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableLock {
    name: String,
    stale_threshold_secs: u64,
    heartbeat_interval_secs: u64,
    state: StorableLockState,
}

impl StorableLock {
    /// Convert from Lock to StorableLock
    pub fn from_lock(lock: &Lock, clock: &impl Clock) -> Self {
        let state = match &lock.state {
            LockState::Free => StorableLockState::Free,
            LockState::Held {
                holder,
                metadata,
                last_heartbeat,
            } => StorableLockState::Held {
                holder: holder.0.clone(),
                metadata: metadata.clone(),
                age_micros: clock.now().duration_since(*last_heartbeat).as_micros() as u64,
            },
        };

        StorableLock {
            name: lock.config.name.clone(),
            stale_threshold_secs: lock.config.stale_threshold.as_secs(),
            heartbeat_interval_secs: lock.config.heartbeat_interval.as_secs(),
            state,
        }
    }

    /// Convert from StorableLock to Lock
    pub fn to_lock(&self, clock: &impl Clock) -> Lock {
        let config = LockConfig::new(&self.name)
            .with_stale_threshold(Duration::from_secs(self.stale_threshold_secs))
            .with_heartbeat_interval(Duration::from_secs(self.heartbeat_interval_secs));

        let state = match &self.state {
            StorableLockState::Free => LockState::Free,
            StorableLockState::Held {
                holder,
                metadata,
                age_micros,
            } => {
                // Reconstruct last_heartbeat by subtracting age from now
                let age = Duration::from_micros(*age_micros);
                let last_heartbeat = clock.now() - age;

                LockState::Held {
                    holder: HolderId::new(holder),
                    metadata: metadata.clone(),
                    last_heartbeat,
                }
            }
        };

        Lock { config, state }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Serializable version of SemaphoreHolder
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorableSemaphoreHolder {
    holder_id: String,
    weight: u32,
    metadata: Option<String>,
    age_micros: u64,
}

/// Serializable version of Semaphore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableSemaphore {
    name: String,
    max_slots: u32,
    stale_threshold_secs: u64,
    holders: Vec<StorableSemaphoreHolder>,
}

impl StorableSemaphore {
    /// Convert from Semaphore to StorableSemaphore
    pub fn from_semaphore(sem: &Semaphore, clock: &impl Clock) -> Self {
        let holders = sem
            .holders
            .values()
            .map(|h| StorableSemaphoreHolder {
                holder_id: h.holder_id.clone(),
                weight: h.weight,
                metadata: h.metadata.clone(),
                age_micros: clock.now().duration_since(h.last_heartbeat).as_micros() as u64,
            })
            .collect();

        StorableSemaphore {
            name: sem.config.name.clone(),
            max_slots: sem.config.max_slots,
            stale_threshold_secs: sem.config.stale_threshold.as_secs(),
            holders,
        }
    }

    /// Convert from StorableSemaphore to Semaphore
    pub fn to_semaphore(&self, clock: &impl Clock) -> Semaphore {
        let config = SemaphoreConfig::new(&self.name, self.max_slots)
            .with_stale_threshold(Duration::from_secs(self.stale_threshold_secs));

        let mut holders = HashMap::new();
        for sh in &self.holders {
            let age = Duration::from_micros(sh.age_micros);
            let last_heartbeat = clock.now() - age;

            holders.insert(
                sh.holder_id.clone(),
                SemaphoreHolder {
                    holder_id: sh.holder_id.clone(),
                    weight: sh.weight,
                    metadata: sh.metadata.clone(),
                    last_heartbeat,
                },
            );
        }

        Semaphore { config, holders }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Serializable version of coordination manager state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorableCoordinationState {
    pub locks: Vec<StorableLock>,
    pub semaphores: Vec<StorableSemaphore>,
}

impl StorableCoordinationState {
    /// Convert from CoordinationManager to StorableCoordinationState
    pub fn from_manager(manager: &CoordinationManager, clock: &impl Clock) -> Self {
        let locks = manager
            .lock_names()
            .iter()
            .filter_map(|name| manager.get_lock(name))
            .map(|lock| StorableLock::from_lock(lock, clock))
            .collect();

        let semaphores = manager
            .semaphore_names()
            .iter()
            .filter_map(|name| manager.get_semaphore(name))
            .map(|sem| StorableSemaphore::from_semaphore(sem, clock))
            .collect();

        StorableCoordinationState { locks, semaphores }
    }

    /// Restore a CoordinationManager from stored state
    pub fn to_manager(&self) -> CoordinationManager {
        let clock = SystemClock;
        let mut manager = CoordinationManager::new();

        for storable_lock in &self.locks {
            let lock = storable_lock.to_lock(&clock);
            manager.ensure_lock(lock.config.clone());
            // Re-acquire if it was held (to restore state)
            if let LockState::Held {
                holder, metadata, ..
            } = &lock.state
            {
                manager.acquire_lock(
                    &storable_lock.name,
                    holder.clone(),
                    metadata.clone(),
                    &clock,
                );
            }
        }

        for storable_sem in &self.semaphores {
            let sem = storable_sem.to_semaphore(&clock);
            manager.ensure_semaphore(sem.config.clone());
            // Re-acquire slots for each holder
            for holder in sem.holders.values() {
                manager.acquire_semaphore(
                    &storable_sem.name,
                    holder.holder_id.clone(),
                    holder.weight,
                    holder.metadata.clone(),
                    &clock,
                );
            }
        }

        manager
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

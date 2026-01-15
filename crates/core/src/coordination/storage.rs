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
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    #[test]
    fn storable_lock_roundtrip_free() {
        let clock = FakeClock::new();
        let lock = Lock::new(LockConfig::new("test-lock"));

        let storable = StorableLock::from_lock(&lock, &clock);
        let restored = storable.to_lock(&clock);

        assert!(restored.is_free());
        assert_eq!(restored.config.name, "test-lock");
    }

    #[test]
    fn storable_lock_roundtrip_held() {
        let clock = FakeClock::new();
        let mut lock = Lock::new(LockConfig::new("test-lock"));
        let (new_lock, _) = lock.transition(
            super::super::lock::LockInput::Acquire {
                holder: HolderId::new("holder-1"),
                metadata: Some("test-metadata".to_string()),
            },
            &clock,
        );
        lock = new_lock;

        // Advance time a bit
        clock.advance(Duration::from_secs(10));

        let storable = StorableLock::from_lock(&lock, &clock);
        let restored = storable.to_lock(&clock);

        assert!(!restored.is_free());
        assert_eq!(restored.holder().unwrap().0, "holder-1");
    }

    #[test]
    fn storable_semaphore_roundtrip() {
        let clock = FakeClock::new();
        let mut sem = Semaphore::new(SemaphoreConfig::new("test-sem", 5));
        let (new_sem, _) = sem.transition(
            super::super::semaphore::SemaphoreInput::Acquire {
                holder_id: "holder-1".to_string(),
                weight: 2,
                metadata: Some("test".to_string()),
            },
            &clock,
        );
        sem = new_sem;

        clock.advance(Duration::from_secs(5));

        let storable = StorableSemaphore::from_semaphore(&sem, &clock);
        let restored = storable.to_semaphore(&clock);

        assert_eq!(restored.used_slots(), 2);
        assert!(restored.holders.contains_key("holder-1"));
    }

    #[test]
    fn storable_coordination_state_roundtrip() {
        let clock = FakeClock::new();
        let mut manager = CoordinationManager::new();

        manager.ensure_lock(LockConfig::new("lock-1"));
        manager.acquire_lock(
            "lock-1",
            HolderId::new("holder-1"),
            Some("metadata".to_string()),
            &clock,
        );

        manager.ensure_semaphore(SemaphoreConfig::new("sem-1", 10));
        manager.acquire_semaphore("sem-1", "holder-2".to_string(), 3, None, &clock);

        let storable = StorableCoordinationState::from_manager(&manager, &clock);

        // Verify serialization
        let json = serde_json::to_string_pretty(&storable).unwrap();
        assert!(json.contains("lock-1"));
        assert!(json.contains("sem-1"));

        // Verify deserialization
        let deserialized: StorableCoordinationState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.locks.len(), 1);
        assert_eq!(deserialized.semaphores.len(), 1);

        // Verify restoration
        let restored_manager = deserialized.to_manager();
        assert!(restored_manager.get_lock("lock-1").is_some());
        assert!(restored_manager.get_semaphore("sem-1").is_some());
    }
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Coordination manager for unified lock/semaphore/guard operations
//!
//! Provides a central interface for managing coordination primitives.

use super::guard::{GuardCondition, GuardInputs, GuardResult};
use super::lock::{HolderId, Lock, LockConfig, LockInput};
use super::semaphore::{Semaphore, SemaphoreConfig, SemaphoreInput};
use crate::clock::Clock;
use crate::effect::Effect;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A registered guard with its wake patterns
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisteredGuard {
    pub id: String,
    pub condition: GuardCondition,
    /// Event patterns that should trigger re-evaluation
    pub wake_on: Vec<String>,
}

impl RegisteredGuard {
    pub fn new(id: impl Into<String>, condition: GuardCondition) -> Self {
        Self {
            id: id.into(),
            condition,
            wake_on: Vec::new(),
        }
    }

    pub fn with_wake_on(mut self, patterns: Vec<String>) -> Self {
        self.wake_on = patterns;
        self
    }
}

/// Manages all coordination primitives
#[derive(Clone, Debug, Default)]
pub struct CoordinationManager {
    /// Named locks
    locks: HashMap<String, Lock>,
    /// Named semaphores
    semaphores: HashMap<String, Semaphore>,
    /// Registered guards by ID
    guards: HashMap<String, RegisteredGuard>,
}

impl CoordinationManager {
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
            semaphores: HashMap::new(),
            guards: HashMap::new(),
        }
    }

    // === Lock Operations ===

    /// Create or get a lock
    pub fn ensure_lock(&mut self, config: LockConfig) -> &Lock {
        self.locks
            .entry(config.name.clone())
            .or_insert_with(|| Lock::new(config))
    }

    /// Get a lock by name
    pub fn get_lock(&self, name: &str) -> Option<&Lock> {
        self.locks.get(name)
    }

    /// Attempt to acquire a lock
    pub fn acquire_lock(
        &mut self,
        name: &str,
        holder: HolderId,
        metadata: Option<String>,
        clock: &impl Clock,
    ) -> (bool, Vec<Effect>) {
        let lock = match self.locks.get(name) {
            Some(l) => l.clone(),
            None => {
                // Auto-create lock with default config
                Lock::new(LockConfig::new(name))
            }
        };

        let (new_lock, effects) = lock.transition(
            LockInput::Acquire {
                holder: holder.clone(),
                metadata,
            },
            clock,
        );

        let acquired = new_lock.is_held_by(&holder);
        self.locks.insert(name.to_string(), new_lock);

        (acquired, effects)
    }

    /// Release a lock
    pub fn release_lock(
        &mut self,
        name: &str,
        holder: HolderId,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let lock = match self.locks.get(name) {
            Some(l) => l.clone(),
            None => return vec![],
        };

        let (new_lock, effects) = lock.transition(LockInput::Release { holder }, clock);
        self.locks.insert(name.to_string(), new_lock);

        effects
    }

    /// Refresh lock heartbeat
    pub fn heartbeat_lock(&mut self, name: &str, holder: HolderId, clock: &impl Clock) {
        if let Some(lock) = self.locks.get(name).cloned() {
            let (new_lock, _) = lock.transition(LockInput::Heartbeat { holder }, clock);
            self.locks.insert(name.to_string(), new_lock);
        }
    }

    // === Semaphore Operations ===

    /// Create or get a semaphore
    pub fn ensure_semaphore(&mut self, config: SemaphoreConfig) -> &Semaphore {
        self.semaphores
            .entry(config.name.clone())
            .or_insert_with(|| Semaphore::new(config))
    }

    /// Get a semaphore by name
    pub fn get_semaphore(&self, name: &str) -> Option<&Semaphore> {
        self.semaphores.get(name)
    }

    /// Acquire semaphore slots
    pub fn acquire_semaphore(
        &mut self,
        name: &str,
        holder_id: String,
        weight: u32,
        metadata: Option<String>,
        clock: &impl Clock,
    ) -> (bool, Vec<Effect>) {
        let semaphore = match self.semaphores.get(name) {
            Some(s) => s.clone(),
            None => {
                // Auto-create with requested capacity
                Semaphore::new(SemaphoreConfig::new(name, weight))
            }
        };

        let (new_sem, effects) = semaphore.transition(
            SemaphoreInput::Acquire {
                holder_id: holder_id.clone(),
                weight,
                metadata,
            },
            clock,
        );

        let acquired = new_sem.holders.contains_key(&holder_id);
        self.semaphores.insert(name.to_string(), new_sem);

        (acquired, effects)
    }

    /// Release semaphore slots
    pub fn release_semaphore(
        &mut self,
        name: &str,
        holder_id: String,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let semaphore = match self.semaphores.get(name) {
            Some(s) => s.clone(),
            None => return vec![],
        };

        let (new_sem, effects) = semaphore.transition(SemaphoreInput::Release { holder_id }, clock);
        self.semaphores.insert(name.to_string(), new_sem);

        effects
    }

    /// Refresh semaphore heartbeat
    pub fn heartbeat_semaphore(&mut self, name: &str, holder_id: String, clock: &impl Clock) {
        if let Some(sem) = self.semaphores.get(name).cloned() {
            let (new_sem, _) = sem.transition(SemaphoreInput::Heartbeat { holder_id }, clock);
            self.semaphores.insert(name.to_string(), new_sem);
        }
    }

    // === Guard Operations ===

    /// Register a guard
    pub fn register_guard(&mut self, guard: RegisteredGuard) {
        self.guards.insert(guard.id.clone(), guard);
    }

    /// Get a guard by ID
    pub fn get_guard(&self, id: &str) -> Option<&RegisteredGuard> {
        self.guards.get(id)
    }

    /// Remove a guard
    pub fn unregister_guard(&mut self, id: &str) -> Option<RegisteredGuard> {
        self.guards.remove(id)
    }

    /// List all registered guard IDs
    pub fn guard_ids(&self) -> Vec<String> {
        self.guards.keys().cloned().collect()
    }

    /// Get guards that should wake on a given event pattern
    pub fn guards_for_event(&self, event_name: &str) -> Vec<&RegisteredGuard> {
        self.guards
            .values()
            .filter(|g| {
                g.wake_on
                    .iter()
                    .any(|p| event_matches_pattern(event_name, p))
            })
            .collect()
    }

    /// Build guard inputs from current coordination state
    pub fn build_coordination_inputs(&self) -> GuardInputs {
        let mut inputs = GuardInputs::default();

        // Add lock states
        for (name, lock) in &self.locks {
            inputs.locks.insert(name.clone(), lock.is_free());
            if let Some(holder) = lock.holder() {
                inputs.lock_holders.insert(name.clone(), holder.0.clone());
            }
        }

        // Add semaphore availability
        for (name, sem) in &self.semaphores {
            inputs
                .semaphores
                .insert(name.clone(), sem.available_slots());
        }

        inputs
    }

    /// Evaluate a guard condition with current coordination state
    pub fn evaluate_guard(&self, condition: &GuardCondition) -> GuardResult {
        let inputs = self.build_coordination_inputs();
        condition.evaluate(&inputs)
    }

    // === Maintenance ===

    /// Run periodic maintenance (check for stale holders)
    pub fn tick(&mut self, clock: &impl Clock) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Tick all locks
        for lock in self.locks.values() {
            let (_, lock_effects) = lock.transition(LockInput::Tick, clock);
            effects.extend(lock_effects);
        }

        // Tick all semaphores
        for sem in self.semaphores.values() {
            let (_, sem_effects) = sem.transition(SemaphoreInput::Tick, clock);
            effects.extend(sem_effects);
        }

        effects
    }

    /// Reclaim all stale resources
    pub fn reclaim_stale(&mut self, clock: &impl Clock) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Reclaim stale locks
        let lock_names: Vec<_> = self.locks.keys().cloned().collect();
        for name in lock_names {
            if let Some(lock) = self.locks.get(&name).cloned() {
                if lock.is_stale(clock) {
                    if let Some(holder) = lock.holder() {
                        // Force release
                        let (new_lock, release_effects) = lock.transition(
                            LockInput::Release {
                                holder: holder.clone(),
                            },
                            clock,
                        );
                        self.locks.insert(name, new_lock);
                        effects.extend(release_effects);
                    }
                }
            }
        }

        // Reclaim stale semaphore holders
        let sem_names: Vec<_> = self.semaphores.keys().cloned().collect();
        for name in sem_names {
            if let Some(sem) = self.semaphores.get(&name).cloned() {
                for holder_id in sem.stale_holders(clock) {
                    let (new_sem, release_effects) =
                        sem.transition(SemaphoreInput::Release { holder_id }, clock);
                    self.semaphores.insert(name.clone(), new_sem);
                    effects.extend(release_effects);
                }
            }
        }

        effects
    }

    /// Get all lock names
    pub fn lock_names(&self) -> Vec<String> {
        self.locks.keys().cloned().collect()
    }

    /// Get all semaphore names
    pub fn semaphore_names(&self) -> Vec<String> {
        self.semaphores.keys().cloned().collect()
    }
}

/// Check if an event name matches a pattern
fn event_matches_pattern(event_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.ends_with(':') || pattern.ends_with("*") {
        // Prefix match: "lock:" matches "lock:acquired", "lock:released", etc.
        let prefix = pattern.trim_end_matches('*').trim_end_matches(':');
        return event_name.starts_with(prefix);
    }

    event_name == pattern
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

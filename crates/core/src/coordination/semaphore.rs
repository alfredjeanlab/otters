// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Semaphore state machine for multi-holder resource limiting
//!
//! Provides a semaphore with weighted slots and heartbeat-based stale detection.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Semaphore configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemaphoreConfig {
    /// Name identifying this semaphore
    pub name: String,
    /// Total available slots
    pub max_slots: u32,
    /// How long before a holder is considered stale
    #[serde(with = "humantime_serde")]
    pub stale_threshold: Duration,
}

impl SemaphoreConfig {
    pub fn new(name: impl Into<String>, max_slots: u32) -> Self {
        Self {
            name: name.into(),
            max_slots,
            stale_threshold: Duration::from_secs(60),
        }
    }

    pub fn with_stale_threshold(mut self, threshold: Duration) -> Self {
        self.stale_threshold = threshold;
        self
    }
}

/// Information about a semaphore holder
#[derive(Clone, Debug)]
pub struct SemaphoreHolder {
    pub holder_id: String,
    pub weight: u32,
    pub metadata: Option<String>,
    pub last_heartbeat: Instant,
}

/// Semaphore state machine
#[derive(Clone, Debug)]
pub struct Semaphore {
    pub config: SemaphoreConfig,
    /// Current holders and their weights
    pub holders: HashMap<String, SemaphoreHolder>,
}

/// Events that trigger semaphore transitions
#[derive(Clone, Debug)]
pub enum SemaphoreInput {
    /// Acquire slots
    Acquire {
        holder_id: String,
        weight: u32,
        metadata: Option<String>,
    },
    /// Release slots
    Release { holder_id: String },
    /// Refresh heartbeat
    Heartbeat { holder_id: String },
    /// Check for stale holders (called periodically)
    Tick,
}

impl Semaphore {
    pub fn new(config: SemaphoreConfig) -> Self {
        Self {
            config,
            holders: HashMap::new(),
        }
    }

    /// Get currently used slots
    pub fn used_slots(&self) -> u32 {
        self.holders.values().map(|h| h.weight).sum()
    }

    /// Get available slots
    pub fn available_slots(&self) -> u32 {
        self.config.max_slots.saturating_sub(self.used_slots())
    }

    /// Check if there's room for the requested weight
    pub fn can_acquire(&self, weight: u32) -> bool {
        self.available_slots() >= weight
    }

    /// Check if a holder is stale
    pub fn is_holder_stale(&self, holder_id: &str, clock: &impl Clock) -> bool {
        self.holders.get(holder_id).is_some_and(|h| {
            clock.now().duration_since(h.last_heartbeat) > self.config.stale_threshold
        })
    }

    /// Get all stale holders
    pub fn stale_holders(&self, clock: &impl Clock) -> Vec<String> {
        self.holders
            .iter()
            .filter(|(_, h)| {
                clock.now().duration_since(h.last_heartbeat) > self.config.stale_threshold
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Pure state transition function
    pub fn transition(
        &self,
        input: SemaphoreInput,
        clock: &impl Clock,
    ) -> (Semaphore, Vec<Effect>) {
        let mut new_sem = self.clone();
        let mut effects = Vec::new();

        match input {
            SemaphoreInput::Acquire {
                holder_id,
                weight,
                metadata,
            } => {
                // First, reclaim any stale slots to make room
                let stale = new_sem.stale_holders(clock);
                for stale_id in stale {
                    if let Some(holder) = new_sem.holders.remove(&stale_id) {
                        effects.push(Effect::Emit(Event::SemaphoreReclaimed {
                            name: self.config.name.clone(),
                            holder_id: stale_id,
                            weight: holder.weight,
                        }));
                    }
                }

                // Check if we can acquire
                if new_sem.can_acquire(weight) {
                    new_sem.holders.insert(
                        holder_id.clone(),
                        SemaphoreHolder {
                            holder_id: holder_id.clone(),
                            weight,
                            metadata: metadata.clone(),
                            last_heartbeat: clock.now(),
                        },
                    );
                    effects.push(Effect::Emit(Event::SemaphoreAcquired {
                        name: self.config.name.clone(),
                        holder_id,
                        weight,
                        metadata,
                        available: new_sem.available_slots(),
                    }));
                } else {
                    effects.push(Effect::Emit(Event::SemaphoreDenied {
                        name: self.config.name.clone(),
                        holder_id,
                        requested: weight,
                        available: new_sem.available_slots(),
                    }));
                }
            }

            SemaphoreInput::Release { holder_id } => {
                if let Some(holder) = new_sem.holders.remove(&holder_id) {
                    effects.push(Effect::Emit(Event::SemaphoreReleased {
                        name: self.config.name.clone(),
                        holder_id,
                        weight: holder.weight,
                        available: new_sem.available_slots(),
                    }));
                }
            }

            SemaphoreInput::Heartbeat { holder_id } => {
                if let Some(holder) = new_sem.holders.get_mut(&holder_id) {
                    holder.last_heartbeat = clock.now();
                }
            }

            SemaphoreInput::Tick => {
                // Emit warnings for stale holders
                for holder_id in self.stale_holders(clock) {
                    if let Some(holder) = self.holders.get(&holder_id) {
                        effects.push(Effect::Emit(Event::SemaphoreHolderStale {
                            name: self.config.name.clone(),
                            holder_id,
                            weight: holder.weight,
                        }));
                    }
                }
            }
        }

        (new_sem, effects)
    }
}

#[cfg(test)]
#[path = "semaphore_tests.rs"]
mod tests;

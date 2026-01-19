// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Lock state machine for exclusive resource access
//!
//! Provides a distributed lock with heartbeat-based stale detection and automatic reclaim.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Unique identifier for a lock holder
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HolderId(pub String);

impl HolderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for HolderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lock configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockConfig {
    /// Name identifying this lock
    pub name: String,
    /// How long before a holder is considered stale
    #[serde(with = "humantime_serde")]
    pub stale_threshold: Duration,
    /// How often holders should refresh their heartbeat
    #[serde(with = "humantime_serde")]
    pub heartbeat_interval: Duration,
}

impl LockConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stale_threshold: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(15),
        }
    }

    pub fn with_stale_threshold(mut self, threshold: Duration) -> Self {
        self.stale_threshold = threshold;
        self
    }

    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }
}

/// Lock state
#[derive(Clone, Debug)]
pub enum LockState {
    /// Lock is available
    Free,
    /// Lock is held by a holder
    Held {
        holder: HolderId,
        /// Metadata about the holder (e.g., operation ID, pipeline ID)
        metadata: Option<String>,
        /// Last heartbeat timestamp
        last_heartbeat: Instant,
    },
}

/// A distributed lock with heartbeat-based stale detection
#[derive(Clone, Debug)]
pub struct Lock {
    pub config: LockConfig,
    pub state: LockState,
}

/// Events that can trigger lock transitions
#[derive(Clone, Debug)]
pub enum LockInput {
    /// Attempt to acquire the lock
    Acquire {
        holder: HolderId,
        metadata: Option<String>,
    },
    /// Release the lock
    Release { holder: HolderId },
    /// Refresh heartbeat
    Heartbeat { holder: HolderId },
    /// Check for and reclaim stale locks (called periodically)
    Tick,
}

impl Lock {
    pub fn new(config: LockConfig) -> Self {
        Self {
            config,
            state: LockState::Free,
        }
    }

    /// Check if the lock is currently free
    pub fn is_free(&self) -> bool {
        matches!(self.state, LockState::Free)
    }

    /// Check if the lock is held by a specific holder
    pub fn is_held_by(&self, holder: &HolderId) -> bool {
        matches!(&self.state, LockState::Held { holder: h, .. } if h == holder)
    }

    /// Get the current holder, if any
    pub fn holder(&self) -> Option<&HolderId> {
        match &self.state {
            LockState::Free => None,
            LockState::Held { holder, .. } => Some(holder),
        }
    }

    /// Check if the current holder is stale
    pub fn is_stale(&self, clock: &impl Clock) -> bool {
        match &self.state {
            LockState::Free => false,
            LockState::Held { last_heartbeat, .. } => {
                clock.now().duration_since(*last_heartbeat) > self.config.stale_threshold
            }
        }
    }

    /// Pure state transition function
    pub fn transition(&self, input: LockInput, clock: &impl Clock) -> (Lock, Vec<Effect>) {
        let mut new_lock = self.clone();
        let mut effects = Vec::new();

        match input {
            LockInput::Acquire { holder, metadata } => {
                match &self.state {
                    LockState::Free => {
                        // Lock is free, grant it
                        new_lock.state = LockState::Held {
                            holder: holder.clone(),
                            metadata: metadata.clone(),
                            last_heartbeat: clock.now(),
                        };
                        effects.push(Effect::Emit(Event::LockAcquired {
                            name: self.config.name.clone(),
                            holder: holder.0.clone(),
                            metadata,
                        }));
                    }
                    LockState::Held {
                        holder: current, ..
                    } => {
                        // Lock is held, check if stale
                        if self.is_stale(clock) {
                            // Reclaim stale lock
                            let previous = current.clone();
                            new_lock.state = LockState::Held {
                                holder: holder.clone(),
                                metadata: metadata.clone(),
                                last_heartbeat: clock.now(),
                            };
                            effects.push(Effect::Emit(Event::LockReclaimed {
                                name: self.config.name.clone(),
                                previous_holder: previous.0,
                                new_holder: holder.0.clone(),
                            }));
                            effects.push(Effect::Emit(Event::LockAcquired {
                                name: self.config.name.clone(),
                                holder: holder.0.clone(),
                                metadata,
                            }));
                        } else {
                            // Lock is held and not stale, acquisition fails
                            effects.push(Effect::Emit(Event::LockDenied {
                                name: self.config.name.clone(),
                                holder: holder.0.clone(),
                                current_holder: current.0.clone(),
                            }));
                        }
                    }
                }
            }

            LockInput::Release { holder } => {
                match &self.state {
                    LockState::Held {
                        holder: current, ..
                    } if current == &holder => {
                        // Holder matches, release the lock
                        new_lock.state = LockState::Free;
                        effects.push(Effect::Emit(Event::LockReleased {
                            name: self.config.name.clone(),
                            holder: holder.0.clone(),
                        }));
                    }
                    _ => {
                        // Wrong holder or already free, no-op
                    }
                }
            }

            LockInput::Heartbeat { holder } => {
                match &self.state {
                    LockState::Held {
                        holder: current,
                        metadata,
                        ..
                    } if current == &holder => {
                        // Refresh heartbeat
                        new_lock.state = LockState::Held {
                            holder: current.clone(),
                            metadata: metadata.clone(),
                            last_heartbeat: clock.now(),
                        };
                    }
                    _ => {
                        // Not the holder, ignore
                    }
                }
            }

            LockInput::Tick => {
                // Check for stale holder and emit warning event
                if self.is_stale(clock) {
                    if let LockState::Held { holder, .. } = &self.state {
                        effects.push(Effect::Emit(Event::LockStale {
                            name: self.config.name.clone(),
                            holder: holder.0.clone(),
                        }));
                    }
                }
            }
        }

        (new_lock, effects)
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;

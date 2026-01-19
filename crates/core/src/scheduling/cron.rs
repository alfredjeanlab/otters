// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Cron state machine for scheduled task execution
//!
//! A Cron is a named scheduled task that runs at fixed intervals.
//! It supports enable/disable, tracks execution history, and prevents
//! overlapping executions.

use super::{ScannerId, WatcherId};
use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Unique identifier for a cron job
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CronId(pub String);

impl CronId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for CronId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CronId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CronId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Configuration for creating a cron job
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CronConfig {
    pub name: String,
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
    #[serde(default)]
    pub enabled: bool,
    /// Watchers to run on each cron tick
    #[serde(default)]
    pub watchers: Vec<WatcherId>,
    /// Scanners to run on each cron tick
    #[serde(default)]
    pub scanners: Vec<ScannerId>,
}

impl CronConfig {
    pub fn new(name: impl Into<String>, interval: Duration) -> Self {
        Self {
            name: name.into(),
            interval,
            enabled: false,
            watchers: vec![],
            scanners: vec![],
        }
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    pub fn with_watchers(mut self, watchers: Vec<WatcherId>) -> Self {
        self.watchers = watchers;
        self
    }

    pub fn with_scanners(mut self, scanners: Vec<ScannerId>) -> Self {
        self.scanners = scanners;
        self
    }
}

/// A named scheduled task that runs at fixed intervals
#[derive(Debug, Clone)]
pub struct Cron {
    pub id: CronId,
    pub name: String,
    pub interval: Duration,
    pub state: CronState,
    pub last_run: Option<Instant>,
    pub next_run: Option<Instant>,
    pub run_count: u64,
    /// Watchers to run on each cron tick
    pub watchers: Vec<WatcherId>,
    /// Scanners to run on each cron tick
    pub scanners: Vec<ScannerId>,
}

/// The current state of a cron job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronState {
    /// Cron is active and will fire on schedule
    Enabled,
    /// Cron is paused and will not fire
    Disabled,
    /// Cron is currently executing (prevents overlap)
    Running,
}

impl fmt::Display for CronState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronState::Enabled => write!(f, "enabled"),
            CronState::Disabled => write!(f, "disabled"),
            CronState::Running => write!(f, "running"),
        }
    }
}

impl std::str::FromStr for CronState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "enabled" => Ok(CronState::Enabled),
            "disabled" => Ok(CronState::Disabled),
            "running" => Ok(CronState::Running),
            _ => Err(format!("unknown cron state: {}", s)),
        }
    }
}

/// Events that can transition a cron's state
#[derive(Debug, Clone)]
pub enum CronEvent {
    /// Enable a disabled cron
    Enable,
    /// Disable an enabled cron
    Disable,
    /// Timer fired, start execution
    Tick,
    /// Execution completed successfully
    Complete,
    /// Execution failed
    Fail { error: String },
}

impl Cron {
    /// Create a new cron job
    pub fn new(id: CronId, config: CronConfig, clock: &impl Clock) -> Self {
        let state = if config.enabled {
            CronState::Enabled
        } else {
            CronState::Disabled
        };
        let next_run = if config.enabled {
            Some(clock.now() + config.interval)
        } else {
            None
        };

        Self {
            id,
            name: config.name,
            interval: config.interval,
            state,
            last_run: None,
            next_run,
            run_count: 0,
            watchers: config.watchers,
            scanners: config.scanners,
        }
    }

    /// Get the timer ID for this cron (used for scheduling)
    pub fn timer_id(&self) -> String {
        format!("cron:{}", self.id)
    }

    /// Pure state transition returning new state and effects
    pub fn transition(&self, event: CronEvent, clock: &impl Clock) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            // Enable a disabled cron
            (CronState::Disabled, CronEvent::Enable) => {
                let next_run = clock.now() + self.interval;
                let new_state = Cron {
                    state: CronState::Enabled,
                    next_run: Some(next_run),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.interval,
                    },
                    Effect::Emit(Event::CronEnabled {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Disable an enabled cron
            (CronState::Enabled, CronEvent::Disable) => {
                let new_state = Cron {
                    state: CronState::Disabled,
                    next_run: None,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::CancelTimer {
                        id: self.timer_id(),
                    },
                    Effect::Emit(Event::CronDisabled {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Disable a running cron (will stop after current execution)
            (CronState::Running, CronEvent::Disable) => {
                // Mark as disabled but don't cancel - let current execution complete
                // It will transition to Disabled after Complete/Fail
                let new_state = Cron {
                    state: CronState::Disabled,
                    next_run: None,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::CancelTimer {
                        id: self.timer_id(),
                    },
                    Effect::Emit(Event::CronDisabled {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Timer fired while enabled, start execution
            (CronState::Enabled, CronEvent::Tick) => {
                let new_state = Cron {
                    state: CronState::Running,
                    last_run: Some(clock.now()),
                    next_run: None,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::CronTriggered {
                    id: self.id.0.clone(),
                })];
                (new_state, effects)
            }

            // Execution completed successfully
            (CronState::Running, CronEvent::Complete) => {
                let next_run = clock.now() + self.interval;
                let new_state = Cron {
                    state: CronState::Enabled,
                    next_run: Some(next_run),
                    run_count: self.run_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.interval,
                    },
                    Effect::Emit(Event::CronCompleted {
                        id: self.id.0.clone(),
                        run_count: new_state.run_count,
                    }),
                ];
                (new_state, effects)
            }

            // Execution failed
            (CronState::Running, CronEvent::Fail { error }) => {
                let next_run = clock.now() + self.interval;
                let new_state = Cron {
                    state: CronState::Enabled,
                    next_run: Some(next_run),
                    run_count: self.run_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.interval,
                    },
                    Effect::Emit(Event::CronFailed {
                        id: self.id.0.clone(),
                        error,
                    }),
                ];
                (new_state, effects)
            }

            // Completion/failure when already disabled (don't reschedule)
            (CronState::Disabled, CronEvent::Complete)
            | (CronState::Disabled, CronEvent::Fail { .. }) => {
                // Already disabled, just stay disabled
                (self.clone(), vec![])
            }

            // Invalid transitions are no-ops
            _ => (self.clone(), vec![]),
        }
    }

    /// Check if the cron is in a terminal state
    pub fn is_active(&self) -> bool {
        matches!(self.state, CronState::Enabled | CronState::Running)
    }
}

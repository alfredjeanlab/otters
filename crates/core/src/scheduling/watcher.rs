// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Watcher state machine for condition monitoring with response chains
//!
//! A Watcher monitors a source for a condition and triggers a response chain
//! when the condition is met. If the first response fails, it escalates to
//! the next response in the chain.

use super::ActionId;
use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Unique identifier for a watcher
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatcherId(pub String);

impl WatcherId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for WatcherId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for WatcherId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for WatcherId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// What the watcher monitors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherSource {
    /// Monitor a specific task's state
    Task { id: String },
    /// Monitor a pipeline's progress
    Pipeline { id: String },
    /// Monitor a session's output
    Session { name: String },
    /// Monitor an event stream pattern
    Events { pattern: String },
    /// Monitor a queue's depth
    Queue { name: String },
    /// Custom source with shell command
    Command { command: String },
    /// Monitor a file's contents
    File { path: String },
    /// Monitor an HTTP endpoint
    Http { url: String },
}

/// When the watcher triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherCondition {
    /// Source hasn't produced output in duration
    Idle {
        #[serde(with = "humantime_serde")]
        threshold: Duration,
    },
    /// Source matches a pattern
    Matches { pattern: String },
    /// Source value exceeds threshold
    Exceeds { threshold: u64 },
    /// Source has been in state for duration
    StuckInState {
        state: String,
        #[serde(with = "humantime_serde")]
        threshold: Duration,
    },
    /// Consecutive check failures
    ConsecutiveFailures { count: u32 },
}

/// What happens when condition is met
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherResponse {
    /// Action to trigger
    pub action: ActionId,
    /// Delay before triggering (0 for immediate)
    #[serde(default, with = "humantime_serde")]
    pub delay: Duration,
    /// Only trigger if previous response failed
    #[serde(default)]
    pub requires_previous_failure: bool,
}

impl WatcherResponse {
    pub fn new(action: ActionId) -> Self {
        Self {
            action,
            delay: Duration::ZERO,
            requires_previous_failure: false,
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn requires_previous_failure(mut self) -> Self {
        self.requires_previous_failure = true;
        self
    }
}

/// Configuration for creating a watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub name: String,
    pub source: WatcherSource,
    pub condition: WatcherCondition,
    pub response_chain: Vec<WatcherResponse>,
    #[serde(with = "humantime_serde")]
    pub check_interval: Duration,
    /// Event patterns that trigger immediate check (bypassing timer)
    #[serde(default)]
    pub wake_on: Vec<String>,
}

impl WatcherConfig {
    pub fn new(
        name: impl Into<String>,
        source: WatcherSource,
        condition: WatcherCondition,
        check_interval: Duration,
    ) -> Self {
        Self {
            name: name.into(),
            source,
            condition,
            response_chain: vec![],
            check_interval,
            wake_on: vec![],
        }
    }

    pub fn with_response(mut self, response: WatcherResponse) -> Self {
        self.response_chain.push(response);
        self
    }

    pub fn with_responses(mut self, responses: Vec<WatcherResponse>) -> Self {
        self.response_chain = responses;
        self
    }

    pub fn with_wake_on(mut self, patterns: Vec<String>) -> Self {
        self.wake_on = patterns;
        self
    }
}

/// The current state of a watcher
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherState {
    /// Actively monitoring
    Active,
    /// Condition met, executing response chain
    Triggered { response_index: usize },
    /// Waiting for response delay
    WaitingForResponse { response_index: usize },
    /// Paused monitoring
    Paused,
}

impl fmt::Display for WatcherState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WatcherState::Active => write!(f, "active"),
            WatcherState::Triggered { response_index } => {
                write!(f, "triggered:{}", response_index)
            }
            WatcherState::WaitingForResponse { response_index } => {
                write!(f, "waiting:{}", response_index)
            }
            WatcherState::Paused => write!(f, "paused"),
        }
    }
}

impl WatcherState {
    pub fn from_string(s: &str) -> Self {
        if s == "active" {
            WatcherState::Active
        } else if s == "paused" {
            WatcherState::Paused
        } else if let Some(idx) = s.strip_prefix("triggered:") {
            WatcherState::Triggered {
                response_index: idx.parse().unwrap_or(0),
            }
        } else if let Some(idx) = s.strip_prefix("waiting:") {
            WatcherState::WaitingForResponse {
                response_index: idx.parse().unwrap_or(0),
            }
        } else {
            WatcherState::Active
        }
    }
}

/// Value from checking a watcher source
#[derive(Debug, Clone, PartialEq)]
pub enum SourceValue {
    /// Source has been idle for this duration
    Idle { duration: Duration },
    /// Source produced this text output
    Text { value: String },
    /// Source has this numeric value
    Numeric { value: i64 },
    /// Source has this boolean value
    Boolean { value: bool },
    /// Source is in this state for this duration
    State { state: String, duration: Duration },
    /// Event count matching a pattern
    EventCount { count: usize },
    /// Task state with optional phase
    TaskState {
        state: String,
        phase: Option<String>,
    },
    /// Check failed
    Error { message: String },
}

/// Events that can transition a watcher's state
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// Check the source and evaluate condition
    Check { value: SourceValue },
    /// Response completed successfully
    ResponseSucceeded,
    /// Response failed
    ResponseFailed,
    /// Response delay timer fired
    ResponseDelayExpired,
    /// Pause the watcher
    Pause,
    /// Resume the watcher
    Resume,
}

/// A watcher that monitors a condition and triggers responses
#[derive(Debug, Clone)]
pub struct Watcher {
    pub id: WatcherId,
    pub name: String,
    pub source: WatcherSource,
    pub condition: WatcherCondition,
    pub response_chain: Vec<WatcherResponse>,
    pub state: WatcherState,
    pub check_interval: Duration,
    pub consecutive_triggers: u32,
    pub last_check: Option<Instant>,
    /// Event patterns that trigger immediate check
    pub wake_on: Vec<String>,
}

impl Watcher {
    /// Create a new watcher
    pub fn new(id: WatcherId, config: WatcherConfig) -> Self {
        Self {
            id,
            name: config.name,
            source: config.source,
            condition: config.condition,
            response_chain: config.response_chain,
            state: WatcherState::Active,
            check_interval: config.check_interval,
            consecutive_triggers: 0,
            last_check: None,
            wake_on: config.wake_on,
        }
    }

    /// Get the timer ID for this watcher's check interval
    pub fn check_timer_id(&self) -> String {
        format!("watcher:{}:check", self.id)
    }

    /// Get the timer ID for response delays
    pub fn response_timer_id(&self) -> String {
        format!("watcher:{}:response", self.id)
    }

    /// Evaluate if the condition is met
    fn evaluate_condition(&self, value: &SourceValue) -> bool {
        match (&self.condition, value) {
            (WatcherCondition::Idle { threshold }, SourceValue::Idle { duration }) => {
                duration >= threshold
            }
            (WatcherCondition::Matches { pattern }, SourceValue::Text { value }) => {
                value.contains(pattern)
            }
            (WatcherCondition::Exceeds { threshold }, SourceValue::Numeric { value }) => {
                *value > *threshold as i64
            }
            (WatcherCondition::Exceeds { threshold }, SourceValue::EventCount { count }) => {
                *count > *threshold as usize
            }
            (
                WatcherCondition::StuckInState {
                    state: expected,
                    threshold,
                },
                SourceValue::State { state, duration },
            ) => state == expected && duration >= threshold,
            (WatcherCondition::ConsecutiveFailures { count }, SourceValue::Error { .. }) => {
                self.consecutive_triggers + 1 >= *count
            }
            _ => false,
        }
    }

    /// Pure state transition returning new state and effects
    pub fn transition(&self, event: WatcherEvent, clock: &impl Clock) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            // Check while active
            (WatcherState::Active, WatcherEvent::Check { value }) => {
                let condition_met = self.evaluate_condition(&value);
                let is_error = matches!(value, SourceValue::Error { .. });

                if condition_met {
                    let consecutive = self.consecutive_triggers + 1;

                    if let Some(resp) = self.response_chain.first() {
                        if resp.delay.is_zero() {
                            // Immediate response
                            let new_state = Watcher {
                                state: WatcherState::Triggered { response_index: 0 },
                                consecutive_triggers: consecutive,
                                last_check: Some(clock.now()),
                                ..self.clone()
                            };
                            let effects = vec![
                                Effect::Emit(Event::WatcherTriggered {
                                    id: self.id.0.clone(),
                                    consecutive,
                                }),
                                Effect::Emit(Event::ActionTriggered {
                                    id: resp.action.0.clone(),
                                    source: format!("watcher:{}", self.name),
                                }),
                            ];
                            (new_state, effects)
                        } else {
                            // Delayed response
                            let new_state = Watcher {
                                state: WatcherState::WaitingForResponse { response_index: 0 },
                                consecutive_triggers: consecutive,
                                last_check: Some(clock.now()),
                                ..self.clone()
                            };
                            let effects = vec![
                                Effect::Emit(Event::WatcherTriggered {
                                    id: self.id.0.clone(),
                                    consecutive,
                                }),
                                Effect::SetTimer {
                                    id: self.response_timer_id(),
                                    duration: resp.delay,
                                },
                            ];
                            (new_state, effects)
                        }
                    } else {
                        // No responses configured, just emit event
                        let new_state = Watcher {
                            consecutive_triggers: consecutive,
                            last_check: Some(clock.now()),
                            ..self.clone()
                        };
                        let effects = vec![Effect::Emit(Event::WatcherTriggered {
                            id: self.id.0.clone(),
                            consecutive,
                        })];
                        (new_state, effects)
                    }
                } else {
                    // Condition not met
                    let consecutive = if is_error {
                        self.consecutive_triggers + 1
                    } else {
                        0
                    };
                    let new_state = Watcher {
                        consecutive_triggers: consecutive,
                        last_check: Some(clock.now()),
                        ..self.clone()
                    };
                    let effects = vec![Effect::SetTimer {
                        id: self.check_timer_id(),
                        duration: self.check_interval,
                    }];
                    (new_state, effects)
                }
            }

            // Response delay expired - trigger the action
            (
                WatcherState::WaitingForResponse { response_index },
                WatcherEvent::ResponseDelayExpired,
            ) => {
                if let Some(resp) = self.response_chain.get(*response_index) {
                    let new_state = Watcher {
                        state: WatcherState::Triggered {
                            response_index: *response_index,
                        },
                        ..self.clone()
                    };
                    let effects = vec![Effect::Emit(Event::ActionTriggered {
                        id: resp.action.0.clone(),
                        source: format!("watcher:{}", self.name),
                    })];
                    (new_state, effects)
                } else {
                    (self.clone(), vec![])
                }
            }

            // Response succeeded - return to active monitoring
            (WatcherState::Triggered { .. }, WatcherEvent::ResponseSucceeded) => {
                let new_state = Watcher {
                    state: WatcherState::Active,
                    consecutive_triggers: 0,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::WatcherResolved {
                        id: self.id.0.clone(),
                    }),
                    Effect::SetTimer {
                        id: self.check_timer_id(),
                        duration: self.check_interval,
                    },
                ];
                (new_state, effects)
            }

            // Response failed - try next in chain
            (WatcherState::Triggered { response_index }, WatcherEvent::ResponseFailed) => {
                let next_index = response_index + 1;

                if let Some(resp) = self.response_chain.get(next_index) {
                    if resp.requires_previous_failure {
                        if resp.delay.is_zero() {
                            // Immediate next response
                            let new_state = Watcher {
                                state: WatcherState::Triggered {
                                    response_index: next_index,
                                },
                                ..self.clone()
                            };
                            let effects = vec![Effect::Emit(Event::ActionTriggered {
                                id: resp.action.0.clone(),
                                source: format!("watcher:{}", self.name),
                            })];
                            (new_state, effects)
                        } else {
                            // Delayed next response
                            let new_state = Watcher {
                                state: WatcherState::WaitingForResponse {
                                    response_index: next_index,
                                },
                                ..self.clone()
                            };
                            let effects = vec![Effect::SetTimer {
                                id: self.response_timer_id(),
                                duration: resp.delay,
                            }];
                            (new_state, effects)
                        }
                    } else {
                        // Next response doesn't require failure, skip it
                        // This handles when we have conditional responses in the chain
                        self.escalate_or_return(next_index + 1, clock)
                    }
                } else {
                    // Chain exhausted - escalate
                    let new_state = Watcher {
                        state: WatcherState::Active,
                        ..self.clone()
                    };
                    let effects = vec![
                        Effect::Emit(Event::WatcherEscalated {
                            id: self.id.0.clone(),
                        }),
                        Effect::SetTimer {
                            id: self.check_timer_id(),
                            duration: self.check_interval,
                        },
                    ];
                    (new_state, effects)
                }
            }

            // Pause
            (WatcherState::Active, WatcherEvent::Pause) => {
                let new_state = Watcher {
                    state: WatcherState::Paused,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::CancelTimer {
                        id: self.check_timer_id(),
                    },
                    Effect::Emit(Event::WatcherPaused {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Resume
            (WatcherState::Paused, WatcherEvent::Resume) => {
                let new_state = Watcher {
                    state: WatcherState::Active,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.check_timer_id(),
                        duration: self.check_interval,
                    },
                    Effect::Emit(Event::WatcherResumed {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Invalid transitions are no-ops
            _ => (self.clone(), vec![]),
        }
    }

    /// Helper to escalate or return to active state
    fn escalate_or_return(&self, from_index: usize, _clock: &impl Clock) -> (Self, Vec<Effect>) {
        // Check if there are more responses that require previous failure
        for (i, resp) in self.response_chain.iter().enumerate().skip(from_index) {
            if resp.requires_previous_failure {
                if resp.delay.is_zero() {
                    let new_state = Watcher {
                        state: WatcherState::Triggered { response_index: i },
                        ..self.clone()
                    };
                    let effects = vec![Effect::Emit(Event::ActionTriggered {
                        id: resp.action.0.clone(),
                        source: format!("watcher:{}", self.name),
                    })];
                    return (new_state, effects);
                } else {
                    let new_state = Watcher {
                        state: WatcherState::WaitingForResponse { response_index: i },
                        ..self.clone()
                    };
                    let effects = vec![Effect::SetTimer {
                        id: self.response_timer_id(),
                        duration: resp.delay,
                    }];
                    return (new_state, effects);
                }
            }
        }

        // No more responses - escalate
        let new_state = Watcher {
            state: WatcherState::Active,
            ..self.clone()
        };
        let effects = vec![
            Effect::Emit(Event::WatcherEscalated {
                id: self.id.0.clone(),
            }),
            Effect::SetTimer {
                id: self.check_timer_id(),
                duration: self.check_interval,
            },
        ];
        (new_state, effects)
    }

    /// Check if the watcher is active
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            WatcherState::Active
                | WatcherState::Triggered { .. }
                | WatcherState::WaitingForResponse { .. }
        )
    }
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Scanner state machine for resource cleanup
//!
//! A Scanner finds stale resources matching conditions and executes cleanup
//! actions on them. This is useful for cleaning up orphaned locks, stale
//! queue items, dead worktrees, etc.

use super::ActionId;
use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Unique identifier for a scanner
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScannerId(pub String);

impl ScannerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for ScannerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ScannerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ScannerId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// What the scanner looks for
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScannerSource {
    /// Scan locks in coordination
    Locks,
    /// Scan semaphore slots in coordination
    Semaphores,
    /// Scan queue items
    Queue { name: String },
    /// Scan worktrees on disk
    Worktrees,
    /// Scan tasks
    Tasks,
    /// Scan pipelines
    Pipelines,
    /// Scan sessions
    Sessions,
    /// Custom command that outputs resources
    Command { command: String },
}

/// Which resources to clean up
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScannerCondition {
    /// Resource is stale (no heartbeat beyond threshold)
    Stale {
        #[serde(with = "humantime_serde")]
        threshold: Duration,
    },
    /// Resource is in terminal state for too long
    TerminalFor {
        #[serde(with = "humantime_serde")]
        threshold: Duration,
    },
    /// Resource matches pattern
    Matches { pattern: String },
    /// Resource has exceeded max attempts
    ExceededAttempts { max: u32 },
    /// Orphaned (no parent reference)
    Orphaned,
}

/// What to do with matching resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CleanupAction {
    /// Delete the resource
    Delete,
    /// Archive to a secondary location
    Archive { destination: String },
    /// Release (for locks/semaphores)
    Release,
    /// Fail (for queue items)
    Fail { reason: String },
    /// Move to dead letter queue
    DeadLetter,
    /// Custom action
    Custom { action_id: ActionId },
}

/// Configuration for creating a scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub name: String,
    pub source: ScannerSource,
    pub condition: ScannerCondition,
    pub cleanup_action: CleanupAction,
    #[serde(with = "humantime_serde")]
    pub scan_interval: Duration,
}

impl ScannerConfig {
    pub fn new(
        name: impl Into<String>,
        source: ScannerSource,
        condition: ScannerCondition,
        cleanup_action: CleanupAction,
        scan_interval: Duration,
    ) -> Self {
        Self {
            name: name.into(),
            source,
            condition,
            cleanup_action,
            scan_interval,
        }
    }
}

/// A resource that the scanner found
#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub id: String,
    pub age: Option<Duration>,
    pub state: Option<String>,
    pub attempts: Option<u32>,
    pub parent_id: Option<String>,
    pub holder: Option<String>,
    pub metadata: std::collections::BTreeMap<String, String>,
}

impl ResourceInfo {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            age: None,
            state: None,
            attempts: None,
            parent_id: None,
            holder: None,
            metadata: std::collections::BTreeMap::new(),
        }
    }

    pub fn with_age(mut self, age: Duration) -> Self {
        self.age = Some(age);
        self
    }

    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub fn with_attempts(mut self, attempts: u32) -> Self {
        self.attempts = Some(attempts);
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_holder(mut self, holder: impl Into<String>) -> Self {
        self.holder = Some(holder.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn orphaned(mut self) -> Self {
        self.parent_id = None;
        self
    }
}

/// The current state of a scanner
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScannerState {
    /// Waiting for next scan interval
    Idle,
    /// Currently scanning
    Scanning,
    /// Executing cleanup on found items
    Cleaning { item_count: u32 },
}

impl fmt::Display for ScannerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScannerState::Idle => write!(f, "idle"),
            ScannerState::Scanning => write!(f, "scanning"),
            ScannerState::Cleaning { item_count } => write!(f, "cleaning:{}", item_count),
        }
    }
}

impl ScannerState {
    pub fn from_string(s: &str) -> Self {
        if s == "idle" {
            ScannerState::Idle
        } else if s == "scanning" {
            ScannerState::Scanning
        } else if let Some(count) = s.strip_prefix("cleaning:") {
            ScannerState::Cleaning {
                item_count: count.parse().unwrap_or(0),
            }
        } else {
            ScannerState::Idle
        }
    }
}

/// Events that can transition a scanner's state
#[derive(Debug, Clone)]
pub enum ScannerEvent {
    /// Timer fired, start scanning
    Tick,
    /// Scan completed with found resources
    ScanComplete { resources: Vec<ResourceInfo> },
    /// Cleanup completed for a batch of items
    CleanupComplete { count: u64 },
    /// Cleanup failed
    CleanupFailed { error: String },
}

/// A scanner that finds and cleans stale resources
#[derive(Debug, Clone)]
pub struct Scanner {
    pub id: ScannerId,
    pub name: String,
    pub source: ScannerSource,
    pub condition: ScannerCondition,
    pub cleanup_action: CleanupAction,
    pub state: ScannerState,
    pub scan_interval: Duration,
    pub last_scan: Option<Instant>,
    pub total_cleaned: u64,
}

impl Scanner {
    /// Create a new scanner
    pub fn new(id: ScannerId, config: ScannerConfig) -> Self {
        Self {
            id,
            name: config.name,
            source: config.source,
            condition: config.condition,
            cleanup_action: config.cleanup_action,
            state: ScannerState::Idle,
            scan_interval: config.scan_interval,
            last_scan: None,
            total_cleaned: 0,
        }
    }

    /// Get the timer ID for this scanner
    pub fn timer_id(&self) -> String {
        format!("scanner:{}", self.id)
    }

    /// Check if a resource matches the condition
    fn matches_condition(&self, resource: &ResourceInfo) -> bool {
        match &self.condition {
            ScannerCondition::Stale { threshold } => {
                resource.age.is_some_and(|age| age >= *threshold)
            }
            ScannerCondition::TerminalFor { threshold } => {
                let is_terminal = resource
                    .state
                    .as_ref()
                    .is_some_and(|s| s == "done" || s == "failed" || s == "dead");
                is_terminal && resource.age.is_some_and(|age| age >= *threshold)
            }
            ScannerCondition::Matches { pattern } => {
                resource.metadata.values().any(|v| v.contains(pattern))
            }
            ScannerCondition::ExceededAttempts { max } => {
                resource.attempts.is_some_and(|a| a >= *max)
            }
            ScannerCondition::Orphaned => resource.parent_id.is_none(),
        }
    }

    /// Generate cleanup effect for a resource
    fn cleanup_effect(&self, resource_id: &str) -> Effect {
        match &self.cleanup_action {
            CleanupAction::Delete => Effect::Emit(Event::ScannerDeleteResource {
                scanner_id: self.id.0.clone(),
                resource_id: resource_id.to_string(),
            }),
            CleanupAction::Release => Effect::Emit(Event::ScannerReleaseResource {
                scanner_id: self.id.0.clone(),
                resource_id: resource_id.to_string(),
            }),
            CleanupAction::Fail { reason } => Effect::Emit(Event::ScannerFailResource {
                scanner_id: self.id.0.clone(),
                resource_id: resource_id.to_string(),
                reason: reason.clone(),
            }),
            CleanupAction::DeadLetter => Effect::Emit(Event::ScannerDeadLetterResource {
                scanner_id: self.id.0.clone(),
                resource_id: resource_id.to_string(),
            }),
            CleanupAction::Archive { destination } => Effect::Emit(Event::ScannerArchiveResource {
                scanner_id: self.id.0.clone(),
                resource_id: resource_id.to_string(),
                destination: destination.clone(),
            }),
            CleanupAction::Custom { action_id } => Effect::Emit(Event::ActionTriggered {
                id: action_id.0.clone(),
                source: format!("scanner:{}:{}", self.name, resource_id),
            }),
        }
    }

    /// Pure state transition returning new state and effects
    pub fn transition(&self, event: ScannerEvent, clock: &impl Clock) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            // Timer fired while idle - start scanning
            (ScannerState::Idle, ScannerEvent::Tick) => {
                let new_state = Scanner {
                    state: ScannerState::Scanning,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::ScannerStarted {
                    id: self.id.0.clone(),
                })];
                (new_state, effects)
            }

            // Scan completed - filter matching resources and start cleanup
            (ScannerState::Scanning, ScannerEvent::ScanComplete { resources }) => {
                let matching: Vec<&ResourceInfo> = resources
                    .iter()
                    .filter(|r| self.matches_condition(r))
                    .collect();

                if matching.is_empty() {
                    // Nothing to clean - back to idle
                    let new_state = Scanner {
                        state: ScannerState::Idle,
                        last_scan: Some(clock.now()),
                        ..self.clone()
                    };
                    let effects = vec![Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.scan_interval,
                    }];
                    (new_state, effects)
                } else {
                    // Start cleanup
                    let count = matching.len() as u32;
                    let new_state = Scanner {
                        state: ScannerState::Cleaning { item_count: count },
                        last_scan: Some(clock.now()),
                        ..self.clone()
                    };

                    let mut effects: Vec<Effect> = matching
                        .iter()
                        .map(|r| self.cleanup_effect(&r.id))
                        .collect();

                    effects.push(Effect::Emit(Event::ScannerFound {
                        id: self.id.0.clone(),
                        count,
                    }));

                    (new_state, effects)
                }
            }

            // Cleanup completed - update stats and return to idle
            (ScannerState::Cleaning { .. }, ScannerEvent::CleanupComplete { count }) => {
                let new_state = Scanner {
                    state: ScannerState::Idle,
                    total_cleaned: self.total_cleaned + count,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.scan_interval,
                    },
                    Effect::Emit(Event::ScannerCleaned {
                        id: self.id.0.clone(),
                        count,
                        total: new_state.total_cleaned,
                    }),
                ];
                (new_state, effects)
            }

            // Cleanup failed - return to idle anyway
            (ScannerState::Cleaning { .. }, ScannerEvent::CleanupFailed { error }) => {
                let new_state = Scanner {
                    state: ScannerState::Idle,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.timer_id(),
                        duration: self.scan_interval,
                    },
                    Effect::Emit(Event::ScannerFailed {
                        id: self.id.0.clone(),
                        error,
                    }),
                ];
                (new_state, effects)
            }

            // Invalid transitions are no-ops
            _ => (self.clone(), vec![]),
        }
    }

    /// Check if the scanner is actively running
    pub fn is_active(&self) -> bool {
        !matches!(self.state, ScannerState::Idle)
    }
}

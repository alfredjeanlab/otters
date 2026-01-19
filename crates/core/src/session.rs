// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Session state machine
//!
//! A session represents a tmux session running in a workspace.
//!
//! ## Heartbeat Tracking
//!
//! Session owns heartbeat state (`last_heartbeat`) for monitoring activity.
//! This is persisted via the `SessionHeartbeat` WAL operation. Tasks check
//! session liveness via `Session::idle_time()` for stuck detection.
//!
//! The flow is:
//! 1. Engine detects activity (via `poll_sessions`)
//! 2. Engine calls `process_heartbeat(session_id)` which persists to WAL
//! 3. On tick, Engine queries `session.idle_time()` and passes to `TaskEvent::Tick`
//! 4. Task uses `session_idle_time` to determine if stuck

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use crate::workspace::WorkspaceId;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Unique identifier for a session
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why a session died
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathReason {
    Completed,
    Killed,
    Error(String),
    Timeout,
}

/// The state of a session
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Session is starting up
    Starting,
    /// Session is actively running
    Running,
    /// Session is idle (no output for a while)
    Idle { since: Instant },
    /// Session has terminated
    Dead { reason: DeathReason },
}

/// A tmux session
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub state: SessionState,
    pub last_output: Option<Instant>,
    pub last_output_hash: Option<u64>,
    pub idle_threshold: Duration,
    pub created_at: Instant,
    /// Last detected activity (heartbeat) for stuck detection
    pub last_heartbeat: Option<Instant>,
}

impl Session {
    /// Create a new session in the Starting state
    pub fn new(
        id: impl Into<String>,
        workspace_id: WorkspaceId,
        idle_threshold: Duration,
        clock: &impl Clock,
    ) -> Self {
        let now = clock.now();
        Self {
            id: SessionId(id.into()),
            workspace_id,
            state: SessionState::Starting,
            last_output: None,
            last_output_hash: None,
            idle_threshold,
            created_at: now,
            last_heartbeat: None,
        }
    }

    /// Mark the session as running
    pub fn mark_running(&self, clock: &impl Clock) -> (Session, Vec<Effect>) {
        let now = clock.now();
        (
            Session {
                state: SessionState::Running,
                last_output: Some(now),
                ..self.clone()
            },
            vec![Effect::Emit(Event::SessionStarted {
                id: self.id.0.clone(),
                workspace_id: self.workspace_id.0.clone(),
            })],
        )
    }

    /// Mark the session as dead
    pub fn mark_dead(&self, reason: DeathReason) -> (Session, Vec<Effect>) {
        (
            Session {
                state: SessionState::Dead {
                    reason: reason.clone(),
                },
                ..self.clone()
            },
            vec![Effect::Emit(Event::SessionDead {
                id: self.id.0.clone(),
                reason: format!("{:?}", reason),
            })],
        )
    }

    /// Evaluate heartbeat based on output activity
    pub fn evaluate_heartbeat(
        &self,
        output_time: Option<Instant>,
        output_hash: Option<u64>,
        clock: &impl Clock,
    ) -> (Session, Vec<Effect>) {
        let now = clock.now();

        // Check if output changed
        let output_changed = output_hash.is_some() && output_hash != self.last_output_hash;
        let activity_time = if output_changed {
            output_time
        } else {
            self.last_output
        };

        match &self.state {
            SessionState::Running => {
                if let Some(last) = activity_time {
                    if now.duration_since(last) > self.idle_threshold {
                        return (
                            self.with_state(SessionState::Idle { since: now })
                                .with_output(activity_time, output_hash),
                            vec![Effect::Emit(Event::SessionIdle {
                                id: self.id.0.clone(),
                            })],
                        );
                    }
                }
                (self.with_output(activity_time, output_hash), vec![])
            }
            SessionState::Idle { .. } => {
                if output_changed {
                    (
                        self.with_state(SessionState::Running)
                            .with_output(output_time, output_hash),
                        vec![Effect::Emit(Event::SessionActive {
                            id: self.id.0.clone(),
                        })],
                    )
                } else {
                    (self.clone(), vec![])
                }
            }
            _ => (self.clone(), vec![]),
        }
    }

    /// Record a heartbeat (activity detected)
    pub fn record_heartbeat(&self, timestamp: Instant) -> Session {
        Session {
            last_heartbeat: Some(timestamp),
            ..self.clone()
        }
    }

    /// Time since last heartbeat (for stuck detection)
    pub fn idle_time(&self, now: Instant) -> Option<Duration> {
        self.last_heartbeat.map(|hb| now.duration_since(hb))
    }

    /// Check if session is idle beyond threshold (based on heartbeat)
    pub fn is_idle_by_heartbeat(&self, now: Instant) -> bool {
        self.idle_time(now)
            .map(|idle| idle > self.idle_threshold)
            .unwrap_or(false)
    }

    fn with_state(&self, state: SessionState) -> Session {
        Session {
            state,
            ..self.clone()
        }
    }

    fn with_output(&self, last_output: Option<Instant>, output_hash: Option<u64>) -> Session {
        Session {
            last_output,
            last_output_hash: output_hash.or(self.last_output_hash),
            ..self.clone()
        }
    }
}

/// Simple hash function for output comparison
pub fn hash_output(output: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    output.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

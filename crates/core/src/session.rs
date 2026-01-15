// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Session state machine
//!
//! A session represents a tmux session running in a workspace.

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
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    #[test]
    fn session_starts_in_starting_state() {
        let clock = FakeClock::new();
        let session = Session::new(
            "sess-1",
            WorkspaceId("ws-1".to_string()),
            Duration::from_secs(60),
            &clock,
        );
        assert!(matches!(session.state, SessionState::Starting));
    }

    #[test]
    fn session_transitions_to_running() {
        let clock = FakeClock::new();
        let session = Session::new(
            "sess-1",
            WorkspaceId("ws-1".to_string()),
            Duration::from_secs(60),
            &clock,
        );
        let (session, effects) = session.mark_running(&clock);
        assert!(matches!(session.state, SessionState::Running));
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn session_becomes_idle_after_threshold() {
        let clock = FakeClock::new();
        let session = Session::new(
            "sess-1",
            WorkspaceId("ws-1".to_string()),
            Duration::from_secs(60),
            &clock,
        );
        let (session, _) = session.mark_running(&clock);

        // Advance past idle threshold
        clock.advance(Duration::from_secs(120));

        let (session, effects) = session.evaluate_heartbeat(None, None, &clock);
        assert!(matches!(session.state, SessionState::Idle { .. }));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::Emit(Event::SessionIdle { .. }))));
    }

    #[test]
    fn session_recovers_from_idle_on_output() {
        let clock = FakeClock::new();
        let session = Session::new(
            "sess-1",
            WorkspaceId("ws-1".to_string()),
            Duration::from_secs(60),
            &clock,
        );
        let (session, _) = session.mark_running(&clock);

        // Make it idle
        clock.advance(Duration::from_secs(120));
        let (session, _) = session.evaluate_heartbeat(None, None, &clock);
        assert!(matches!(session.state, SessionState::Idle { .. }));

        // New output arrives
        let now = clock.now();
        let (session, effects) = session.evaluate_heartbeat(Some(now), Some(12345), &clock);
        assert!(matches!(session.state, SessionState::Running));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::Emit(Event::SessionActive { .. }))));
    }

    #[test]
    fn hash_output_produces_consistent_hashes() {
        let output = "Hello, world!";
        let hash1 = hash_output(output);
        let hash2 = hash_output(output);
        assert_eq!(hash1, hash2);

        let hash3 = hash_output("Different output");
        assert_ne!(hash1, hash3);
    }
}

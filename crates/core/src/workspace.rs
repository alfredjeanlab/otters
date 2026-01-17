//! Workspace state machine
//!
//! A workspace represents a git worktree where work is performed.

use crate::adapters::SessionId;
use crate::effect::Effect;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unique identifier for a workspace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub String);

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The state of a workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceState {
    /// Workspace is being created
    Creating,
    /// Workspace is ready for use
    Ready,
    /// Workspace is in use by a session
    InUse { session_id: String },
    /// Workspace has uncommitted changes
    Dirty,
    /// Workspace branch is gone from remote
    Stale,
}

/// Events that can change workspace state
#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    SetupComplete,
    SessionStarted { session_id: SessionId },
    SessionEnded { clean: bool },
    BranchGone,
    Remove,
}

/// A workspace where work is performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub state: WorkspaceState,
    pub created_at: DateTime<Utc>,
}

impl Workspace {
    /// Create a new workspace in the Creating state
    pub fn new(id: impl Into<String>, name: impl Into<String>, path: PathBuf, branch: impl Into<String>) -> Self {
        Self {
            id: WorkspaceId(id.into()),
            name: name.into(),
            path,
            branch: branch.into(),
            state: WorkspaceState::Creating,
            created_at: Utc::now(),
        }
    }

    /// Create a new workspace in the Ready state
    pub fn new_ready(id: impl Into<String>, name: impl Into<String>, path: PathBuf, branch: impl Into<String>) -> Self {
        Self {
            id: WorkspaceId(id.into()),
            name: name.into(),
            path,
            branch: branch.into(),
            state: WorkspaceState::Ready,
            created_at: Utc::now(),
        }
    }

    /// Transition the workspace to a new state based on an event
    pub fn transition(&self, event: WorkspaceEvent) -> (Workspace, Vec<Effect>) {
        match (&self.state, event) {
            (WorkspaceState::Creating, WorkspaceEvent::SetupComplete) => {
                (self.with_state(WorkspaceState::Ready), vec![])
            }
            (WorkspaceState::Ready, WorkspaceEvent::SessionStarted { session_id }) => {
                (
                    self.with_state(WorkspaceState::InUse {
                        session_id: session_id.0,
                    }),
                    vec![],
                )
            }
            (WorkspaceState::InUse { .. }, WorkspaceEvent::SessionEnded { clean: true }) => {
                (self.with_state(WorkspaceState::Ready), vec![])
            }
            (WorkspaceState::InUse { .. }, WorkspaceEvent::SessionEnded { clean: false }) => {
                (self.with_state(WorkspaceState::Dirty), vec![])
            }
            (_, WorkspaceEvent::BranchGone) => (self.with_state(WorkspaceState::Stale), vec![]),
            // Invalid transitions are ignored
            _ => (self.clone(), vec![]),
        }
    }

    fn with_state(&self, state: WorkspaceState) -> Workspace {
        Workspace {
            state,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_transitions_from_creating_to_ready() {
        let ws = Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
        let (ws, _) = ws.transition(WorkspaceEvent::SetupComplete);
        assert_eq!(ws.state, WorkspaceState::Ready);
    }

    #[test]
    fn workspace_transitions_from_ready_to_in_use() {
        let ws = Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
        let (ws, _) = ws.transition(WorkspaceEvent::SessionStarted {
            session_id: SessionId("sess-1".to_string()),
        });
        assert_eq!(
            ws.state,
            WorkspaceState::InUse {
                session_id: "sess-1".to_string()
            }
        );
    }

    #[test]
    fn workspace_transitions_from_in_use_to_ready_on_clean_end() {
        let ws = Workspace {
            state: WorkspaceState::InUse {
                session_id: "sess-1".to_string(),
            },
            ..Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x")
        };
        let (ws, _) = ws.transition(WorkspaceEvent::SessionEnded { clean: true });
        assert_eq!(ws.state, WorkspaceState::Ready);
    }

    #[test]
    fn workspace_transitions_from_in_use_to_dirty_on_unclean_end() {
        let ws = Workspace {
            state: WorkspaceState::InUse {
                session_id: "sess-1".to_string(),
            },
            ..Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x")
        };
        let (ws, _) = ws.transition(WorkspaceEvent::SessionEnded { clean: false });
        assert_eq!(ws.state, WorkspaceState::Dirty);
    }

    #[test]
    fn workspace_transitions_to_stale_on_branch_gone() {
        let ws = Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
        let (ws, _) = ws.transition(WorkspaceEvent::BranchGone);
        assert_eq!(ws.state, WorkspaceState::Stale);
    }

    #[test]
    fn invalid_transition_is_ignored() {
        let ws = Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
        let (ws2, effects) = ws.transition(WorkspaceEvent::SessionEnded { clean: true });
        assert_eq!(ws.state, ws2.state);
        assert!(effects.is_empty());
    }
}

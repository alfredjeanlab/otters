// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Effects represent side effects the system needs to perform

use crate::event::Event;
use crate::operation::Operation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Effects that need to be executed by the runtime
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Emit an event for observability
    Emit { event: Event },

    /// Spawn a new session
    Spawn {
        workspace_id: String,
        command: String,
        env: Vec<(String, String)>,
        /// Working directory override (if different from workspace path)
        cwd: Option<PathBuf>,
    },

    /// Send input to an existing session
    Send { session_id: String, input: String },

    /// Kill a session
    Kill { session_id: String },

    /// Add a git worktree
    WorktreeAdd { branch: String, path: PathBuf },

    /// Remove a git worktree
    WorktreeRemove { path: PathBuf },

    /// Set a timer
    SetTimer {
        id: String,
        #[serde(with = "duration_serde")]
        duration: Duration,
    },

    /// Cancel a timer
    CancelTimer { id: String },

    /// Persist an operation to storage
    Persist { operation: Operation },

    /// Execute a shell command
    Shell {
        /// Pipeline this belongs to
        pipeline_id: String,
        /// Phase name
        phase: String,
        /// Command to execute (already interpolated)
        command: String,
        /// Working directory
        cwd: PathBuf,
        /// Environment variables
        env: HashMap<String, String>,
    },

    /// Send a desktop notification
    Notify {
        /// Notification title
        title: String,
        /// Notification message body
        message: String,
    },
}

impl crate::traced::TracedEffect for Effect {
    fn name(&self) -> &'static str {
        match self {
            Effect::Emit { .. } => "emit",
            Effect::Spawn { .. } => "spawn",
            Effect::Send { .. } => "send",
            Effect::Kill { .. } => "kill",
            Effect::WorktreeAdd { .. } => "worktree_add",
            Effect::WorktreeRemove { .. } => "worktree_remove",
            Effect::SetTimer { .. } => "set_timer",
            Effect::CancelTimer { .. } => "cancel_timer",
            Effect::Persist { .. } => "persist",
            Effect::Shell { .. } => "shell",
            Effect::Notify { .. } => "notify",
        }
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        match self {
            Effect::Emit { event } => vec![("event", format!("{:?}", event))],
            Effect::Spawn {
                workspace_id,
                command,
                cwd,
                ..
            } => vec![
                ("workspace_id", workspace_id.clone()),
                ("command", command.clone()),
                (
                    "cwd",
                    cwd.as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
            ],
            Effect::Send { session_id, .. } => vec![("session_id", session_id.clone())],
            Effect::Kill { session_id } => vec![("session_id", session_id.clone())],
            Effect::WorktreeAdd { branch, path } => vec![
                ("branch", branch.clone()),
                ("path", path.display().to_string()),
            ],
            Effect::WorktreeRemove { path } => vec![("path", path.display().to_string())],
            Effect::SetTimer { id, duration } => vec![
                ("timer_id", id.clone()),
                ("duration_ms", duration.as_millis().to_string()),
            ],
            Effect::CancelTimer { id } => vec![("timer_id", id.clone())],
            Effect::Persist { operation } => vec![("operation", format!("{:?}", operation))],
            Effect::Shell {
                pipeline_id,
                phase,
                cwd,
                ..
            } => vec![
                ("pipeline_id", pipeline_id.clone()),
                ("phase", phase.clone()),
                ("cwd", cwd.display().to_string()),
            ],
            Effect::Notify { title, .. } => vec![("title", title.clone())],
        }
    }
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(duration: &Duration, s: S) -> Result<S::Ok, S::Error> {
        duration.as_millis().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let millis = u64::deserialize(d)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
#[path = "effect_tests.rs"]
mod tests;

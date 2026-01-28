// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Event types for the Otter Jobs system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Events that trigger state transitions in the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// External command invocation
    CommandInvoked {
        command: String,
        args: HashMap<String, String>,
    },

    /// Worker wake signal
    WorkerWake { worker: String },

    /// Session started successfully
    SessionStarted { session_id: String },

    /// Session output received
    SessionOutput { session_id: String, output: String },

    /// Session exited
    SessionExited { session_id: String, exit_code: i32 },

    /// Timer fired
    Timer { id: String },

    /// Agent signaled completion
    AgentDone { pipeline_id: String },

    /// Agent encountered an error
    AgentError { pipeline_id: String, error: String },

    /// Shell command completed
    ShellCompleted {
        pipeline_id: String,
        phase: String,
        exit_code: i32,
    },

    /// Custom event for extensibility
    Custom {
        name: String,
        data: serde_json::Value,
    },
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Operations for the write-ahead log

use crate::PhaseStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Operations that can be persisted to the WAL
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Create a new pipeline
    PipelineCreate {
        id: String,
        kind: String,
        name: String,
        inputs: HashMap<String, String>,
        /// Initial phase name from runbook (defaults to "init" for legacy WAL compat)
        #[serde(default = "default_init_phase")]
        initial_phase: String,
    },

    /// Transition a pipeline to a new phase
    PipelineTransition { id: String, phase: String },

    /// Update the status of the current phase
    PhaseStatusUpdate {
        pipeline_id: String,
        status: PhaseStatus,
    },

    /// Delete a pipeline
    PipelineDelete { id: String },

    /// Create a session record
    SessionCreate { id: String, pipeline_id: String },

    /// Delete a session record
    SessionDelete { id: String },

    /// Create a workspace record
    WorkspaceCreate {
        id: String,
        path: PathBuf,
        branch: String,
    },

    /// Delete a workspace record
    WorkspaceDelete { id: String },
}

/// Default phase for legacy WAL entries without initial_phase
fn default_init_phase() -> String {
    "init".to_string()
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;

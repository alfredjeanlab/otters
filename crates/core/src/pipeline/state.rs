// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Pipeline state machine

use super::phase::PhaseStatus;
use crate::clock::Clock;
use crate::effect::Effect;
use crate::event::Event;
use crate::operation::Operation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// A pipeline instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// Current phase name (from runbook definition)
    pub phase: String,
    pub phase_status: PhaseStatus,
    pub inputs: HashMap<String, String>,
    pub workspace_path: Option<PathBuf>,
    pub session_id: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default = "Instant::now")]
    pub phase_started_at: Instant,
    pub error: Option<String>,
}

impl Pipeline {
    /// Create a new pipeline with the given initial phase
    pub fn new(
        id: String,
        name: String,
        kind: String,
        inputs: HashMap<String, String>,
        initial_phase: String,
        clock: &impl Clock,
    ) -> Self {
        let now = clock.now();
        Self {
            id,
            name,
            kind,
            phase: initial_phase,
            phase_status: PhaseStatus::Pending,
            inputs,
            workspace_path: None,
            session_id: None,
            created_at: now,
            phase_started_at: now,
            error: None,
        }
    }

    /// Handle an event and return the new state plus effects
    ///
    /// Note: Phase transitions (determining the next phase) are handled by the runtime
    /// using the runbook definition. This method only handles status updates and failures.
    pub fn transition(&self, event: &Event, _clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        let mut pipeline = self.clone();
        let mut effects = Vec::new();

        match event {
            Event::SessionStarted { session_id } => {
                if pipeline.session_id.as_ref() == Some(session_id) {
                    pipeline.phase_status = PhaseStatus::Running;
                }
            }

            Event::SessionExited {
                session_id,
                exit_code,
            } => {
                if pipeline.session_id.as_ref() == Some(session_id) {
                    if *exit_code == 0 {
                        // Success - mark phase as completed
                        // The runtime will determine the next phase from the runbook
                        pipeline.phase_status = PhaseStatus::Completed;
                        pipeline.session_id = None;
                    } else {
                        // Failure
                        pipeline.phase = "failed".to_string();
                        pipeline.phase_status = PhaseStatus::Failed;
                        pipeline.error = Some(format!("exit code: {}", exit_code));

                        effects.push(Effect::Persist {
                            operation: Operation::PipelineTransition {
                                id: pipeline.id.clone(),
                                phase: "failed".to_string(),
                            },
                        });
                    }
                }
            }

            Event::AgentDone { pipeline_id } => {
                if &pipeline.id == pipeline_id {
                    // Mark phase as completed, runtime handles transition
                    pipeline.phase_status = PhaseStatus::Completed;
                }
            }

            Event::AgentError { pipeline_id, error } => {
                if &pipeline.id == pipeline_id {
                    pipeline.phase = "failed".to_string();
                    pipeline.phase_status = PhaseStatus::Failed;
                    pipeline.error = Some(error.clone());

                    effects.push(Effect::Persist {
                        operation: Operation::PipelineTransition {
                            id: pipeline.id.clone(),
                            phase: "failed".to_string(),
                        },
                    });
                }
            }

            // Timer events handled at runtime level
            Event::Timer { .. } => {}

            // These events are handled elsewhere (by the runtime)
            Event::CommandInvoked { .. }
            | Event::WorkerWake { .. }
            | Event::SessionOutput { .. }
            | Event::ShellCompleted { .. }
            | Event::Custom { .. } => {}
        }

        (pipeline, effects)
    }

    /// Check if the pipeline is in a terminal state
    pub fn is_terminal(&self) -> bool {
        self.phase == "done" || self.phase == "failed"
    }

    /// Set the workspace path
    pub fn with_workspace(mut self, path: PathBuf) -> Self {
        self.workspace_path = Some(path);
        self
    }

    /// Set the session ID
    pub fn with_session(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self.phase_status = PhaseStatus::Running;
        self
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

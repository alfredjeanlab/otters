// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Pipeline state machine
//!
//! A pipeline represents a multi-phase workflow (build or bugfix)
//! with checkpoint and recovery support.

use crate::clock::Clock;
use crate::effect::{Checkpoint, Effect, Event};
use crate::task::TaskId;
use crate::workspace::WorkspaceId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

/// Unique identifier for a pipeline
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PipelineId(pub String);

impl std::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for PipelineId {
    fn from(s: String) -> Self {
        PipelineId(s)
    }
}

impl From<&str> for PipelineId {
    fn from(s: &str) -> Self {
        PipelineId(s.to_string())
    }
}

/// The type of pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineKind {
    /// Dynamic pipeline created from runbook definitions.
    /// Aliases provide backward compatibility for persisted pipelines.
    #[serde(alias = "Build", alias = "Bugfix")]
    Dynamic,
}

/// The current phase of a pipeline
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    // Common phases
    Init,
    Done,
    Failed {
        reason: String,
    },
    Blocked {
        waiting_on: String,
        guard_id: Option<String>,
    },
    // Build phases
    Plan,
    Decompose,
    Execute,
    // Bugfix phases
    Fix,
    Verify,
    // Shared phases
    Merge,
    Cleanup,
}

impl Phase {
    pub fn name(&self) -> &str {
        match self {
            Phase::Init => "init",
            Phase::Done => "done",
            Phase::Failed { .. } => "failed",
            Phase::Blocked { .. } => "blocked",
            Phase::Plan => "plan",
            Phase::Decompose => "decompose",
            Phase::Execute => "execute",
            Phase::Fix => "fix",
            Phase::Verify => "verify",
            Phase::Merge => "merge",
            Phase::Cleanup => "cleanup",
        }
    }

    /// Check if this phase is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, Phase::Done | Phase::Failed { .. })
    }
}

/// Events that can change pipeline state
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// Phase completed successfully
    PhaseComplete,
    /// Phase completed with outputs
    PhaseCompleteWithOutputs { outputs: BTreeMap<String, String> },
    /// Phase failed (non-recoverable)
    PhaseFailed { reason: String },
    /// Phase failed but can be recovered
    PhaseFailedRecoverable { reason: String },
    /// Blocking condition cleared
    Unblocked,
    /// Task assigned to pipeline phase
    TaskAssigned { task_id: TaskId },
    /// Task completed
    TaskComplete {
        task_id: TaskId,
        output: Option<String>,
    },
    /// Task failed
    TaskFailed { task_id: TaskId, reason: String },
    /// Request checkpoint
    RequestCheckpoint,
    /// Restore from checkpoint
    Restore { checkpoint: Checkpoint },
}

/// A pipeline representing a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: PipelineId,
    pub kind: PipelineKind,
    pub name: String,
    pub phase: Phase,
    pub inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub workspace_id: Option<WorkspaceId>,
    pub current_task_id: Option<TaskId>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub checkpoint_sequence: u64,
    #[serde(skip)]
    pub last_checkpoint_at: Option<Instant>,
}

impl Pipeline {
    /// Create a new dynamic pipeline.
    ///
    /// Dynamic pipelines are created from runbook definitions. The actual phases
    /// are determined by the runbook metadata stored in outputs.
    pub fn new_dynamic(
        id: impl Into<String>,
        name: impl Into<String>,
        inputs: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id: PipelineId(id.into()),
            kind: PipelineKind::Dynamic,
            name: name.into(),
            phase: Phase::Init,
            inputs,
            outputs: BTreeMap::new(),
            workspace_id: None,
            current_task_id: None,
            created_at: Utc::now(),
            checkpoint_sequence: 0,
            last_checkpoint_at: None,
        }
    }

    /// Set the workspace for this pipeline
    pub fn with_workspace(self, workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            ..self
        }
    }

    /// Set the current task for this pipeline
    pub fn with_current_task(self, task_id: TaskId) -> Self {
        Self {
            current_task_id: Some(task_id),
            ..self
        }
    }

    /// Get the ordered phases for this pipeline kind.
    ///
    /// For dynamic pipelines, this returns just Init and Done. The actual phase
    /// sequence is determined by runbook metadata stored in outputs.
    fn phase_sequence(&self) -> Vec<Phase> {
        match self.kind {
            PipelineKind::Dynamic => vec![Phase::Init, Phase::Done],
        }
    }

    /// Get the next phase after the current one
    pub fn next_phase(&self) -> Option<Phase> {
        let seq = self.phase_sequence();
        seq.iter()
            .position(|p| p == &self.phase)
            .and_then(|i| seq.get(i + 1))
            .cloned()
    }

    /// Get the first working phase for this pipeline kind (after Init).
    ///
    /// For dynamic pipelines, checks runbook metadata in outputs.
    /// Defaults to Done if no runbook phase is set.
    fn first_working_phase(&self) -> Phase {
        match self.kind {
            PipelineKind::Dynamic => {
                // Dynamic pipelines store their phase in outputs._runbook_phase
                // The phase_from_name will be used after restore; here we default to Done
                self.outputs
                    .get("_runbook_phase")
                    .and_then(|name| self.phase_from_name(name))
                    .unwrap_or(Phase::Done)
            }
        }
    }

    /// Pure transition function - returns new state and effects
    pub fn transition(&self, event: PipelineEvent, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        let now = clock.now();

        match (&self.phase, &event) {
            // Init → first working phase
            (Phase::Init, PipelineEvent::PhaseComplete) => self.advance_to_next_phase(None),
            (Phase::Init, PipelineEvent::PhaseCompleteWithOutputs { outputs }) => {
                self.advance_to_next_phase(Some(outputs.clone()))
            }

            // Working phase → next phase or Done
            (phase, PipelineEvent::PhaseComplete)
                if !phase.is_terminal()
                    && !matches!(phase, Phase::Init | Phase::Blocked { .. }) =>
            {
                self.advance_to_next_phase(None)
            }
            (phase, PipelineEvent::PhaseCompleteWithOutputs { outputs })
                if !phase.is_terminal()
                    && !matches!(phase, Phase::Init | Phase::Blocked { .. }) =>
            {
                self.advance_to_next_phase(Some(outputs.clone()))
            }

            // Working phase → Blocked (recoverable failure)
            (phase, PipelineEvent::PhaseFailedRecoverable { reason }) if !phase.is_terminal() => {
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Blocked {
                    waiting_on: reason.clone(),
                    guard_id: None,
                };

                let effects = vec![Effect::Emit(Event::PipelineBlocked {
                    id: self.id.0.clone(),
                    reason: reason.clone(),
                })];
                (pipeline, effects)
            }

            // Any phase can fail non-recoverably
            (_, PipelineEvent::PhaseFailed { reason }) => {
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Failed {
                    reason: reason.clone(),
                };

                let effects = vec![Effect::Emit(Event::PipelineFailed {
                    id: self.id.0.clone(),
                    reason: reason.clone(),
                })];
                (pipeline, effects)
            }

            // Blocked → resume to first working phase
            (Phase::Blocked { .. }, PipelineEvent::Unblocked) => {
                let resumed_phase = self.first_working_phase();
                let mut pipeline = self.clone();
                pipeline.phase = resumed_phase.clone();

                let effects = vec![Effect::Emit(Event::PipelineResumed {
                    id: self.id.0.clone(),
                    phase: resumed_phase.name().to_string(),
                })];
                (pipeline, effects)
            }

            // Task assignment (only in working phases)
            (phase, PipelineEvent::TaskAssigned { task_id })
                if !phase.is_terminal()
                    && !matches!(phase, Phase::Init | Phase::Blocked { .. }) =>
            {
                let mut pipeline = self.clone();
                pipeline.current_task_id = Some(task_id.clone());
                (pipeline, vec![])
            }

            // Task completion triggers phase completion
            (phase, PipelineEvent::TaskComplete { output, .. })
                if !phase.is_terminal()
                    && !matches!(phase, Phase::Init | Phase::Blocked { .. }) =>
            {
                let outputs = output.clone().map(|o| {
                    let mut map = BTreeMap::new();
                    map.insert("task_output".to_string(), o);
                    map
                });
                self.advance_to_next_phase(outputs)
            }

            // Task failure triggers recoverable phase failure
            (phase, PipelineEvent::TaskFailed { reason, .. }) if !phase.is_terminal() => self
                .transition(
                    PipelineEvent::PhaseFailedRecoverable {
                        reason: reason.clone(),
                    },
                    clock,
                ),

            // Checkpoint request (works from any state)
            (_, PipelineEvent::RequestCheckpoint) => {
                let checkpoint = Checkpoint {
                    pipeline_id: self.id.clone(),
                    phase: self.phase.name().to_string(),
                    inputs: self.inputs.clone(),
                    outputs: self.outputs.clone(),
                    created_at: now,
                    sequence: self.checkpoint_sequence + 1,
                };

                let mut pipeline = self.clone();
                pipeline.checkpoint_sequence += 1;
                pipeline.last_checkpoint_at = Some(now);

                let effects = vec![Effect::SaveCheckpoint {
                    pipeline_id: self.id.clone(),
                    checkpoint,
                }];
                (pipeline, effects)
            }

            // Restore from checkpoint (only from Blocked state)
            (Phase::Blocked { .. }, PipelineEvent::Restore { checkpoint }) => {
                let restored_phase = self
                    .phase_from_name(&checkpoint.phase)
                    .unwrap_or_else(|| self.first_working_phase());

                let mut pipeline = self.clone();
                pipeline.phase = restored_phase;
                pipeline.inputs = checkpoint.inputs.clone();
                pipeline.outputs = checkpoint.outputs.clone();

                let effects = vec![Effect::Emit(Event::PipelineRestored {
                    id: self.id.0.clone(),
                    from_sequence: checkpoint.sequence,
                })];
                (pipeline, effects)
            }

            // Invalid transitions - no change
            _ => (self.clone(), vec![]),
        }
    }

    /// Convert phase name string back to Phase variant
    fn phase_from_name(&self, name: &str) -> Option<Phase> {
        match name {
            "init" => Some(Phase::Init),
            "plan" => Some(Phase::Plan),
            "decompose" => Some(Phase::Decompose),
            "execute" => Some(Phase::Execute),
            "fix" => Some(Phase::Fix),
            "verify" => Some(Phase::Verify),
            "merge" => Some(Phase::Merge),
            "cleanup" => Some(Phase::Cleanup),
            "done" => Some(Phase::Done),
            _ => None,
        }
    }

    /// Helper to advance to the next phase
    fn advance_to_next_phase(
        &self,
        outputs: Option<BTreeMap<String, String>>,
    ) -> (Pipeline, Vec<Effect>) {
        let mut pipeline = self.clone();
        if let Some(out) = outputs {
            pipeline.outputs.extend(out);
        }
        pipeline.current_task_id = None;

        match self.next_phase() {
            Some(Phase::Done) | None => {
                pipeline.phase = Phase::Done;
                let effects = vec![Effect::Emit(Event::PipelineComplete {
                    id: self.id.0.clone(),
                })];
                (pipeline, effects)
            }
            Some(next) => {
                let next_name = next.name().to_string();
                pipeline.phase = next;
                let effects = vec![Effect::Emit(Event::PipelinePhase {
                    id: self.id.0.clone(),
                    phase: next_name,
                })];
                (pipeline, effects)
            }
        }
    }

    /// Create a checkpoint of current state
    pub fn checkpoint(&self, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        self.transition(PipelineEvent::RequestCheckpoint, clock)
    }

    /// Restore pipeline from a checkpoint
    pub fn restore_from(checkpoint: Checkpoint, kind: PipelineKind) -> Pipeline {
        // Parse phase from checkpoint, keeping legacy support for all phase names
        let phase = match checkpoint.phase.as_str() {
            "init" => Phase::Init,
            "plan" => Phase::Plan,
            "decompose" => Phase::Decompose,
            "execute" => Phase::Execute,
            "fix" => Phase::Fix,
            "verify" => Phase::Verify,
            "merge" => Phase::Merge,
            "cleanup" => Phase::Cleanup,
            "done" => Phase::Done,
            _ => Phase::Done,
        };

        Pipeline {
            id: checkpoint.pipeline_id,
            kind,
            name: String::new(),
            phase,
            inputs: checkpoint.inputs,
            outputs: checkpoint.outputs,
            workspace_id: None,
            current_task_id: None,
            created_at: Utc::now(),
            checkpoint_sequence: checkpoint.sequence,
            last_checkpoint_at: None,
        }
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

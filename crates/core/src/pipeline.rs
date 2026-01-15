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
use std::collections::HashMap;
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
    Build,
    Bugfix,
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
    PhaseCompleteWithOutputs { outputs: HashMap<String, String> },
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
    pub inputs: HashMap<String, String>,
    pub outputs: HashMap<String, String>,
    pub workspace_id: Option<WorkspaceId>,
    pub current_task_id: Option<TaskId>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub checkpoint_sequence: u64,
    #[serde(skip)]
    pub last_checkpoint_at: Option<Instant>,
}

impl Pipeline {
    /// Create a new build pipeline
    pub fn new_build(
        id: impl Into<String>,
        name: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("prompt".to_string(), prompt.into());

        Self {
            id: PipelineId(id.into()),
            kind: PipelineKind::Build,
            name: name.into(),
            phase: Phase::Init,
            inputs,
            outputs: HashMap::new(),
            workspace_id: None,
            current_task_id: None,
            created_at: Utc::now(),
            checkpoint_sequence: 0,
            last_checkpoint_at: None,
        }
    }

    /// Create a new bugfix pipeline
    pub fn new_bugfix(
        id: impl Into<String>,
        name: impl Into<String>,
        issue_id: impl Into<String>,
    ) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("issue_id".to_string(), issue_id.into());

        Self {
            id: PipelineId(id.into()),
            kind: PipelineKind::Bugfix,
            name: name.into(),
            phase: Phase::Init,
            inputs,
            outputs: HashMap::new(),
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

    /// Get the ordered phases for this pipeline kind
    fn phase_sequence(&self) -> Vec<Phase> {
        match self.kind {
            PipelineKind::Build => vec![
                Phase::Init,
                Phase::Plan,
                Phase::Decompose,
                Phase::Execute,
                Phase::Merge,
                Phase::Done,
            ],
            PipelineKind::Bugfix => vec![
                Phase::Init,
                Phase::Fix,
                Phase::Verify,
                Phase::Merge,
                Phase::Cleanup,
                Phase::Done,
            ],
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

    /// Get the first working phase for this pipeline kind (after Init)
    fn first_working_phase(&self) -> Phase {
        match self.kind {
            PipelineKind::Build => Phase::Plan,
            PipelineKind::Bugfix => Phase::Fix,
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
                    let mut map = HashMap::new();
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
        outputs: Option<HashMap<String, String>>,
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
        let phase = match checkpoint.phase.as_str() {
            "plan" => Phase::Plan,
            "decompose" => Phase::Decompose,
            "execute" => Phase::Execute,
            "fix" => Phase::Fix,
            "verify" => Phase::Verify,
            "merge" => Phase::Merge,
            "cleanup" => Phase::Cleanup,
            _ => match kind {
                PipelineKind::Build => Phase::Plan,
                PipelineKind::Bugfix => Phase::Fix,
            },
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
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    #[test]
    fn build_pipeline_starts_in_init() {
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");
        assert_eq!(pipeline.phase, Phase::Init);
        assert_eq!(pipeline.kind, PipelineKind::Build);
    }

    #[test]
    fn build_pipeline_follows_correct_phase_order() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Plan);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Decompose);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Execute);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Merge);

        let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Done);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::Emit(Event::PipelineComplete { .. }))));
    }

    #[test]
    fn bugfix_pipeline_follows_correct_phase_order() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_bugfix("p-1", "bugfix-42", "42");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Fix);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Verify);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Merge);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Cleanup);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Done);
    }

    #[test]
    fn pipeline_can_fail_from_any_phase() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Plan);

        let (pipeline, effects) = pipeline.transition(
            PipelineEvent::PhaseFailed {
                reason: "Tests failed".to_string(),
            },
            &clock,
        );
        assert!(matches!(pipeline.phase, Phase::Failed { .. }));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::Emit(Event::PipelineFailed { .. }))));
    }

    #[test]
    fn pipeline_emits_phase_events() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");
        let (_, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Emit(Event::PipelinePhase { phase, .. }) if phase == "plan"
        )));
    }

    #[test]
    fn pipeline_transition_with_outputs() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let mut outputs = HashMap::new();
        outputs.insert("plan".to_string(), "detailed plan here".to_string());

        let (pipeline, _) =
            pipeline.transition(PipelineEvent::PhaseCompleteWithOutputs { outputs }, &clock);

        assert_eq!(pipeline.phase, Phase::Plan);
        assert_eq!(
            pipeline.outputs.get("plan"),
            Some(&"detailed plan here".to_string())
        );
    }

    #[test]
    fn pipeline_running_to_blocked() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

        let (pipeline, effects) = pipeline.transition(
            PipelineEvent::PhaseFailedRecoverable {
                reason: "Need more context".to_string(),
            },
            &clock,
        );

        assert!(matches!(
            &pipeline.phase,
            Phase::Blocked { waiting_on, .. } if waiting_on == "Need more context"
        ));
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::PipelineBlocked { .. })
        ));
    }

    #[test]
    fn pipeline_blocked_to_running() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        let (pipeline, _) = pipeline.transition(
            PipelineEvent::PhaseFailedRecoverable {
                reason: "Need more context".to_string(),
            },
            &clock,
        );

        let (pipeline, effects) = pipeline.transition(PipelineEvent::Unblocked, &clock);

        assert_eq!(pipeline.phase, Phase::Plan);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::PipelineResumed { .. })
        ));
    }

    #[test]
    fn pipeline_task_assignment() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

        let (pipeline, effects) = pipeline.transition(
            PipelineEvent::TaskAssigned {
                task_id: TaskId("task-1".to_string()),
            },
            &clock,
        );

        assert_eq!(pipeline.current_task_id, Some(TaskId("task-1".to_string())));
        assert!(effects.is_empty());
    }

    #[test]
    fn pipeline_task_completion_advances_phase() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Plan);

        let (pipeline, _) = pipeline.transition(
            PipelineEvent::TaskAssigned {
                task_id: TaskId("task-1".to_string()),
            },
            &clock,
        );

        let (pipeline, effects) = pipeline.transition(
            PipelineEvent::TaskComplete {
                task_id: TaskId("task-1".to_string()),
                output: Some("plan output".to_string()),
            },
            &clock,
        );

        assert_eq!(pipeline.phase, Phase::Decompose);
        assert!(pipeline.outputs.contains_key("task_output"));
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::PipelinePhase { .. })
        ));
    }

    #[test]
    fn pipeline_checkpoint() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

        let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

        assert_eq!(pipeline.checkpoint_sequence, 1);
        assert!(pipeline.last_checkpoint_at.is_some());
        assert!(matches!(
            &effects[0],
            Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == 1
        ));
    }

    #[test]
    fn pipeline_checkpoint_increments_sequence() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
        assert_eq!(pipeline.checkpoint_sequence, 1);

        let (pipeline, _) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
        assert_eq!(pipeline.checkpoint_sequence, 2);

        let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
        assert_eq!(pipeline.checkpoint_sequence, 3);
        assert!(matches!(
            &effects[0],
            Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == 3
        ));
    }

    #[test]
    fn pipeline_restore_from_checkpoint() {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        // Move to Plan, take checkpoint
        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

        let checkpoint = if let Effect::SaveCheckpoint { checkpoint, .. } = &effects[0] {
            checkpoint.clone()
        } else {
            panic!("Expected SaveCheckpoint effect");
        };

        // Block it (recoverable failure)
        let (pipeline, _) = pipeline.transition(
            PipelineEvent::PhaseFailedRecoverable {
                reason: "error".to_string(),
            },
            &clock,
        );
        assert!(matches!(pipeline.phase, Phase::Blocked { .. }));

        // Restore from checkpoint
        let (pipeline, effects) =
            pipeline.transition(PipelineEvent::Restore { checkpoint }, &clock);

        assert_eq!(pipeline.phase, Phase::Plan);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::PipelineRestored { from_sequence, .. }) if *from_sequence == 1
        ));
    }

    use yare::parameterized;

    #[parameterized(
            build_init_to_plan = { PipelineKind::Build, "init", "plan" },
            build_plan_to_decompose = { PipelineKind::Build, "plan", "decompose" },
            build_decompose_to_execute = { PipelineKind::Build, "decompose", "execute" },
            build_execute_to_merge = { PipelineKind::Build, "execute", "merge" },
            build_merge_to_done = { PipelineKind::Build, "merge", "done" },
            bugfix_init_to_fix = { PipelineKind::Bugfix, "init", "fix" },
            bugfix_fix_to_verify = { PipelineKind::Bugfix, "fix", "verify" },
            bugfix_verify_to_merge = { PipelineKind::Bugfix, "verify", "merge" },
            bugfix_merge_to_cleanup = { PipelineKind::Bugfix, "merge", "cleanup" },
            bugfix_cleanup_to_done = { PipelineKind::Bugfix, "cleanup", "done" },
        )]
    fn phase_progression(kind: PipelineKind, current: &str, expected_next: &str) {
        let clock = FakeClock::new();

        // Create the pipeline
        let pipeline = match kind {
            PipelineKind::Build => Pipeline::new_build("p-1", "test", "prompt"),
            PipelineKind::Bugfix => Pipeline::new_bugfix("p-1", "test", "issue-1"),
        };

        // Transition to the current phase
        let pipeline = transition_to_phase(pipeline, current, &clock);

        // Complete current phase
        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

        assert_eq!(pipeline.phase.name(), expected_next);
    }

    fn transition_to_phase(pipeline: Pipeline, target_phase: &str, clock: &FakeClock) -> Pipeline {
        let mut pipeline = pipeline;
        while pipeline.phase.name() != target_phase {
            let (p, _) = pipeline.transition(PipelineEvent::PhaseComplete, clock);
            pipeline = p;
            if pipeline.phase.is_terminal() {
                break;
            }
        }
        pipeline
    }

    #[parameterized(
            build_done_is_terminal = { PipelineKind::Build },
            bugfix_done_is_terminal = { PipelineKind::Bugfix },
        )]
    fn done_is_terminal(kind: PipelineKind) {
        let clock = FakeClock::new();

        let pipeline = match kind {
            PipelineKind::Build => Pipeline::new_build("p-1", "test", "prompt"),
            PipelineKind::Bugfix => Pipeline::new_bugfix("p-1", "test", "issue-1"),
        };

        // Transition all the way to Done
        let pipeline = transition_to_phase(pipeline, "done", &clock);
        assert_eq!(pipeline.phase, Phase::Done);

        // Try to transition further - should be no-op
        let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
        assert_eq!(pipeline.phase, Phase::Done);
        assert!(effects.is_empty());
    }

    #[parameterized(
            from_plan = { "plan" },
            from_decompose = { "decompose" },
            from_execute = { "execute" },
        )]
    fn failure_from_any_phase(current: &str) {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "test", "prompt");

        // Transition to the target phase
        let pipeline = transition_to_phase(pipeline, current, &clock);
        assert_eq!(pipeline.phase.name(), current);

        // Fail it
        let (pipeline, effects) = pipeline.transition(
            PipelineEvent::PhaseFailed {
                reason: "Test failure".to_string(),
            },
            &clock,
        );

        assert!(matches!(pipeline.phase, Phase::Failed { .. }));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::Emit(Event::PipelineFailed { .. }))));
    }

    #[parameterized(
            checkpoint_at_init = { "init", 1 },
            checkpoint_at_plan = { "plan", 1 },
            checkpoint_at_execute = { "execute", 1 },
        )]
    fn checkpoint_at_phase(phase: &str, expected_sequence: u64) {
        let clock = FakeClock::new();
        let pipeline = Pipeline::new_build("p-1", "test", "prompt");

        // Transition to target phase
        let pipeline = transition_to_phase(pipeline, phase, &clock);

        // Take a checkpoint
        let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

        assert_eq!(pipeline.checkpoint_sequence, expected_sequence);
        assert!(matches!(
            &effects[0],
            Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == expected_sequence
        ));
    }

    #[test]
    fn pipeline_restore_from_creates_pipeline() {
        let checkpoint = Checkpoint {
            pipeline_id: PipelineId("p-1".to_string()),
            phase: "execute".to_string(),
            inputs: {
                let mut m = HashMap::new();
                m.insert("prompt".to_string(), "test".to_string());
                m
            },
            outputs: {
                let mut m = HashMap::new();
                m.insert("plan".to_string(), "plan output".to_string());
                m
            },
            created_at: std::time::Instant::now(),
            sequence: 5,
        };

        let pipeline = Pipeline::restore_from(checkpoint, PipelineKind::Build);

        assert_eq!(pipeline.id.0, "p-1");
        assert_eq!(pipeline.phase, Phase::Execute);
        assert_eq!(pipeline.checkpoint_sequence, 5);
        assert_eq!(
            pipeline.outputs.get("plan"),
            Some(&"plan output".to_string())
        );
    }
}

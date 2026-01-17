//! Pipeline state machine
//!
//! A pipeline represents a multi-phase workflow (build or bugfix).

use crate::effect::{Effect, Event};
use crate::workspace::WorkspaceId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a pipeline
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PipelineId(pub String);

impl std::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
    Init,
    Blocked { waiting_on: String },
    Plan,
    Decompose,
    Execute,
    Fix,
    Verify,
    Merge,
    Cleanup,
    Done,
    Failed { reason: String },
}

impl Phase {
    pub fn name(&self) -> &'static str {
        match self {
            Phase::Init => "init",
            Phase::Blocked { .. } => "blocked",
            Phase::Plan => "plan",
            Phase::Decompose => "decompose",
            Phase::Execute => "execute",
            Phase::Fix => "fix",
            Phase::Verify => "verify",
            Phase::Merge => "merge",
            Phase::Cleanup => "cleanup",
            Phase::Done => "done",
            Phase::Failed { .. } => "failed",
        }
    }
}

/// Events that can change pipeline state
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    PhaseComplete,
    PhaseFailed { reason: String },
    Unblocked,
}

/// A pipeline representing a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: PipelineId,
    pub kind: PipelineKind,
    pub name: String,
    pub phase: Phase,
    pub inputs: HashMap<String, String>,
    pub workspace_id: Option<WorkspaceId>,
    pub created_at: DateTime<Utc>,
}

impl Pipeline {
    /// Create a new build pipeline
    pub fn new_build(id: impl Into<String>, name: impl Into<String>, prompt: impl Into<String>) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("prompt".to_string(), prompt.into());

        Self {
            id: PipelineId(id.into()),
            kind: PipelineKind::Build,
            name: name.into(),
            phase: Phase::Init,
            inputs,
            workspace_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new bugfix pipeline
    pub fn new_bugfix(id: impl Into<String>, name: impl Into<String>, issue_id: impl Into<String>) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("issue_id".to_string(), issue_id.into());

        Self {
            id: PipelineId(id.into()),
            kind: PipelineKind::Bugfix,
            name: name.into(),
            phase: Phase::Init,
            inputs,
            workspace_id: None,
            created_at: Utc::now(),
        }
    }

    /// Set the workspace for this pipeline
    pub fn with_workspace(self, workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            ..self
        }
    }

    /// Transition the pipeline to a new state based on an event
    pub fn transition(&self, event: PipelineEvent) -> (Pipeline, Vec<Effect>) {
        match (&self.kind, &self.phase, event) {
            // Build pipeline transitions
            (PipelineKind::Build, Phase::Init, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Plan)
            }
            (PipelineKind::Build, Phase::Plan, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Decompose)
            }
            (PipelineKind::Build, Phase::Decompose, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Execute)
            }
            (PipelineKind::Build, Phase::Execute, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Merge)
            }
            (PipelineKind::Build, Phase::Merge, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Done)
            }

            // Bugfix pipeline transitions
            (PipelineKind::Bugfix, Phase::Init, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Fix)
            }
            (PipelineKind::Bugfix, Phase::Fix, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Verify)
            }
            (PipelineKind::Bugfix, Phase::Verify, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Merge)
            }
            (PipelineKind::Bugfix, Phase::Merge, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Cleanup)
            }
            (PipelineKind::Bugfix, Phase::Cleanup, PipelineEvent::PhaseComplete) => {
                self.to_phase(Phase::Done)
            }

            // Blocked handling
            (_, Phase::Blocked { .. }, PipelineEvent::Unblocked) => {
                // Return to appropriate phase based on kind
                match self.kind {
                    PipelineKind::Build => self.to_phase(Phase::Execute),
                    PipelineKind::Bugfix => self.to_phase(Phase::Fix),
                }
            }

            // Failure handling - any phase can fail
            (_, _, PipelineEvent::PhaseFailed { reason }) => {
                (
                    self.with_phase(Phase::Failed { reason: reason.clone() }),
                    vec![Effect::Emit(Event::PipelineFailed {
                        id: self.id.0.clone(),
                        reason,
                    })],
                )
            }

            // Invalid transitions are ignored
            _ => (self.clone(), vec![]),
        }
    }

    fn to_phase(&self, phase: Phase) -> (Pipeline, Vec<Effect>) {
        let phase_name = phase.name().to_string();
        let is_done = matches!(phase, Phase::Done);

        let effects = if is_done {
            vec![Effect::Emit(Event::PipelineComplete {
                id: self.id.0.clone(),
            })]
        } else {
            vec![Effect::Emit(Event::PipelinePhase {
                id: self.id.0.clone(),
                phase: phase_name,
            })]
        };

        (self.with_phase(phase), effects)
    }

    fn with_phase(&self, phase: Phase) -> Pipeline {
        Pipeline {
            phase,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_pipeline_starts_in_init() {
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");
        assert_eq!(pipeline.phase, Phase::Init);
        assert_eq!(pipeline.kind, PipelineKind::Build);
    }

    #[test]
    fn build_pipeline_follows_correct_phase_order() {
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Plan);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Decompose);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Execute);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Merge);

        let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Done);
        assert!(effects.iter().any(|e| matches!(e, Effect::Emit(Event::PipelineComplete { .. }))));
    }

    #[test]
    fn bugfix_pipeline_follows_correct_phase_order() {
        let pipeline = Pipeline::new_bugfix("p-1", "bugfix-42", "42");

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Fix);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Verify);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Merge);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Cleanup);

        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Done);
    }

    #[test]
    fn pipeline_can_fail_from_any_phase() {
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");
        let (pipeline, _) = pipeline.transition(PipelineEvent::PhaseComplete);
        assert_eq!(pipeline.phase, Phase::Plan);

        let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseFailed {
            reason: "Tests failed".to_string(),
        });
        assert!(matches!(pipeline.phase, Phase::Failed { .. }));
        assert!(effects.iter().any(|e| matches!(e, Effect::Emit(Event::PipelineFailed { .. }))));
    }

    #[test]
    fn pipeline_emits_phase_events() {
        let pipeline = Pipeline::new_build("p-1", "auth", "Add authentication");
        let (_, effects) = pipeline.transition(PipelineEvent::PhaseComplete);

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Emit(Event::PipelinePhase { phase, .. }) if phase == "plan"
        )));
    }
}

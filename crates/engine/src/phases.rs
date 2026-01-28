// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Pipeline phase transition effects.
//!
//! Helpers for building effects that transition pipelines between phases.

use oj_core::{Effect, Event, Operation, PhaseStatus, Pipeline};

/// Build effects to mark a phase as running
pub fn phase_start_effects(pipeline_id: &str, phase_name: &str) -> Vec<Effect> {
    vec![
        Effect::Persist {
            operation: Operation::PhaseStatusUpdate {
                pipeline_id: pipeline_id.to_string(),
                status: PhaseStatus::Running,
            },
        },
        Effect::Emit {
            event: Event::Custom {
                name: "pipeline:phase:start".to_string(),
                data: serde_json::json!({
                    "pipeline_id": pipeline_id,
                    "phase": phase_name,
                }),
            },
        },
    ]
}

/// Build effects to transition to the next phase
pub fn phase_transition_effects(pipeline: &Pipeline, next_phase: &str) -> Vec<Effect> {
    vec![
        Effect::Persist {
            operation: Operation::PipelineTransition {
                id: pipeline.id.clone(),
                phase: next_phase.to_string(),
            },
        },
        Effect::Emit {
            event: Event::Custom {
                name: "pipeline:phase".to_string(),
                data: serde_json::json!({
                    "id": pipeline.id,
                    "phase": next_phase,
                }),
            },
        },
    ]
}

/// Build effects to transition to failure phase with error
pub fn failure_transition_effects(pipeline: &Pipeline, on_fail: &str, error: &str) -> Vec<Effect> {
    vec![
        Effect::Persist {
            operation: Operation::PipelineTransition {
                id: pipeline.id.clone(),
                phase: on_fail.to_string(),
            },
        },
        Effect::Emit {
            event: Event::Custom {
                name: "pipeline:phase".to_string(),
                data: serde_json::json!({
                    "id": pipeline.id,
                    "phase": on_fail,
                    "error": error,
                }),
            },
        },
    ]
}

/// Build effects to mark pipeline as failed (terminal)
pub fn failure_effects(pipeline: &Pipeline, error: &str) -> Vec<Effect> {
    vec![
        Effect::Persist {
            operation: Operation::PipelineTransition {
                id: pipeline.id.clone(),
                phase: "failed".to_string(),
            },
        },
        Effect::Emit {
            event: Event::Custom {
                name: "pipeline:failed".to_string(),
                data: serde_json::json!({
                    "pipeline_id": pipeline.id,
                    "name": pipeline.name,
                    "phase": &pipeline.phase,
                    "error": error,
                }),
            },
        },
    ]
}

/// Build effects to complete a pipeline
pub fn completion_effects(pipeline: &Pipeline) -> Vec<Effect> {
    let mut effects = vec![];

    // Ensure pipeline is in done phase with completed status
    if !pipeline.is_terminal() {
        effects.push(Effect::Persist {
            operation: Operation::PipelineTransition {
                id: pipeline.id.clone(),
                phase: "done".to_string(),
            },
        });
    }
    effects.push(Effect::Persist {
        operation: Operation::PhaseStatusUpdate {
            pipeline_id: pipeline.id.clone(),
            status: PhaseStatus::Completed,
        },
    });

    // Emit completion event
    effects.push(Effect::Emit {
        event: Event::Custom {
            name: "pipeline:completed".to_string(),
            data: serde_json::json!({
                "pipeline_id": pipeline.id,
                "name": pipeline.name,
                "kind": pipeline.kind,
            }),
        },
    });

    // Cleanup session if exists
    if let Some(session_id) = &pipeline.session_id {
        effects.push(Effect::Kill {
            session_id: session_id.clone(),
        });
        effects.push(Effect::Persist {
            operation: Operation::SessionDelete {
                id: session_id.clone(),
            },
        });
    }

    effects
}

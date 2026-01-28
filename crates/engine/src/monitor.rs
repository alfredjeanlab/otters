// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Session monitoring for agent pipelines.
//!
//! Handles detection of agent state from session logs and triggers
//! appropriate actions (nudge, recover, escalate, etc.).

use crate::session_log::FailureReason;
use crate::RuntimeError;
use oj_core::{Effect, Operation, Pipeline};
use oj_runbook::{ActionConfig, AgentAction, AgentDef, ErrorType, RunDirective, Runbook};
use std::collections::HashMap;

/// Convert a session log failure reason to an error type
pub fn failure_to_error_type(reason: &FailureReason) -> Option<ErrorType> {
    match reason {
        FailureReason::Unauthorized => Some(ErrorType::Unauthorized),
        FailureReason::OutOfCredits => Some(ErrorType::OutOfCredits),
        FailureReason::NoInternet => Some(ErrorType::NoInternet),
        FailureReason::RateLimited => Some(ErrorType::RateLimited),
        FailureReason::Other(_) => None,
    }
}

/// Convert a failure reason to a human-readable error message
pub fn failure_to_message(reason: &FailureReason) -> &str {
    match reason {
        FailureReason::Unauthorized => "API key unauthorized",
        FailureReason::OutOfCredits => "Out of API credits",
        FailureReason::NoInternet => "Network error",
        FailureReason::RateLimited => "Rate limited",
        FailureReason::Other(msg) => msg.as_str(),
    }
}

/// Get the current agent definition for a pipeline phase
pub fn get_agent_def<'a>(
    runbook: &'a Runbook,
    pipeline: &Pipeline,
) -> Result<&'a AgentDef, RuntimeError> {
    let pipeline_def = runbook
        .get_pipeline(&pipeline.kind)
        .ok_or_else(|| RuntimeError::PipelineDefNotFound(pipeline.kind.clone()))?;

    let phase_def = pipeline_def.get_phase(&pipeline.phase).ok_or_else(|| {
        RuntimeError::PipelineNotFound(format!("phase {} not found", pipeline.phase))
    })?;

    // Extract agent name from run directive
    let agent_name = match &phase_def.run {
        RunDirective::Agent { agent } => agent,
        _ => {
            return Err(RuntimeError::InvalidRunDirective {
                context: format!("phase {}", pipeline.phase),
                directive: "not an agent phase".to_string(),
            })
        }
    };

    runbook
        .get_agent(agent_name)
        .ok_or_else(|| RuntimeError::AgentNotFound(agent_name.clone()))
}

/// Build effects for an agent action (nudge, recover, escalate, etc.)
pub fn build_action_effects(
    pipeline: &Pipeline,
    agent_def: &AgentDef,
    action_config: &ActionConfig,
    trigger: &str,
    inputs: &HashMap<String, String>,
) -> Result<ActionEffects, RuntimeError> {
    let action = action_config.action();
    let message = action_config.message();

    tracing::info!(
        pipeline_id = %pipeline.id,
        trigger = trigger,
        action = ?action,
        "building agent action effects"
    );

    match action {
        AgentAction::Nudge => {
            let session_id = pipeline
                .session_id
                .as_ref()
                .ok_or_else(|| RuntimeError::PipelineNotFound("no session".into()))?;

            let nudge_message = message.unwrap_or("Please continue with the task.");
            Ok(ActionEffects::Nudge {
                effects: vec![Effect::Send {
                    session_id: session_id.clone(),
                    input: format!("{}\n", nudge_message),
                }],
            })
        }

        AgentAction::Done => Ok(ActionEffects::AdvancePipeline),

        AgentAction::Fail => Ok(ActionEffects::FailPipeline {
            error: trigger.to_string(),
        }),

        AgentAction::Restart => {
            // Fresh restart: kill session, remove workspace, re-spawn
            Ok(ActionEffects::Restart {
                kill_session: pipeline.session_id.clone(),
                workspace_path: pipeline.workspace_path.clone(),
                agent_name: agent_def.name.clone(),
                inputs: inputs.clone(),
            })
        }

        AgentAction::Recover => {
            // Build modified inputs for re-spawn
            let mut new_inputs = inputs.clone();
            if let Some(msg) = message {
                if action_config.append() {
                    let existing = new_inputs.get("prompt").cloned().unwrap_or_default();
                    new_inputs.insert("prompt".to_string(), format!("{}\n\n{}", existing, msg));
                } else {
                    new_inputs.insert("prompt".to_string(), msg.to_string());
                }
            }

            Ok(ActionEffects::Recover {
                kill_session: pipeline.session_id.clone(),
                agent_name: agent_def.name.clone(),
                inputs: new_inputs,
            })
        }

        AgentAction::Escalate => {
            tracing::warn!(
                pipeline_id = %pipeline.id,
                trigger = trigger,
                message = ?message,
                "escalating to human"
            );

            let effects = vec![
                // Emit escalation event
                Effect::Emit {
                    event: oj_core::Event::Custom {
                        name: "pipeline:escalate".to_string(),
                        data: serde_json::json!({
                            "pipeline_id": pipeline.id,
                            "pipeline_name": pipeline.name,
                            "phase": pipeline.phase,
                            "reason": trigger,
                        }),
                    },
                },
                // Desktop notification
                Effect::Notify {
                    title: format!("Pipeline needs attention: {}", pipeline.name),
                    message: trigger.to_string(),
                },
                // Update pipeline status to Waiting
                Effect::Persist {
                    operation: Operation::PhaseStatusUpdate {
                        pipeline_id: pipeline.id.clone(),
                        status: oj_core::PhaseStatus::Waiting,
                    },
                },
                // Stop monitoring timer (human will intervene)
                Effect::CancelTimer {
                    id: format!("session:{}:check", pipeline.id),
                },
            ];

            Ok(ActionEffects::Escalate { effects })
        }
    }
}

/// Results from building action effects
pub enum ActionEffects {
    /// Send nudge message to session
    Nudge { effects: Vec<Effect> },
    /// Advance to next pipeline phase
    AdvancePipeline,
    /// Fail the pipeline with an error
    FailPipeline { error: String },
    /// Restart with fresh workspace
    Restart {
        kill_session: Option<String>,
        workspace_path: Option<std::path::PathBuf>,
        agent_name: String,
        inputs: HashMap<String, String>,
    },
    /// Recover by re-spawning agent (keeps workspace)
    Recover {
        kill_session: Option<String>,
        agent_name: String,
        inputs: HashMap<String, String>,
    },
    /// Escalate to human
    Escalate { effects: Vec<Effect> },
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Custom event handlers for the runtime.
//!
//! This module handles custom events like `session:send` and `pipeline:resume`
//! that allow external control of sessions and pipelines.

use crate::error::RuntimeError;
use crate::Executor;
use oj_adapters::{NotifyAdapter, RepoAdapter, SessionAdapter};
use oj_core::{Effect, Event, Operation, PhaseStatus, Pipeline};
use std::time::Duration;

/// Handle custom events from the daemon.
///
/// Custom events provide an extension point for controlling sessions and pipelines.
pub async fn handle_custom_event<S, R, N>(
    executor: &Executor<S, R, N>,
    name: &str,
    data: &serde_json::Value,
    get_pipeline: impl Fn(&str) -> Option<Pipeline>,
) -> Result<Vec<Event>, RuntimeError>
where
    S: SessionAdapter,
    R: RepoAdapter,
    N: NotifyAdapter,
{
    match name {
        "session:send" => {
            // Send input to a session
            let session_id = data["session_id"]
                .as_str()
                .ok_or_else(|| RuntimeError::PipelineNotFound("missing session_id".into()))?;
            let input = data["input"]
                .as_str()
                .ok_or_else(|| RuntimeError::PipelineNotFound("missing input".into()))?;

            executor
                .execute(Effect::Send {
                    session_id: session_id.to_string(),
                    input: format!("{}\n", input),
                })
                .await?;
            Ok(vec![])
        }
        "pipeline:resume" => {
            // Resume monitoring for an escalated pipeline
            let pipeline_id = data["pipeline_id"]
                .as_str()
                .ok_or_else(|| RuntimeError::PipelineNotFound("missing pipeline_id".into()))?;

            let pipeline = get_pipeline(pipeline_id)
                .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;

            // Update status back to Running
            executor
                .execute(Effect::Persist {
                    operation: Operation::PhaseStatusUpdate {
                        pipeline_id: pipeline.id.clone(),
                        status: PhaseStatus::Running,
                    },
                })
                .await?;

            // Restart session monitoring
            executor
                .execute(Effect::SetTimer {
                    id: format!("session:{}:check", pipeline.id),
                    duration: Duration::from_secs(10),
                })
                .await?;

            tracing::info!(pipeline_id, "resumed monitoring for pipeline");
            Ok(vec![])
        }
        _ => {
            // Unknown custom events are informational
            tracing::debug!(name, "ignoring unknown custom event");
            Ok(vec![])
        }
    }
}

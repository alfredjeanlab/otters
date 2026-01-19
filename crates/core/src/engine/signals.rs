// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Signal handling and polling for the engine
//!
//! This module contains methods for handling external signals (done, checkpoint)
//! and polling session output for heartbeats and stuck task detection.

use crate::adapters::{SessionAdapter, SessionId};
use crate::clock::Clock;
use crate::effect::Event;
use crate::engine::executor::Adapters;
use crate::engine::runtime::{Engine, EngineError};
use crate::pipeline::PipelineEvent;
use crate::task::{Task, TaskEvent, TaskId};
use crate::workspace::WorkspaceId;
use std::time::Duration;

impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Handle external signal from `oj done`
    pub async fn signal_done(
        &mut self,
        workspace_id: &WorkspaceId,
        error: Option<String>,
    ) -> Result<(), EngineError> {
        let pipeline = self
            .find_pipeline_by_workspace(workspace_id)
            .ok_or_else(|| EngineError::WorkspaceNotFound(workspace_id.clone()))?;

        let task_id = pipeline.current_task_id.clone();

        match (error, task_id) {
            (None, Some(task_id)) => {
                self.process_task_event(&task_id, TaskEvent::Complete { output: None })
                    .await?;
            }
            (Some(reason), Some(task_id)) => {
                self.process_task_event(&task_id, TaskEvent::Fail { reason })
                    .await?;
            }
            (_, None) => {
                tracing::warn!(?workspace_id, "done signal with no active task");
            }
        }

        Ok(())
    }

    /// Handle checkpoint signal
    pub async fn signal_checkpoint(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<(), EngineError> {
        let pipeline = self
            .find_pipeline_by_workspace(workspace_id)
            .ok_or_else(|| EngineError::WorkspaceNotFound(workspace_id.clone()))?;

        let pipeline_id = pipeline.id.clone();
        self.process_pipeline_event(&pipeline_id, PipelineEvent::RequestCheckpoint)
            .await
    }

    /// Process heartbeat from session output monitoring
    ///
    /// This persists the heartbeat to the Session via WAL and updates in-memory state.
    /// Session owns heartbeat tracking for stuck detection.
    pub async fn process_heartbeat(
        &mut self,
        session_id: &crate::session::SessionId,
    ) -> Result<(), EngineError> {
        // Persist heartbeat to WAL (Session owns heartbeat state)
        self.store_mut().session_heartbeat(&session_id.0)?;

        // Update in-memory session state
        // Note: Engine's sessions map uses adapters::SessionId as key
        let adapter_session_id = SessionId(session_id.0.clone());
        if let Some(session) = self.sessions().get(&adapter_session_id) {
            let new_session = session.record_heartbeat(self.clock().now());
            self.sessions_mut().insert(adapter_session_id, new_session);
        }

        // Also update task heartbeat for backward compatibility (during migration)
        // TODO: Remove this once Task.last_heartbeat is fully removed
        if let Some(task) = self.find_task_by_session(&session_id.0) {
            let task_id = task.id.clone();
            self.process_task_event(
                &task_id,
                TaskEvent::Heartbeat {
                    timestamp: self.clock().now(),
                },
            )
            .await?;
        }

        Ok(())
    }

    /// Monitor sessions and generate heartbeats
    pub async fn poll_sessions(&mut self) -> Result<(), EngineError> {
        let session_ids: Vec<_> = self
            .tasks()
            .values()
            .filter_map(|t| t.session_id.clone())
            .collect();

        for session_id in session_ids {
            let adapter_id = SessionId(session_id.0.clone());

            // Check if session is alive
            let is_alive = self
                .adapters()
                .sessions()
                .is_alive(&adapter_id)
                .await
                .unwrap_or(false);

            if !is_alive {
                self.process_event(Event::SessionDead {
                    id: session_id.0.clone(),
                    reason: "session terminated".to_string(),
                })
                .await?;
                continue;
            }

            // Capture pane and check for new output
            let output = self
                .adapters()
                .sessions()
                .capture_pane(&adapter_id, 50)
                .await
                .unwrap_or_default();

            let hash = calculate_hash(&output);

            // Check if output changed (simple hash comparison)
            // Note: In a real implementation, we'd track last_output_hash per session
            if !output.is_empty() {
                self.process_heartbeat(&session_id).await?;
            }
            let _ = hash; // Suppress unused warning for now
        }

        Ok(())
    }

    /// Tick all active tasks to detect stuck state
    ///
    /// This queries each task's session for idle time (from Session::idle_time())
    /// and passes it to the task tick event for stuck detection.
    pub async fn tick_all_tasks(&mut self) -> Result<(), EngineError> {
        let now = self.clock().now();

        // Collect task info including session idle time
        // Note: Task has crate::session::SessionId, Engine's sessions map uses adapters::SessionId
        let task_info: Vec<_> = self
            .tasks()
            .values()
            .filter(|t| t.is_running() || t.is_stuck())
            .map(|t| {
                // Get session idle time for stuck detection
                // Convert from session::SessionId to adapters::SessionId for lookup
                let session_idle_time = t
                    .session_id
                    .as_ref()
                    .and_then(|sid| {
                        let adapter_sid = SessionId(sid.0.clone());
                        self.sessions().get(&adapter_sid)
                    })
                    .and_then(|session| session.idle_time(now));
                (t.id.clone(), session_idle_time)
            })
            .collect();

        for (task_id, session_idle_time) in task_info {
            self.process_task_event(&task_id, TaskEvent::Tick { session_idle_time })
                .await?;
        }

        Ok(())
    }

    /// Tick queue to handle visibility timeouts
    pub fn tick_queue(&mut self, queue_name: &str) -> Result<(), EngineError> {
        use crate::clock::SystemClock;
        use crate::effect::Effect;

        // Use the granular queue_tick operation which handles both
        // the state transition and WAL persistence
        // Note: We use SystemClock here since tick is time-sensitive and
        // doesn't need to be deterministic for testing
        let effects = self.store_mut().queue_tick(queue_name, &SystemClock)?;

        for effect in effects {
            if let Effect::Emit(event) = effect {
                tracing::info!(?event, "queue tick event");
            }
        }

        Ok(())
    }

    /// Start a task for the current phase of a pipeline
    pub async fn start_phase_task(
        &mut self,
        pipeline_id: &crate::pipeline::PipelineId,
    ) -> Result<TaskId, EngineError> {
        // Extract the info we need before borrowing mutably
        let (task_id, phase_name) = {
            let pipeline = self
                .get_pipeline(pipeline_id)
                .ok_or_else(|| EngineError::PipelineNotFound(pipeline_id.clone()))?;

            let phase_name = pipeline.phase.name().to_string();
            let task_id = TaskId(format!("{}-{}", pipeline_id.0, phase_name));
            (task_id, phase_name)
        };

        let task = Task::new(
            task_id.clone(),
            pipeline_id.clone(),
            &phase_name,
            Duration::from_secs(30),
            Duration::from_secs(120),
            self.clock(),
        );

        self.add_task(task)?;

        // Assign task to pipeline
        self.process_pipeline_event(
            pipeline_id,
            PipelineEvent::TaskAssigned {
                task_id: task_id.clone(),
            },
        )
        .await?;

        // Start the task with a session - extract workspace path first
        let workspace_path = self
            .find_workspace_for_pipeline(pipeline_id)
            .map(|w| w.path.clone());

        if let Some(path) = workspace_path {
            let session_name = format!("oj-{}-{}", pipeline_id.0, phase_name);
            let command = "claude";

            match self
                .adapters()
                .sessions()
                .spawn(&session_name, &path, command)
                .await
            {
                Ok(_) => {
                    self.process_task_event(
                        &task_id,
                        TaskEvent::Start {
                            session_id: crate::session::SessionId(session_name),
                        },
                    )
                    .await?;
                }
                Err(e) => {
                    tracing::error!(?e, "failed to spawn session for task");
                }
            }
        }

        Ok(task_id)
    }
}

fn calculate_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

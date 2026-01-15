// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Main engine for orchestrating state machines and executing effects

use crate::adapters::{MergeResult, NotifyAdapter, RepoAdapter, SessionAdapter, SessionId};
use crate::clock::Clock;
use crate::config::NotifyConfig;
use crate::effect::{Effect, Event, LogLevel};
use crate::engine::executor::Adapters;
use crate::events::{EventBus, EventLog, EventPattern, EventReceiver, Subscription};
use crate::pipeline::{Pipeline, PipelineEvent, PipelineId};
use crate::session::Session;
use crate::storage::{JsonStore, StorageError};
use crate::task::{Task, TaskEvent, TaskId, TaskState};
use crate::workspace::{Workspace, WorkspaceId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("adapter error: {0}")]
    Adapter(String),
    #[error("pipeline not found: {0}")]
    PipelineNotFound(PipelineId),
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(WorkspaceId),
}

/// Result of executing an effect
#[derive(Debug)]
pub enum EffectResult {
    /// Effect succeeded
    Ok,
    /// Effect failed, generate recovery event
    Failed { event: Event },
    /// Effect requires retry
    Retry { after: Duration },
}

/// The engine orchestrates state machines and executes effects
pub struct Engine<A: Adapters, C: Clock> {
    adapters: A,
    store: JsonStore,
    clock: C,

    // In-memory state caches (authoritative state is in store)
    pipelines: HashMap<PipelineId, Pipeline>,
    tasks: HashMap<TaskId, Task>,
    workspaces: HashMap<WorkspaceId, Workspace>,
    sessions: HashMap<SessionId, Session>,

    // Recovery state tracking
    recovery_states: HashMap<TaskId, crate::engine::recovery::RecoveryState>,

    // Events system
    event_bus: EventBus,
    event_log: Option<EventLog>,
    notify_config: NotifyConfig,
}

impl<A: Adapters, C: Clock> Engine<A, C> {
    pub fn new(adapters: A, store: JsonStore, clock: C) -> Self {
        Self {
            adapters,
            store,
            clock,
            pipelines: HashMap::new(),
            tasks: HashMap::new(),
            workspaces: HashMap::new(),
            sessions: HashMap::new(),
            recovery_states: HashMap::new(),
            event_bus: EventBus::new(),
            event_log: None,
            notify_config: NotifyConfig::default(),
        }
    }

    /// Enable event logging to the given path
    pub fn with_event_log(mut self, path: impl Into<PathBuf>) -> std::io::Result<Self> {
        self.event_log = Some(EventLog::open(path.into())?);
        Ok(self)
    }

    /// Set notification configuration
    pub fn with_notify_config(mut self, config: NotifyConfig) -> Self {
        self.notify_config = config;
        self
    }

    /// Get the event bus for subscriptions
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Subscribe to events matching patterns
    pub fn subscribe(&self, id: &str, patterns: Vec<&str>, description: &str) -> EventReceiver {
        let subscription = Subscription::new(
            id,
            patterns.into_iter().map(EventPattern::new).collect(),
            description,
        );
        self.event_bus.subscribe(subscription)
    }

    /// Get a reference to the clock
    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Get a reference to the adapters
    pub fn adapters(&self) -> &A {
        &self.adapters
    }

    /// Load state from store on startup
    pub fn load(&mut self) -> Result<(), EngineError> {
        // Load pipelines
        for id in self.store.list_pipelines()? {
            let pipeline = self.store.load_pipeline(&id)?;
            self.pipelines.insert(pipeline.id.clone(), pipeline);
        }

        // Load workspaces
        for id in self.store.list_workspaces()? {
            let workspace = self.store.load_workspace(&id)?;
            self.workspaces.insert(workspace.id.clone(), workspace);
        }

        // Load tasks
        for id in self.store.list_tasks()? {
            let task = self.store.load_task(&id)?;
            self.tasks.insert(task.id.clone(), task);
        }

        Ok(())
    }

    /// Add a pipeline to the engine
    pub fn add_pipeline(&mut self, pipeline: Pipeline) -> Result<(), EngineError> {
        self.store.save_pipeline(&pipeline)?;
        self.pipelines.insert(pipeline.id.clone(), pipeline);
        Ok(())
    }

    /// Get a pipeline by ID
    pub fn get_pipeline(&self, id: &PipelineId) -> Option<&Pipeline> {
        self.pipelines.get(id)
    }

    /// Add a task to the engine
    pub fn add_task(&mut self, task: Task) -> Result<(), EngineError> {
        self.store.save_task(&task)?;
        self.tasks.insert(task.id.clone(), task);
        Ok(())
    }

    /// Get a task by ID
    pub fn get_task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Get current task for a pipeline
    pub fn current_task_for_pipeline(&self, pipeline_id: &PipelineId) -> Option<&Task> {
        let pipeline = self.pipelines.get(pipeline_id)?;
        let task_id = pipeline.current_task_id.as_ref()?;
        self.tasks.get(task_id)
    }

    /// Add a workspace to the engine
    pub fn add_workspace(&mut self, workspace: Workspace) -> Result<(), EngineError> {
        self.store.save_workspace(&workspace)?;
        self.workspaces.insert(workspace.id.clone(), workspace);
        Ok(())
    }

    /// Get a workspace by ID
    pub fn get_workspace(&self, id: &WorkspaceId) -> Option<&Workspace> {
        self.workspaces.get(id)
    }

    /// Find pipeline by workspace ID
    pub fn find_pipeline_by_workspace(&self, workspace_id: &WorkspaceId) -> Option<&Pipeline> {
        self.pipelines
            .values()
            .find(|p| p.workspace_id.as_ref() == Some(workspace_id))
    }

    /// Find task by session ID
    pub fn find_task_by_session(&self, session_id: &str) -> Option<&Task> {
        self.tasks.values().find(|t| {
            t.session_id
                .as_ref()
                .map(|s| s.0 == session_id)
                .unwrap_or(false)
        })
    }

    /// Find workspace for a pipeline
    pub fn find_workspace_for_pipeline(&self, pipeline_id: &PipelineId) -> Option<&Workspace> {
        let pipeline = self.pipelines.get(pipeline_id)?;
        let workspace_id = pipeline.workspace_id.as_ref()?;
        self.workspaces.get(workspace_id)
    }

    /// Process a pipeline event, execute effects, handle feedback
    pub fn process_pipeline_event<'a>(
        &'a mut self,
        pipeline_id: &'a PipelineId,
        event: PipelineEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EngineError>> + Send + 'a>>
    {
        Box::pin(async move {
            let pipeline = self
                .pipelines
                .get(pipeline_id)
                .ok_or_else(|| EngineError::PipelineNotFound(pipeline_id.clone()))?;

            let (new_pipeline, effects) = pipeline.transition(event, &self.clock);

            // Persist state first (crash safety)
            self.store.save_pipeline(&new_pipeline)?;
            self.pipelines.insert(pipeline_id.clone(), new_pipeline);

            // Execute effects, collecting any failure events
            let mut feedback_events = Vec::new();
            for effect in effects {
                match self.execute_effect(effect).await {
                    EffectResult::Ok => {}
                    EffectResult::Failed { event } => feedback_events.push(event),
                    EffectResult::Retry { after } => {
                        tracing::warn!(?after, "effect requires retry");
                    }
                }
            }

            // Process feedback events
            for event in feedback_events {
                self.process_event(event).await?;
            }

            Ok(())
        })
    }

    /// Process task event and cascade to pipeline
    pub async fn process_task_event(
        &mut self,
        task_id: &TaskId,
        event: TaskEvent,
    ) -> Result<(), EngineError> {
        let task = self
            .tasks
            .get(task_id)
            .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;

        let (new_task, effects) = task.transition(event, &self.clock);

        // Persist and update cache
        self.store.save_task(&new_task)?;
        let pipeline_id = new_task.pipeline_id.clone();
        let is_terminal = new_task.is_terminal();
        self.tasks.insert(task_id.clone(), new_task);

        // Execute effects
        for effect in effects {
            self.execute_effect(effect).await;
        }

        // Cascade to pipeline if task completed
        if is_terminal {
            let task = self
                .tasks
                .get(task_id)
                .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;
            let pipeline_event = match &task.state {
                TaskState::Done { output } => PipelineEvent::TaskComplete {
                    task_id: task_id.clone(),
                    output: output.clone(),
                },
                TaskState::Failed { reason } => PipelineEvent::TaskFailed {
                    task_id: task_id.clone(),
                    reason: reason.clone(),
                },
                _ => return Ok(()),
            };

            self.process_pipeline_event(&pipeline_id, pipeline_event)
                .await?;
        }

        Ok(())
    }

    /// Execute a single effect
    async fn execute_effect(&mut self, effect: Effect) -> EffectResult {
        match effect {
            Effect::Emit(event) => {
                // Log event
                if let Some(ref mut log) = self.event_log {
                    if let Err(e) = log.append(event.clone()) {
                        tracing::warn!(?e, "failed to log event");
                    }
                }

                // Check if notification needed
                if let Some(notification) = self.notify_config.to_notification(&event) {
                    if let Err(e) = self.adapters.notify().notify(notification).await {
                        tracing::warn!(?e, "failed to send notification");
                    }
                }

                // Route through event bus
                self.event_bus.publish(event.clone());

                tracing::info!(event = ?event.name(), "event emitted");
                EffectResult::Ok
            }

            Effect::SpawnSession { name, cwd, command } => {
                match self.adapters.sessions().spawn(&name, &cwd, &command).await {
                    Ok(_) => EffectResult::Ok,
                    Err(e) => EffectResult::Failed {
                        event: Event::SessionDead {
                            id: name,
                            reason: e.to_string(),
                        },
                    },
                }
            }

            Effect::KillSession { name } => {
                let id = SessionId(name.clone());
                match self.adapters.sessions().kill(&id).await {
                    Ok(_) => EffectResult::Ok,
                    Err(e) => {
                        tracing::warn!(session = %name, error = %e, "failed to kill session");
                        EffectResult::Ok // Killing is best-effort
                    }
                }
            }

            Effect::SendToSession { name, input } => {
                let id = SessionId(name.clone());
                match self.adapters.sessions().send(&id, &input).await {
                    Ok(_) => EffectResult::Ok,
                    Err(e) => EffectResult::Failed {
                        event: Event::SessionDead {
                            id: name,
                            reason: e.to_string(),
                        },
                    },
                }
            }

            Effect::CreateWorktree { branch, path } => {
                match self.adapters.repos().worktree_add(&branch, &path).await {
                    Ok(_) => EffectResult::Ok,
                    Err(_e) => EffectResult::Failed {
                        event: Event::WorkspaceDeleted {
                            id: path.to_string_lossy().to_string(),
                        },
                    },
                }
            }

            Effect::RemoveWorktree { path } => {
                match self.adapters.repos().worktree_remove(&path).await {
                    Ok(_) => EffectResult::Ok,
                    Err(e) => {
                        tracing::warn!(path = ?path, error = %e, "failed to remove worktree");
                        EffectResult::Ok // Cleanup is best-effort
                    }
                }
            }

            Effect::Merge {
                path,
                branch,
                strategy,
            } => match self.adapters.repos().merge(&path, &branch, strategy).await {
                Ok(MergeResult::Success)
                | Ok(MergeResult::FastForwarded)
                | Ok(MergeResult::Rebased) => EffectResult::Ok,
                Ok(MergeResult::Conflict { files }) => EffectResult::Failed {
                    event: Event::PipelineFailed {
                        id: "unknown".to_string(),
                        reason: format!("merge conflict in: {}", files.join(", ")),
                    },
                },
                Err(e) => EffectResult::Failed {
                    event: Event::PipelineFailed {
                        id: "unknown".to_string(),
                        reason: e.to_string(),
                    },
                },
            },

            Effect::SaveState { kind, id } => {
                tracing::debug!(kind, id, "save state (handled by caller)");
                EffectResult::Ok
            }

            Effect::SaveCheckpoint {
                pipeline_id,
                checkpoint,
            } => {
                tracing::info!(?pipeline_id, seq = checkpoint.sequence, "checkpoint saved");
                EffectResult::Ok
            }

            Effect::ScheduleTask { task_id, delay } => {
                tracing::debug!(?task_id, ?delay, "task scheduled (handled by scheduler)");
                EffectResult::Ok
            }

            Effect::CancelTask { task_id } => {
                tracing::debug!(?task_id, "task cancelled");
                EffectResult::Ok
            }

            Effect::SetTimer { id, duration } => {
                tracing::debug!(id, ?duration, "timer set (handled by scheduler)");
                EffectResult::Ok
            }

            Effect::CancelTimer { id } => {
                tracing::debug!(id, "timer cancelled");
                EffectResult::Ok
            }

            Effect::Log { level, message } => {
                match level {
                    LogLevel::Debug => tracing::debug!("{}", message),
                    LogLevel::Info => tracing::info!("{}", message),
                    LogLevel::Warn => tracing::warn!("{}", message),
                    LogLevel::Error => tracing::error!("{}", message),
                }
                EffectResult::Ok
            }
        }
    }

    /// Route an event to the appropriate state machine
    pub fn process_event<'a>(
        &'a mut self,
        event: Event,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EngineError>> + Send + 'a>>
    {
        Box::pin(async move {
            match event {
                Event::TaskStuck { id, .. } => self.handle_stuck_task(&id).await,
                Event::SessionDead { id, reason } => {
                    if let Some(task) = self.find_task_by_session(&id) {
                        let task_id = task.id.clone();
                        self.process_task_event(&task_id, TaskEvent::Fail { reason })
                            .await?;
                    }
                    Ok(())
                }
                Event::TaskFailed { id, reason } => {
                    // Route to pipeline
                    if let Some(task) = self.tasks.get(&id) {
                        let pipeline_id = task.pipeline_id.clone();
                        self.process_pipeline_event(
                            &pipeline_id,
                            PipelineEvent::TaskFailed {
                                task_id: id,
                                reason,
                            },
                        )
                        .await?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        })
    }

    /// Handle a stuck task with recovery chain
    pub async fn handle_stuck_task(&mut self, task_id: &TaskId) -> Result<(), EngineError> {
        use crate::engine::recovery::{RecoveryAction, RecoveryConfig};

        let task = self
            .tasks
            .get(task_id)
            .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;

        if !task.is_stuck() {
            return Ok(());
        }

        let config = RecoveryConfig::default();
        let recovery = self.recovery_states.entry(task_id.clone()).or_default();

        let now = self.clock.now();
        let action = recovery.next_action(task, &config, now);

        match action {
            RecoveryAction::Nudge => {
                tracing::info!(?task_id, "nudging stuck task");

                if let Some(session_id) = &task.session_id {
                    self.adapters
                        .sessions()
                        .send(&SessionId(session_id.0.clone()), &config.nudge_message)
                        .await
                        .ok();
                }

                // Update recovery state
                if let Some(recovery) = self.recovery_states.get_mut(task_id) {
                    recovery.record_nudge(now);
                }

                self.process_task_event(task_id, TaskEvent::Nudged).await?;
            }

            RecoveryAction::Restart => {
                tracing::warn!(?task_id, "restarting stuck task");

                // Kill existing session
                if let Some(session_id) = &task.session_id {
                    self.adapters
                        .sessions()
                        .kill(&SessionId(session_id.0.clone()))
                        .await
                        .ok();
                }

                // Spawn new session
                let new_session_id = self.spawn_task_session(task).await?;

                // Update recovery state
                if let Some(recovery) = self.recovery_states.get_mut(task_id) {
                    recovery.record_restart(now);
                }

                self.process_task_event(
                    task_id,
                    TaskEvent::Restart {
                        session_id: new_session_id,
                    },
                )
                .await?;
            }

            RecoveryAction::Escalate => {
                tracing::error!(?task_id, "escalating stuck task - recovery exhausted");

                if let Some(recovery) = self.recovery_states.get_mut(task_id) {
                    recovery.record_escalation();
                }

                self.process_event(Event::TaskFailed {
                    id: task_id.clone(),
                    reason: "recovery exhausted - manual intervention required".to_string(),
                })
                .await?;
            }

            RecoveryAction::Wait { until } => {
                tracing::debug!(?task_id, ?until, "waiting for recovery cooldown");
            }

            RecoveryAction::None => {}
        }

        Ok(())
    }

    /// Spawn a session for a task
    async fn spawn_task_session(
        &self,
        task: &Task,
    ) -> Result<crate::session::SessionId, EngineError> {
        let workspace = self
            .find_workspace_for_pipeline(&task.pipeline_id)
            .ok_or_else(|| {
                EngineError::WorkspaceNotFound(WorkspaceId(format!(
                    "for-pipeline-{}",
                    task.pipeline_id.0
                )))
            })?;

        let session_name = format!("oj-{}-{}", task.pipeline_id.0, task.phase);
        let command = "claude";

        self.adapters
            .sessions()
            .spawn(&session_name, &workspace.path, command)
            .await
            .map_err(|e| EngineError::Adapter(e.to_string()))?;

        Ok(crate::session::SessionId(session_name))
    }

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
    pub async fn process_heartbeat(
        &mut self,
        session_id: &crate::session::SessionId,
    ) -> Result<(), EngineError> {
        // Find and update associated task
        if let Some(task) = self.find_task_by_session(&session_id.0) {
            let task_id = task.id.clone();
            self.process_task_event(
                &task_id,
                TaskEvent::Heartbeat {
                    timestamp: self.clock.now(),
                },
            )
            .await?;
        }

        Ok(())
    }

    /// Monitor sessions and generate heartbeats
    pub async fn poll_sessions(&mut self) -> Result<(), EngineError> {
        let session_ids: Vec<_> = self
            .tasks
            .values()
            .filter_map(|t| t.session_id.clone())
            .collect();

        for session_id in session_ids {
            let adapter_id = SessionId(session_id.0.clone());

            // Check if session is alive
            let is_alive = self
                .adapters
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
                .adapters
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
    pub async fn tick_all_tasks(&mut self) -> Result<(), EngineError> {
        let task_ids: Vec<_> = self
            .tasks
            .values()
            .filter(|t| t.is_running() || t.is_stuck())
            .map(|t| t.id.clone())
            .collect();

        for task_id in task_ids {
            self.process_task_event(&task_id, TaskEvent::Tick).await?;
        }

        Ok(())
    }

    /// Tick queue to handle visibility timeouts
    pub fn tick_queue(&mut self, queue_name: &str) -> Result<(), EngineError> {
        let queue = self.store.load_queue(queue_name)?;
        let (new_queue, effects) = queue.transition(crate::queue::QueueEvent::Tick, &self.clock);
        self.store.save_queue(queue_name, &new_queue)?;

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
        pipeline_id: &PipelineId,
    ) -> Result<TaskId, EngineError> {
        // Extract the info we need before borrowing mutably
        let (task_id, phase_name) = {
            let pipeline = self
                .pipelines
                .get(pipeline_id)
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
            &self.clock,
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
                .adapters
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FakeAdapters;
    use crate::clock::FakeClock;

    fn make_test_engine() -> (Engine<FakeAdapters, FakeClock>, FakeClock) {
        let adapters = FakeAdapters::new();
        let store = JsonStore::open_temp().unwrap();
        let clock = FakeClock::new();
        let engine = Engine::new(adapters, store, clock.clone());
        (engine, clock)
    }

    #[tokio::test]
    async fn engine_can_add_and_get_pipeline() {
        let (mut engine, _clock) = make_test_engine();

        let pipeline = Pipeline::new_build("p-1", "test", "Test prompt");
        engine.add_pipeline(pipeline.clone()).unwrap();

        let loaded = engine.get_pipeline(&pipeline.id);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "test");
    }

    #[tokio::test]
    async fn engine_processes_pipeline_events() {
        let (mut engine, _clock) = make_test_engine();

        let pipeline = Pipeline::new_build("p-1", "test", "Test prompt");
        engine.add_pipeline(pipeline.clone()).unwrap();

        // Transition from Init to Plan
        engine
            .process_pipeline_event(&pipeline.id, PipelineEvent::PhaseComplete)
            .await
            .unwrap();

        let updated = engine.get_pipeline(&pipeline.id).unwrap();
        assert_eq!(updated.phase.name(), "plan");
    }

    #[tokio::test]
    async fn engine_can_add_workspace() {
        let (mut engine, _clock) = make_test_engine();

        let workspace = Workspace::new_ready(
            "ws-1",
            "test",
            std::path::PathBuf::from("/tmp/test"),
            "feature-x",
        );
        engine.add_workspace(workspace.clone()).unwrap();

        let loaded = engine.get_workspace(&workspace.id);
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn engine_finds_pipeline_by_workspace() {
        let (mut engine, _clock) = make_test_engine();

        let workspace_id = WorkspaceId("ws-1".to_string());
        let workspace = Workspace::new_ready(
            "ws-1",
            "test",
            std::path::PathBuf::from("/tmp/test"),
            "feature-x",
        );
        engine.add_workspace(workspace).unwrap();

        let pipeline =
            Pipeline::new_build("p-1", "test", "Test prompt").with_workspace(workspace_id.clone());
        engine.add_pipeline(pipeline.clone()).unwrap();

        let found = engine.find_pipeline_by_workspace(&workspace_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id.0, "p-1");
    }
}

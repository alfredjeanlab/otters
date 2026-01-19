// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Main engine for orchestrating state machines and executing effects

use crate::adapters::{MergeResult, NotifyAdapter, RepoAdapter, SessionAdapter, SessionId};
use crate::clock::Clock;
use crate::config::NotifyConfig;
use crate::effect::{Effect, Event, LogLevel};
use crate::engine::executor::Adapters;
use crate::engine::scheduler::{ScheduledKind, Scheduler};
use crate::events::{EventBus, EventLog, EventPattern, EventReceiver, Subscription};
use crate::pipeline::{Pipeline, PipelineEvent, PipelineId};
use crate::pipelines::dynamic::{create_pipeline, DynamicError};
use crate::runbook::{RunbookRegistry, TemplateEngine};
use crate::scheduling::{
    ActionExecutor, ActionId, ActionResult, ActionState, AlwaysTrueEvaluator, CleanupError,
    CleanupExecutor, CleanupResult, CronController, CronControllerReadonly, CronId, CronState,
    DefaultResourceScanner, DefaultSourceFetcher, ExecutionContext, FetchExecutor, FetchRequest,
    FetchResults, NoOpCommandRunner, NoOpCoordinationCleanup, NoOpSessionCleanup,
    NoOpStorageCleanup, NoOpTaskStarter, NoOpWorktreeCleanup, ScannerId, SchedulingManager,
    SourceFetcher, SourceValue, WatcherEventBridge, WatcherId,
};
use crate::session::Session;
use crate::storage::{WalStore, WalStoreError};
use crate::task::{Task, TaskEvent, TaskId, TaskState};
use crate::workspace::{Workspace, WorkspaceId};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("storage error: {0}")]
    Storage(#[from] WalStoreError),
    #[error("adapter error: {0}")]
    Adapter(String),
    #[error("pipeline not found: {0}")]
    PipelineNotFound(PipelineId),
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(WorkspaceId),
    #[error("runbook not found: {0}")]
    RunbookNotFound(String),
    #[error("pipeline definition not found: {runbook}/{pipeline}")]
    PipelineDefNotFound { runbook: String, pipeline: String },
    #[error("dynamic pipeline error: {0}")]
    DynamicPipeline(#[from] DynamicError),
    #[error("runbook load error: {0}")]
    RunbookLoad(#[from] crate::runbook::LoadError),
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
    store: WalStore,
    clock: C,

    // In-memory state caches (authoritative state is in store)
    pipelines: HashMap<PipelineId, Pipeline>,
    tasks: HashMap<TaskId, Task>,
    workspaces: HashMap<WorkspaceId, Workspace>,
    #[allow(dead_code)] // Epic 6: Strategy & Runbook System - session tracking
    sessions: HashMap<SessionId, Session>,

    // Recovery state tracking
    recovery_states: HashMap<TaskId, crate::engine::recovery::RecoveryState>,

    // Events system
    event_bus: EventBus,
    event_log: Option<EventLog>,
    notify_config: NotifyConfig,

    // Runbook system (Epic 6)
    runbook_registry: Option<RunbookRegistry>,
    template_engine: TemplateEngine,

    // Scheduling system (Epic 8)
    scheduler: Scheduler,
    scheduling_manager: SchedulingManager,

    // Integration layer (Epic 8b)
    watcher_bridge: WatcherEventBridge,
}

impl<A: Adapters, C: Clock> Engine<A, C> {
    pub fn new(adapters: A, store: WalStore, clock: C) -> Self {
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
            runbook_registry: None,
            template_engine: TemplateEngine::new(),
            scheduler: Scheduler::new(),
            scheduling_manager: SchedulingManager::new(),
            watcher_bridge: WatcherEventBridge::new(),
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

    /// Load runbooks from a directory
    ///
    /// This loads all `.toml` files from the specified directory into
    /// the runbook registry, making their pipeline definitions available
    /// for `create_runbook_pipeline`.
    pub fn load_runbooks(&mut self, dir: &Path) -> Result<(), EngineError> {
        let mut registry = self.runbook_registry.take().unwrap_or_default();
        registry.load_directory(dir)?;
        self.runbook_registry = Some(registry);
        Ok(())
    }

    /// Create a pipeline from a runbook definition
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the new pipeline
    /// * `runbook` - Name of the runbook (file name without extension)
    /// * `pipeline` - Name of the pipeline within the runbook
    /// * `inputs` - Input values for the pipeline
    ///
    /// # Returns
    ///
    /// The created pipeline, already added to the engine and persisted.
    pub fn create_runbook_pipeline(
        &mut self,
        id: impl Into<String>,
        runbook: &str,
        pipeline: &str,
        inputs: BTreeMap<String, String>,
    ) -> Result<Pipeline, EngineError> {
        let registry = self
            .runbook_registry
            .as_ref()
            .ok_or_else(|| EngineError::RunbookNotFound("no runbooks loaded".to_string()))?;

        let runbook_def = registry
            .get(runbook)
            .ok_or_else(|| EngineError::RunbookNotFound(runbook.to_string()))?;

        let pipeline_def = runbook_def.pipelines.get(pipeline).ok_or_else(|| {
            EngineError::PipelineDefNotFound {
                runbook: runbook.to_string(),
                pipeline: pipeline.to_string(),
            }
        })?;

        let new_pipeline = create_pipeline(id, pipeline_def, inputs, &self.clock)?;

        // Persist and cache
        self.store.save_pipeline(&new_pipeline)?;
        self.pipelines
            .insert(new_pipeline.id.clone(), new_pipeline.clone());

        Ok(new_pipeline)
    }

    /// Get a reference to the runbook registry
    pub fn runbook_registry(&self) -> Option<&RunbookRegistry> {
        self.runbook_registry.as_ref()
    }

    /// Get a reference to the template engine
    pub fn template_engine(&self) -> &TemplateEngine {
        &self.template_engine
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

    /// Get a mutable reference to the store
    pub(crate) fn store_mut(&mut self) -> &mut WalStore {
        &mut self.store
    }

    /// Get a reference to the tasks map
    pub(crate) fn tasks(&self) -> &HashMap<TaskId, Task> {
        &self.tasks
    }

    /// Get a reference to the sessions map
    pub(crate) fn sessions(&self) -> &HashMap<SessionId, Session> {
        &self.sessions
    }

    /// Get a mutable reference to the sessions map
    pub(crate) fn sessions_mut(&mut self) -> &mut HashMap<SessionId, Session> {
        &mut self.sessions
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
                let fire_at = self.clock.now() + duration;
                self.scheduler
                    .schedule(&id, fire_at, ScheduledKind::Timer { id: id.clone() });
                tracing::debug!(id, ?duration, "timer set");
                EffectResult::Ok
            }

            Effect::CancelTimer { id } => {
                self.scheduler.cancel(&id);
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

    // ==================== Scheduling System (Epic 8) ====================

    /// Initialize the scheduling system with default schedules
    ///
    /// This sets up the default timers and should be called after `load()`.
    pub fn init_scheduling(&mut self) {
        self.scheduler.init_defaults(&self.clock);

        // Register watchers with event bridge for wake_on patterns
        let watcher_registrations: Vec<_> = self
            .scheduling_manager
            .watchers()
            .filter(|w| !w.wake_on.is_empty())
            .map(|w| (w.id.clone(), w.wake_on.clone()))
            .collect();

        for (id, patterns) in watcher_registrations {
            self.watcher_bridge.register(id, patterns);
        }

        // Schedule enabled crons
        let crons_to_schedule: Vec<_> = self
            .scheduling_manager
            .crons()
            .filter(|c| c.state == CronState::Enabled)
            .map(|c| (c.id.clone(), c.interval))
            .collect();

        for (id, interval) in crons_to_schedule {
            self.scheduler.schedule(
                format!("cron:{}", id.0),
                self.clock.now() + interval,
                ScheduledKind::Timer {
                    id: format!("cron:{}", id.0),
                },
            );
        }

        let watcher_count = self.scheduling_manager.watchers().count();
        let scanner_count = self.scheduling_manager.scanners().count();
        let cron_count = self.scheduling_manager.crons().count();
        let action_count = self.scheduling_manager.actions().count();

        tracing::info!(
            watchers = watcher_count,
            scanners = scanner_count,
            crons = cron_count,
            actions = action_count,
            "scheduling initialized"
        );
    }

    /// Handle a cron tick with full integration (two-phase)
    ///
    /// This method uses a two-phase approach to avoid borrow conflicts:
    /// 1. **Planning phase**: Determine what needs to be fetched (readonly access)
    /// 2. **Execution phase**: Fetch values using production adapters, then evaluate
    fn on_cron_tick(&mut self, cron_id: &CronId) -> Vec<Effect> {
        // Phase 0: Tick the cron state machine (Enabled -> Running)
        let cron_effects = self.scheduling_manager.tick_cron(cron_id, &self.clock);

        // Phase 1: Plan what needs to be fetched (no state access needed)
        let fetch_batch = {
            let readonly = CronControllerReadonly::new(&self.scheduling_manager);
            readonly.plan_cron_tick(cron_id)
        };

        // Phase 2: Execute fetches using production adapters
        let fetch_results = if !fetch_batch.is_empty() {
            let state = self.store.state();
            let source_fetcher = DefaultSourceFetcher::new(state);
            let resource_scanner = DefaultResourceScanner::new(state, &self.clock);
            let executor = FetchExecutor::new(&source_fetcher, &resource_scanner);
            executor.execute(fetch_batch)
        } else {
            FetchResults::default()
        };

        // Phase 3: Execute with fetched results (mutable access to scheduling_manager)
        let orchestration_effects = {
            let mut controller =
                CronController::new_for_execution(&mut self.scheduling_manager, &self.clock);
            controller.execute_cron_tick_with_results(cron_id, &fetch_results)
        };

        // Phase 4: Complete the cron (Running -> Enabled, also emits SetTimer for rescheduling)
        let completion_effects = self.scheduling_manager.complete_cron(cron_id, &self.clock);

        [cron_effects, orchestration_effects, completion_effects].concat()
    }

    /// Execute a scheduling effect, returning any nested effects
    fn execute_scheduling_effect(&mut self, effect: &Effect) -> Vec<Effect> {
        match effect {
            Effect::Emit(Event::ActionTriggered { id, source }) => {
                self.handle_action_triggered(id, source)
            }
            Effect::Emit(event) => {
                let mut nested = Vec::new();

                // Wake watchers subscribed to this event
                let event_name = event.name();
                let watchers_to_wake: Vec<_> = self
                    .watcher_bridge
                    .watchers_for_event(&event_name)
                    .into_iter()
                    .collect();

                for watcher_id in watchers_to_wake {
                    let effects = self.check_watcher_immediate(&watcher_id);
                    nested.extend(effects);
                }

                // Handle cleanup events
                if self.is_cleanup_event(event) {
                    if let Err(e) = self.execute_cleanup_effect(effect) {
                        tracing::error!(?e, "cleanup failed");
                    }
                }

                // Publish to event bus for other subscribers
                self.event_bus.publish(event.clone());

                nested
            }
            Effect::SetTimer { id, duration } => {
                self.scheduler.schedule(
                    id.clone(),
                    self.clock.now() + *duration,
                    ScheduledKind::Timer { id: id.clone() },
                );
                Vec::new()
            }
            Effect::CancelTimer { id } => {
                self.scheduler.cancel(id);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Check if an event is a cleanup event from a scanner
    fn is_cleanup_event(&self, event: &Event) -> bool {
        matches!(
            event,
            Event::ScannerDeleteResource { .. }
                | Event::ScannerReleaseResource { .. }
                | Event::ScannerArchiveResource { .. }
                | Event::ScannerFailResource { .. }
                | Event::ScannerDeadLetterResource { .. }
        )
    }

    /// Check a watcher immediately (event-driven wake)
    ///
    /// Uses two-phase approach to fetch the watcher's source value
    /// using production adapters.
    fn check_watcher_immediate(&mut self, watcher_id: &WatcherId) -> Vec<Effect> {
        // Phase 1: Plan fetch
        let fetch_request = {
            let readonly = CronControllerReadonly::new(&self.scheduling_manager);
            readonly.plan_watcher_check(watcher_id)
        };

        let Some(request) = fetch_request else {
            return Vec::new();
        };

        // Phase 2: Execute fetch using production adapters
        let value = {
            let state = self.store.state();
            let source_fetcher = DefaultSourceFetcher::new(state);
            match request {
                FetchRequest::WatcherSource {
                    source, context, ..
                } => source_fetcher.fetch(&source, &context),
                _ => return Vec::new(),
            }
        };

        // Phase 3: Evaluate with result
        match value {
            Ok(v) => self
                .scheduling_manager
                .check_watcher(watcher_id, v, &self.clock),
            Err(e) => {
                tracing::warn!(?watcher_id, error = %e, "failed to fetch watcher source");
                // Check with error value so watcher can track consecutive failures
                self.scheduling_manager.check_watcher(
                    watcher_id,
                    SourceValue::Error {
                        message: e.to_string(),
                    },
                    &self.clock,
                )
            }
        }
    }

    /// Handle action triggered event
    fn handle_action_triggered(&mut self, action_id: &str, source: &str) -> Vec<Effect> {
        let action_id = ActionId::new(action_id);
        let mut effects = Vec::new();

        // First, trigger the action to transition it from Ready -> Executing
        // This is needed because watchers emit ActionTriggered directly without
        // first calling trigger_action on the scheduling manager
        let trigger_effects =
            self.scheduling_manager
                .trigger_action(&action_id, source, &self.clock);
        effects.extend(trigger_effects);

        let action = match self.scheduling_manager.get_action(&action_id) {
            Some(a) => a.clone(),
            None => {
                tracing::warn!(?action_id, "action not found");
                return effects;
            }
        };

        // Check if action is now executing (trigger was accepted)
        if !matches!(action.state, ActionState::Executing) {
            tracing::debug!(?action_id, state = ?action.state, "action not in executing state, skipping execution");
            return effects;
        }

        // Determine execution type for WAL
        let execution_type = match &action.execution {
            crate::scheduling::ActionExecution::Command { .. } => "command",
            crate::scheduling::ActionExecution::Task { .. } => "task",
            crate::scheduling::ActionExecution::Rules { .. } => "rules",
            crate::scheduling::ActionExecution::None => "none",
        };

        // Record execution start in WAL
        if let Err(e) = self
            .store
            .action_execution_started(&action_id, source, execution_type)
        {
            tracing::error!(?e, "failed to record action start");
        }

        let start = self.clock.now();
        let context = ExecutionContext::new(source);

        // Create executor with local no-op implementations
        let command_runner = NoOpCommandRunner;
        let task_starter = NoOpTaskStarter;
        let condition_evaluator = AlwaysTrueEvaluator;
        let executor = ActionExecutor::new(&command_runner, &task_starter, &condition_evaluator);

        match executor.execute(&action, &context) {
            Ok(result) => {
                let duration = self.clock.now().duration_since(start);

                // Extract output if present
                let output = match &result {
                    ActionResult::CommandOutput { stdout, .. } => Some(stdout.clone()),
                    ActionResult::TaskStarted { task_id } => {
                        Some(format!("task started: {}", task_id))
                    }
                    ActionResult::RuleMatched { action, .. } => {
                        Some(format!("rule matched: {}", action))
                    }
                    ActionResult::NoOp => None,
                };

                // Record success in WAL
                if let Err(e) = self
                    .store
                    .action_execution_completed(&action_id, true, output, None, duration)
                {
                    tracing::error!(?e, "failed to record action completion");
                }

                // Update action state
                let completion_effects = self
                    .scheduling_manager
                    .complete_action(&action_id, &self.clock);
                effects.extend(completion_effects);

                tracing::info!(?action_id, ?duration, "action executed successfully");
            }
            Err(e) => {
                let duration = self.clock.now().duration_since(start);

                // Record failure in WAL
                if let Err(wal_err) = self.store.action_execution_completed(
                    &action_id,
                    false,
                    None,
                    Some(e.to_string()),
                    duration,
                ) {
                    tracing::error!(?wal_err, "failed to record action failure");
                }

                // Update action state
                let failure_effects =
                    self.scheduling_manager
                        .fail_action(&action_id, e.to_string(), &self.clock);
                effects.extend(failure_effects);

                tracing::error!(?action_id, ?e, "action execution failed");
            }
        }

        effects
    }

    /// Execute cleanup effects from scanners
    fn execute_cleanup_effect(&mut self, effect: &Effect) -> Result<CleanupResult, CleanupError> {
        // Create cleanup executor with local no-op implementations
        let coordination = NoOpCoordinationCleanup;
        let session = NoOpSessionCleanup;
        let worktree = NoOpWorktreeCleanup;
        let storage = NoOpStorageCleanup;
        let cleanup_executor = CleanupExecutor::new(&coordination, &session, &worktree, &storage);

        match cleanup_executor.execute(effect) {
            Ok(result) if result.success => {
                // Record in WAL
                if let Effect::Emit(event) = effect {
                    let scanner_id = self.extract_scanner_id(event);
                    if let Err(e) = self.store.cleanup_executed(
                        &scanner_id,
                        &result.resource_id,
                        &result.action,
                        true,
                        None,
                    ) {
                        tracing::error!(?e, "failed to record cleanup");
                    }
                }
                Ok(result)
            }
            Ok(result) => {
                // Cleanup reported failure
                Err(CleanupError::OperationFailed {
                    message: result.message.unwrap_or_default(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Extract scanner ID from a scanner event
    fn extract_scanner_id(&self, event: &Event) -> ScannerId {
        match event {
            Event::ScannerDeleteResource { scanner_id, .. }
            | Event::ScannerReleaseResource { scanner_id, .. }
            | Event::ScannerArchiveResource { scanner_id, .. }
            | Event::ScannerFailResource { scanner_id, .. }
            | Event::ScannerDeadLetterResource { scanner_id, .. } => ScannerId::new(scanner_id),
            _ => ScannerId::new("unknown"),
        }
    }

    /// Process any due scheduling timers
    ///
    /// Returns effects that need to be executed. Call this method periodically
    /// (e.g., in the main loop) to drive the scheduling system.
    pub fn tick_scheduling(&mut self) -> Vec<Effect> {
        let now = self.clock.now();
        let due_items = self.scheduler.poll(now);

        let mut all_effects = Vec::new();

        for item in due_items {
            match &item.kind {
                ScheduledKind::Timer { id } => {
                    // Check if this is a cron timer
                    if let Some(cron_id_str) = id.strip_prefix("cron:") {
                        let cron_id = CronId::new(cron_id_str);
                        let effects = self.on_cron_tick(&cron_id);
                        all_effects.extend(effects);
                    } else {
                        // Route other timers to scheduling manager
                        let effects = self.scheduling_manager.process_timer(id, &self.clock);
                        all_effects.extend(effects);
                    }
                }
                ScheduledKind::TaskTick => {
                    // Handled by existing task tick logic
                    tracing::debug!("task tick processed");
                }
                ScheduledKind::QueueTick { queue_name } => {
                    tracing::debug!(queue_name, "queue tick processed");
                }
                ScheduledKind::HeartbeatPoll => {
                    tracing::debug!("heartbeat poll processed");
                }
            }
        }

        // Execute effects (may produce nested effects)
        let mut pending = all_effects.clone();
        while !pending.is_empty() {
            let mut next_pending = Vec::new();
            for effect in pending {
                let nested = self.execute_scheduling_effect(&effect);
                next_pending.extend(nested);
            }
            all_effects.extend(next_pending.clone());
            pending = next_pending;
        }

        all_effects
    }

    /// Get a reference to the scheduler
    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    /// Get a mutable reference to the scheduler
    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Get a reference to the scheduling manager
    pub fn scheduling_manager(&self) -> &SchedulingManager {
        &self.scheduling_manager
    }

    /// Get a mutable reference to the scheduling manager
    pub fn scheduling_manager_mut(&mut self) -> &mut SchedulingManager {
        &mut self.scheduling_manager
    }

    /// Get a reference to the watcher event bridge
    pub fn watcher_bridge(&self) -> &WatcherEventBridge {
        &self.watcher_bridge
    }

    /// Get a mutable reference to the watcher event bridge
    pub fn watcher_bridge_mut(&mut self) -> &mut WatcherEventBridge {
        &mut self.watcher_bridge
    }

    /// Register a watcher's wake_on patterns with the event bridge
    pub fn register_watcher_wake_on(
        &mut self,
        watcher_id: crate::scheduling::WatcherId,
        patterns: Vec<String>,
    ) {
        self.watcher_bridge.register(watcher_id, patterns);
    }

    /// Unregister a watcher from the event bridge
    pub fn unregister_watcher(&mut self, watcher_id: &crate::scheduling::WatcherId) {
        self.watcher_bridge.unregister(watcher_id);
    }

    // Signal handling and polling methods are in signals.rs
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

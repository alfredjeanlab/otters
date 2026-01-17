# Epic 3: Engine & Execution Loop

**Root Feature:** `oj-0eb1`

## Overview

Build the imperative shell that executes effects from the functional core. The engine orchestrates state machine transitions, executes effects via adapters, and implements the feedback loop where effect failures become events driving recovery. This epic connects the pure core (Epic 2) to real I/O while maintaining testability through fake adapters.

**Key Changes from Epic 2:**
- Unified `Engine` that orchestrates Pipeline → Task → Session lifecycle
- Real effect execution for all effect variants (not just logging)
- Feedback loop converting effect failures into state machine events
- Recovery action chains: nudge → restart → escalate for stuck tasks
- Tick-based scheduling for timers and heartbeat evaluation
- Contract tests ensuring fake and real adapters behave identically

## Project Structure

```
crates/core/src/
├── lib.rs                      # Update exports
│
├── engine/
│   ├── mod.rs                  # ENHANCE: Export new components
│   ├── executor.rs             # ENHANCE: Real effect execution
│   ├── engine.rs               # NEW: Main orchestration loop
│   ├── scheduler.rs            # NEW: Timer and task scheduling
│   ├── recovery.rs             # NEW: Recovery action chains
│   └── worker.rs               # Keep: Merge queue worker
│
├── adapters/
│   ├── mod.rs                  # ENHANCE: Add AgentAdapter
│   ├── traits.rs               # ENHANCE: Add AgentAdapter trait
│   ├── fake.rs                 # ENHANCE: Configurable failures, response queues
│   ├── claude.rs               # ENHANCE: Real Claude integration
│   ├── tmux.rs                 # Already complete
│   ├── git.rs                  # Already complete
│   └── wk.rs                   # Already complete
│
├── # Existing (unchanged)
├── pipeline.rs
├── task.rs
├── queue.rs
├── session.rs
├── workspace.rs
├── effect.rs
├── clock.rs
├── id.rs
└── storage/

crates/core/tests/
├── contract_tests.rs           # NEW: Adapter contract tests
├── engine_integration.rs       # NEW: Engine integration tests
└── recovery_tests.rs           # NEW: Recovery chain tests
```

## Dependencies

### Additions to Core Crate

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "time", "macros"] }

[dev-dependencies]
tokio-test = "0.4"              # Deterministic async testing
```

No new external dependencies required. The existing tokio setup is sufficient.

## Implementation Phases

### Phase 1: Engine Core & Effect Execution

**Goal**: Build the `Engine` struct that orchestrates state machines and executes all effect variants.

**Deliverables**:
1. `Engine` struct holding state, adapters, and store
2. Complete effect execution for all variants
3. State persistence after each transition
4. Basic pipeline lifecycle: create → run phases → complete

**Key Code**:

```rust
// core/src/engine/engine.rs

use crate::adapters::Adapters;
use crate::clock::Clock;
use crate::effect::{Effect, Event};
use crate::pipeline::{Pipeline, PipelineEvent, PipelineId};
use crate::session::{Session, SessionId};
use crate::storage::JsonStore;
use crate::task::{Task, TaskEvent, TaskId};
use crate::workspace::{Workspace, WorkspaceId};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
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
    Retry { after: std::time::Duration },
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
        }
    }

    /// Load state from store on startup
    pub fn load(&mut self) -> Result<(), EngineError> {
        for id in self.store.list_pipelines()? {
            let pipeline = self.store.load_pipeline(&id)?;
            self.pipelines.insert(pipeline.id.clone(), pipeline);
        }
        // Load other entities...
        Ok(())
    }

    /// Process a pipeline event, execute effects, handle feedback
    pub async fn process_pipeline_event(
        &mut self,
        pipeline_id: &PipelineId,
        event: PipelineEvent,
    ) -> Result<(), EngineError> {
        let pipeline = self.pipelines.get(pipeline_id)
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
                    // Schedule retry (handled by scheduler)
                    tracing::warn!(?after, "effect requires retry");
                }
            }
        }

        // Process feedback events
        for event in feedback_events {
            self.process_event(event).await?;
        }

        Ok(())
    }

    /// Execute a single effect
    async fn execute_effect(&self, effect: Effect) -> EffectResult {
        match effect {
            Effect::Emit(event) => {
                tracing::info!(?event, "event emitted");
                // Events are handled separately
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
                let id = crate::adapters::SessionId(name.clone());
                match self.adapters.sessions().kill(&id).await {
                    Ok(_) => EffectResult::Ok,
                    Err(e) => {
                        tracing::warn!(session = %name, error = %e, "failed to kill session");
                        EffectResult::Ok // Killing is best-effort
                    }
                }
            }

            Effect::SendToSession { name, input } => {
                let id = crate::adapters::SessionId(name.clone());
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
                    Err(e) => EffectResult::Failed {
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

            Effect::Merge { path, branch, strategy } => {
                match self.adapters.repos().merge(&path, &branch, strategy).await {
                    Ok(crate::adapters::MergeResult::Success)
                    | Ok(crate::adapters::MergeResult::FastForwarded)
                    | Ok(crate::adapters::MergeResult::Rebased) => EffectResult::Ok,
                    Ok(crate::adapters::MergeResult::Conflict { files }) => {
                        EffectResult::Failed {
                            event: Event::PipelineFailed {
                                id: "unknown".to_string(), // Caller must handle
                                reason: format!("merge conflict in: {}", files.join(", ")),
                            },
                        }
                    }
                    Err(e) => EffectResult::Failed {
                        event: Event::PipelineFailed {
                            id: "unknown".to_string(),
                            reason: e.to_string(),
                        },
                    },
                }
            }

            Effect::SaveState { kind, id } => {
                // State is saved by caller after transition
                tracing::debug!(kind, id, "save state (handled by caller)");
                EffectResult::Ok
            }

            Effect::SaveCheckpoint { pipeline_id, checkpoint } => {
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
                    crate::effect::LogLevel::Debug => tracing::debug!("{}", message),
                    crate::effect::LogLevel::Info => tracing::info!("{}", message),
                    crate::effect::LogLevel::Warn => tracing::warn!("{}", message),
                    crate::effect::LogLevel::Error => tracing::error!("{}", message),
                }
                EffectResult::Ok
            }
        }
    }

    /// Route an event to the appropriate state machine
    async fn process_event(&mut self, event: Event) -> Result<(), EngineError> {
        match event {
            Event::TaskStuck { id, .. } => {
                // Trigger recovery chain
                self.handle_stuck_task(&id).await
            }
            Event::SessionDead { id, reason } => {
                // Find associated task and fail it
                if let Some(task) = self.find_task_by_session(&id) {
                    self.process_task_event(
                        &task.id.clone(),
                        TaskEvent::Fail { reason },
                    ).await?;
                }
                Ok(())
            }
            // Other events...
            _ => Ok(()),
        }
    }
}
```

**Verification**:
- `Engine::process_pipeline_event` correctly transitions pipeline state
- Effects are executed via adapters
- State is persisted before effect execution
- Effect failures generate appropriate events

---

### Phase 2: Feedback Loop & Event Routing

**Goal**: Implement the feedback loop where effect results and external signals become state machine events.

**Deliverables**:
1. Event routing to appropriate state machines
2. Cross-primitive coordination (pipeline ↔ task ↔ session)
3. Signal handling for `oj done` and `oj checkpoint`
4. Heartbeat detection via session output monitoring

**Key Code**:

```rust
// core/src/engine/engine.rs (additions)

impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Handle external signal from `oj done`
    pub async fn signal_done(
        &mut self,
        workspace_id: &WorkspaceId,
        error: Option<String>,
    ) -> Result<(), EngineError> {
        // Find pipeline for workspace
        let pipeline = self.find_pipeline_by_workspace(workspace_id)?;
        let task_id = pipeline.current_task_id.clone();

        match (error, task_id) {
            (None, Some(task_id)) => {
                // Success - complete task, which advances pipeline
                self.process_task_event(&task_id, TaskEvent::Complete { output: None }).await?;
            }
            (Some(reason), Some(task_id)) => {
                // Failure - fail task
                self.process_task_event(&task_id, TaskEvent::Fail { reason }).await?;
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
        let pipeline = self.find_pipeline_by_workspace(workspace_id)?;
        self.process_pipeline_event(
            &pipeline.id.clone(),
            PipelineEvent::RequestCheckpoint,
        ).await
    }

    /// Process heartbeat from session output monitoring
    pub async fn process_heartbeat(
        &mut self,
        session_id: &SessionId,
    ) -> Result<(), EngineError> {
        // Update session last_output
        if let Some(session) = self.sessions.get_mut(session_id) {
            let now = self.clock.now();
            session.last_output = Some(now);
        }

        // Find and update associated task
        if let Some(task) = self.find_task_by_session(&session_id.0) {
            self.process_task_event(
                &task.id.clone(),
                TaskEvent::Heartbeat { timestamp: self.clock.now() },
            ).await?;
        }

        Ok(())
    }

    /// Monitor sessions and generate heartbeats
    pub async fn poll_sessions(&mut self) -> Result<(), EngineError> {
        let session_ids: Vec<_> = self.sessions.keys().cloned().collect();

        for session_id in session_ids {
            // Check if session is alive
            let is_alive = self.adapters.sessions()
                .is_alive(&crate::adapters::SessionId(session_id.0.clone()))
                .await
                .unwrap_or(false);

            if !is_alive {
                // Session died unexpectedly
                self.process_event(Event::SessionDead {
                    id: session_id.0.clone(),
                    reason: "session terminated".to_string(),
                }).await?;
                continue;
            }

            // Capture pane and check for new output
            let output = self.adapters.sessions()
                .capture_pane(&crate::adapters::SessionId(session_id.0.clone()), 50)
                .await
                .unwrap_or_default();

            let hash = calculate_hash(&output);
            let session = self.sessions.get(&session_id);

            if let Some(session) = session {
                if session.last_output_hash != Some(hash) {
                    // New output detected - heartbeat
                    self.process_heartbeat(&session_id).await?;

                    // Update hash
                    if let Some(session) = self.sessions.get_mut(&session_id) {
                        session.last_output_hash = Some(hash);
                    }
                }
            }
        }

        Ok(())
    }

    /// Process task event and cascade to pipeline
    pub async fn process_task_event(
        &mut self,
        task_id: &TaskId,
        event: TaskEvent,
    ) -> Result<(), EngineError> {
        let task = self.tasks.get(task_id)
            .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;

        let (new_task, effects) = task.transition(event.clone(), &self.clock);

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
            let task = self.tasks.get(task_id).unwrap();
            let pipeline_event = match &task.state {
                crate::task::TaskState::Done { output } => {
                    PipelineEvent::TaskComplete {
                        task_id: task_id.clone(),
                        output: output.clone(),
                    }
                }
                crate::task::TaskState::Failed { reason } => {
                    PipelineEvent::TaskFailed {
                        task_id: task_id.clone(),
                        reason: reason.clone(),
                    }
                }
                _ => return Ok(()),
            };

            self.process_pipeline_event(&pipeline_id, pipeline_event).await?;
        }

        Ok(())
    }
}

fn calculate_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
```

**Verification**:
- `oj done` signal correctly completes/fails tasks
- Task completion cascades to pipeline advancement
- Session death triggers task failure
- Heartbeats detected from output changes

---

### Phase 3: Scheduler & Tick Loop

**Goal**: Implement timer-based scheduling for periodic tasks (heartbeat checks, visibility timeouts, stuck detection).

**Deliverables**:
1. `Scheduler` struct managing timers
2. Periodic tick loop for state machine evaluation
3. Integration with Engine for scheduled events
4. Deterministic scheduling for tests via FakeClock

**Key Code**:

```rust
// core/src/engine/scheduler.rs

use crate::clock::Clock;
use std::collections::BinaryHeap;
use std::cmp::Reverse;
use std::time::{Duration, Instant};

/// A scheduled item
#[derive(Debug, Clone)]
pub struct ScheduledItem {
    pub id: String,
    pub fire_at: Instant,
    pub kind: ScheduledKind,
    pub repeat: Option<Duration>,
}

#[derive(Debug, Clone)]
pub enum ScheduledKind {
    /// Evaluate all tasks for stuck detection
    TaskTick,
    /// Evaluate queue for visibility timeout
    QueueTick { queue_name: String },
    /// Custom timer
    Timer { id: String },
    /// Heartbeat check for sessions
    HeartbeatPoll,
}

impl PartialEq for ScheduledItem {
    fn eq(&self, other: &Self) -> bool {
        self.fire_at == other.fire_at
    }
}

impl Eq for ScheduledItem {}

impl PartialOrd for ScheduledItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: earliest first
        Reverse(self.fire_at).cmp(&Reverse(other.fire_at))
    }
}

/// Manages scheduled events
pub struct Scheduler {
    items: BinaryHeap<ScheduledItem>,
    cancelled: std::collections::HashSet<String>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            items: BinaryHeap::new(),
            cancelled: std::collections::HashSet::new(),
        }
    }

    /// Schedule a one-shot timer
    pub fn schedule(&mut self, id: impl Into<String>, fire_at: Instant, kind: ScheduledKind) {
        self.items.push(ScheduledItem {
            id: id.into(),
            fire_at,
            kind,
            repeat: None,
        });
    }

    /// Schedule a repeating timer
    pub fn schedule_repeating(
        &mut self,
        id: impl Into<String>,
        fire_at: Instant,
        interval: Duration,
        kind: ScheduledKind,
    ) {
        self.items.push(ScheduledItem {
            id: id.into(),
            fire_at,
            kind,
            repeat: Some(interval),
        });
    }

    /// Cancel a scheduled item
    pub fn cancel(&mut self, id: &str) {
        self.cancelled.insert(id.to_string());
    }

    /// Get all items that should fire at or before the given time
    pub fn poll(&mut self, now: Instant) -> Vec<ScheduledItem> {
        let mut ready = Vec::new();

        while let Some(item) = self.items.peek() {
            if item.fire_at > now {
                break;
            }

            let item = self.items.pop().unwrap();

            // Skip cancelled items
            if self.cancelled.contains(&item.id) {
                self.cancelled.remove(&item.id);
                continue;
            }

            // Re-schedule if repeating
            if let Some(interval) = item.repeat {
                self.items.push(ScheduledItem {
                    fire_at: item.fire_at + interval,
                    ..item.clone()
                });
            }

            ready.push(item);
        }

        ready
    }

    /// Initialize default schedules
    pub fn init_defaults(&mut self, clock: &impl Clock) {
        let now = clock.now();

        // Task tick every 30 seconds
        self.schedule_repeating(
            "task-tick",
            now + Duration::from_secs(30),
            Duration::from_secs(30),
            ScheduledKind::TaskTick,
        );

        // Queue tick every 10 seconds
        self.schedule_repeating(
            "queue-tick-merges",
            now + Duration::from_secs(10),
            Duration::from_secs(10),
            ScheduledKind::QueueTick { queue_name: "merges".to_string() },
        );

        // Heartbeat poll every 5 seconds
        self.schedule_repeating(
            "heartbeat-poll",
            now + Duration::from_secs(5),
            Duration::from_secs(5),
            ScheduledKind::HeartbeatPoll,
        );
    }
}

// core/src/engine/engine.rs (additions)

impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Process scheduled items
    pub async fn process_scheduled(&mut self, scheduler: &mut Scheduler) -> Result<(), EngineError> {
        let now = self.clock.now();
        let ready = scheduler.poll(now);

        for item in ready {
            match item.kind {
                ScheduledKind::TaskTick => {
                    self.tick_all_tasks().await?;
                }
                ScheduledKind::QueueTick { queue_name } => {
                    self.tick_queue(&queue_name)?;
                }
                ScheduledKind::Timer { id } => {
                    self.process_event(Event::TimerFired { id }).await?;
                }
                ScheduledKind::HeartbeatPoll => {
                    self.poll_sessions().await?;
                }
            }
        }

        Ok(())
    }

    /// Tick all active tasks to detect stuck state
    async fn tick_all_tasks(&mut self) -> Result<(), EngineError> {
        let task_ids: Vec<_> = self.tasks.keys().cloned().collect();

        for task_id in task_ids {
            let task = self.tasks.get(&task_id).unwrap();
            if task.is_running() || task.is_stuck() {
                self.process_task_event(&task_id, TaskEvent::Tick).await?;
            }
        }

        Ok(())
    }

    /// Tick queue to handle visibility timeouts
    fn tick_queue(&mut self, queue_name: &str) -> Result<(), EngineError> {
        let queue = self.store.load_queue(queue_name)?;
        let (new_queue, effects) = queue.transition(
            crate::queue::QueueEvent::Tick,
            &self.clock,
        );
        self.store.save_queue(queue_name, &new_queue)?;

        // Log any released items
        for effect in effects {
            if let Effect::Emit(event) = effect {
                tracing::info!(?event, "queue tick event");
            }
        }

        Ok(())
    }
}
```

**Verification**:
- Scheduler fires items at correct times
- Repeating timers reschedule correctly
- Cancelled timers don't fire
- TaskTick detects stuck tasks
- QueueTick handles visibility timeouts

---

### Phase 4: Recovery Actions

**Goal**: Implement recovery action chains for stuck tasks (nudge → restart → escalate).

**Deliverables**:
1. Recovery chain configuration
2. Nudge implementation (send prompt to session)
3. Restart implementation (kill and respawn session)
4. Escalation when recovery exhausted
5. Cooldowns between recovery attempts

**Key Code**:

```rust
// core/src/engine/recovery.rs

use crate::task::{Task, TaskState};
use std::time::{Duration, Instant};

/// Configuration for recovery actions
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum nudge attempts before restart
    pub max_nudges: u32,
    /// Cooldown between nudges
    pub nudge_cooldown: Duration,
    /// Maximum restart attempts before escalation
    pub max_restarts: u32,
    /// Cooldown between restarts
    pub restart_cooldown: Duration,
    /// Nudge message to send
    pub nudge_message: String,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_nudges: 3,
            nudge_cooldown: Duration::from_secs(60),
            max_restarts: 2,
            restart_cooldown: Duration::from_secs(300),
            nudge_message: "Are you still working? Please run `oj done` when finished or `oj done --error 'reason'` if stuck.".to_string(),
        }
    }
}

/// Recovery state tracked per task
#[derive(Debug, Clone, Default)]
pub struct RecoveryState {
    pub nudge_count: u32,
    pub restart_count: u32,
    pub last_nudge: Option<Instant>,
    pub last_restart: Option<Instant>,
    pub escalated: bool,
}

/// Determines the next recovery action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Send nudge message to session
    Nudge,
    /// Kill session and restart task
    Restart,
    /// Escalate to user (notifications, alerts)
    Escalate,
    /// Wait for cooldown
    Wait { until: Instant },
    /// No action needed
    None,
}

impl RecoveryState {
    /// Determine next action based on task state and config
    pub fn next_action(
        &self,
        task: &Task,
        config: &RecoveryConfig,
        now: Instant,
    ) -> RecoveryAction {
        // Only act on stuck tasks
        let TaskState::Stuck { since, nudge_count } = &task.state else {
            return RecoveryAction::None;
        };

        // Already escalated
        if self.escalated {
            return RecoveryAction::None;
        }

        // Check nudge cooldown
        if self.nudge_count < config.max_nudges {
            if let Some(last) = self.last_nudge {
                if now < last + config.nudge_cooldown {
                    return RecoveryAction::Wait {
                        until: last + config.nudge_cooldown,
                    };
                }
            }
            return RecoveryAction::Nudge;
        }

        // Check restart cooldown
        if self.restart_count < config.max_restarts {
            if let Some(last) = self.last_restart {
                if now < last + config.restart_cooldown {
                    return RecoveryAction::Wait {
                        until: last + config.restart_cooldown,
                    };
                }
            }
            return RecoveryAction::Restart;
        }

        // All options exhausted
        RecoveryAction::Escalate
    }

    /// Record that a nudge was performed
    pub fn record_nudge(&mut self, now: Instant) {
        self.nudge_count += 1;
        self.last_nudge = Some(now);
    }

    /// Record that a restart was performed
    pub fn record_restart(&mut self, now: Instant) {
        self.restart_count += 1;
        self.last_restart = Some(now);
        // Reset nudge count after restart
        self.nudge_count = 0;
        self.last_nudge = None;
    }

    /// Mark as escalated
    pub fn record_escalation(&mut self) {
        self.escalated = true;
    }
}

// core/src/engine/engine.rs (additions)

use crate::engine::recovery::{RecoveryConfig, RecoveryState, RecoveryAction};

impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Handle a stuck task with recovery chain
    pub async fn handle_stuck_task(&mut self, task_id: &TaskId) -> Result<(), EngineError> {
        let task = self.tasks.get(task_id)
            .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;

        if !task.is_stuck() {
            return Ok(());
        }

        let config = RecoveryConfig::default();
        let recovery = self.recovery_states
            .entry(task_id.clone())
            .or_insert_with(RecoveryState::default);

        let now = self.clock.now();
        let action = recovery.next_action(task, &config, now);

        match action {
            RecoveryAction::Nudge => {
                tracing::info!(?task_id, "nudging stuck task");

                if let Some(session_id) = &task.session_id {
                    self.adapters.sessions()
                        .send(
                            &crate::adapters::SessionId(session_id.0.clone()),
                            &config.nudge_message,
                        )
                        .await
                        .ok(); // Best effort
                }

                recovery.record_nudge(now);
                self.process_task_event(task_id, TaskEvent::Nudged).await?;
            }

            RecoveryAction::Restart => {
                tracing::warn!(?task_id, "restarting stuck task");

                // Kill existing session
                if let Some(session_id) = &task.session_id {
                    self.adapters.sessions()
                        .kill(&crate::adapters::SessionId(session_id.0.clone()))
                        .await
                        .ok();
                }

                // Spawn new session
                let new_session_id = self.spawn_task_session(task).await?;

                recovery.record_restart(now);
                self.process_task_event(
                    task_id,
                    TaskEvent::Restart { session_id: new_session_id },
                ).await?;
            }

            RecoveryAction::Escalate => {
                tracing::error!(?task_id, "escalating stuck task - recovery exhausted");

                recovery.record_escalation();

                // Emit escalation event (handled by notifications in Epic 4)
                self.process_event(Event::TaskFailed {
                    id: task_id.clone(),
                    reason: "recovery exhausted - manual intervention required".to_string(),
                }).await?;
            }

            RecoveryAction::Wait { until } => {
                tracing::debug!(?task_id, ?until, "waiting for recovery cooldown");
            }

            RecoveryAction::None => {}
        }

        Ok(())
    }

    /// Spawn a session for a task
    async fn spawn_task_session(&self, task: &Task) -> Result<SessionId, EngineError> {
        let workspace = self.workspaces.get(&task.pipeline_id)
            .and_then(|_| self.find_workspace_for_pipeline(&task.pipeline_id))
            .ok_or_else(|| EngineError::WorkspaceNotFound(
                WorkspaceId(format!("for-pipeline-{}", task.pipeline_id.0))
            ))?;

        let session_name = format!("oj-{}-{}", task.pipeline_id.0, task.phase);
        let command = "claude"; // Or configured agent command

        self.adapters.sessions()
            .spawn(&session_name, &workspace.path, command)
            .await
            .map_err(|e| EngineError::Adapter(e.to_string()))?;

        Ok(SessionId(session_name))
    }
}
```

**Verification**:
- Nudge sends message to session
- Restart kills old session and spawns new one
- Escalation fires after max attempts
- Cooldowns prevent rapid retries
- Recovery state resets appropriately

---

### Phase 5: Contract Tests & Fake Adapter Enhancements

**Goal**: Ensure fake adapters match real adapter behavior through contract tests; add configurable failure modes.

**Deliverables**:
1. Contract test trait defining expected behaviors
2. Tests running against both FakeAdapters and real adapters
3. Configurable failure injection for fakes
4. Response queue for deterministic test sequences

**Key Code**:

```rust
// core/src/adapters/fake.rs (enhancements)

use std::collections::VecDeque;

/// Configuration for fake adapter responses
pub struct FakeConfig {
    /// Queue of responses for spawn calls
    pub spawn_responses: VecDeque<Result<SessionId, SessionError>>,
    /// Whether send should fail
    pub send_fails: bool,
    /// Whether merge should conflict
    pub merge_conflicts: bool,
    /// Specific sessions to mark as dead
    pub dead_sessions: std::collections::HashSet<String>,
}

impl Default for FakeConfig {
    fn default() -> Self {
        Self {
            spawn_responses: VecDeque::new(),
            send_fails: false,
            merge_conflicts: false,
            dead_sessions: std::collections::HashSet::new(),
        }
    }
}

impl FakeAdapters {
    /// Configure a sequence of spawn responses
    pub fn queue_spawn_response(&self, result: Result<SessionId, SessionError>) {
        let mut state = self.state.lock().unwrap();
        state.config.spawn_responses.push_back(result);
    }

    /// Configure send to fail
    pub fn set_send_fails(&self, fails: bool) {
        let mut state = self.state.lock().unwrap();
        state.config.send_fails = fails;
    }

    /// Configure merges to conflict
    pub fn set_merge_conflicts(&self, conflicts: bool) {
        let mut state = self.state.lock().unwrap();
        state.config.merge_conflicts = conflicts;
    }

    /// Mark a session as dead (is_alive returns false)
    pub fn mark_session_dead(&self, session_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.config.dead_sessions.insert(session_id.to_string());
    }
}

// core/tests/contract_tests.rs

/// Contract tests that verify adapter behavior
/// Run against both fake and real adapters

#[cfg(test)]
mod session_adapter_contracts {
    use super::*;

    /// Test that spawn creates a session that can be killed
    async fn spawn_then_kill_contract<A: SessionAdapter>(adapter: A) {
        let session_id = adapter.spawn("test-session", Path::new("."), "echo test").await.unwrap();
        assert!(adapter.is_alive(&session_id).await.unwrap());

        adapter.kill(&session_id).await.unwrap();
        // Give it a moment to die
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!adapter.is_alive(&session_id).await.unwrap());
    }

    /// Test that send fails for non-existent sessions
    async fn send_to_nonexistent_fails<A: SessionAdapter>(adapter: A) {
        let fake_id = SessionId("nonexistent-session".to_string());
        let result = adapter.send(&fake_id, "test").await;
        assert!(result.is_err());
    }

    /// Test that capture returns content
    async fn capture_returns_content<A: SessionAdapter>(adapter: A) {
        let session_id = adapter.spawn("test-capture", Path::new("."), "echo hello").await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        let content = adapter.capture_pane(&session_id, 10).await.unwrap();
        // Content should include our command or output
        assert!(!content.is_empty());

        adapter.kill(&session_id).await.ok();
    }

    // Run contracts against fake adapter
    #[tokio::test]
    async fn fake_adapter_spawn_then_kill() {
        let fake = FakeAdapters::new();
        spawn_then_kill_contract(fake.sessions()).await;
    }

    #[tokio::test]
    async fn fake_adapter_send_to_nonexistent() {
        let fake = FakeAdapters::new();
        send_to_nonexistent_fails(fake.sessions()).await;
    }

    // Conditionally run against real adapter
    #[tokio::test]
    #[ignore] // Run with --ignored to test real tmux
    async fn real_adapter_spawn_then_kill() {
        let real = TmuxAdapter::new("test-".to_string());
        spawn_then_kill_contract(real).await;
    }
}

#[cfg(test)]
mod repo_adapter_contracts {
    use super::*;

    async fn worktree_lifecycle<A: RepoAdapter>(adapter: A) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test-worktree");

        adapter.worktree_add("main", &path).await.unwrap();

        let trees = adapter.worktree_list().await.unwrap();
        assert!(trees.iter().any(|t| t.path == path));

        adapter.worktree_remove(&path).await.unwrap();
    }

    #[tokio::test]
    async fn fake_worktree_lifecycle() {
        let fake = FakeAdapters::new();
        worktree_lifecycle(fake.repos()).await;
    }
}
```

**Verification**:
- Contract tests pass for FakeAdapters
- Contract tests pass for real adapters (when `--ignored`)
- Failure injection works correctly
- Response queues provide deterministic sequences

---

### Phase 6: Integration Tests

**Goal**: End-to-end tests verifying the full engine lifecycle with fake adapters.

**Deliverables**:
1. Full pipeline execution test
2. Recovery chain test
3. Signal handling test
4. Concurrent operation test

**Key Code**:

```rust
// core/tests/engine_integration.rs

use oj_core::adapters::FakeAdapters;
use oj_core::clock::FakeClock;
use oj_core::engine::{Engine, Scheduler};
use oj_core::pipeline::{Pipeline, PipelineEvent, PipelineKind};
use oj_core::storage::JsonStore;
use std::time::Duration;

#[tokio::test]
async fn full_build_pipeline_lifecycle() {
    let adapters = FakeAdapters::new();
    let store = JsonStore::open_temp().unwrap();
    let clock = FakeClock::new();

    let mut engine = Engine::new(adapters, store, clock.clone());

    // Create a build pipeline
    let pipeline = Pipeline::new_build("test-build", "implement feature X");
    engine.add_pipeline(pipeline.clone());

    // Simulate phase completions
    for _ in 0..4 { // Plan, Decompose, Execute, Merge
        // Signal done
        engine.signal_done(&pipeline.workspace_id.clone().unwrap(), None).await.unwrap();
    }

    // Pipeline should be complete
    let pipeline = engine.get_pipeline(&pipeline.id).unwrap();
    assert!(pipeline.is_complete());
}

#[tokio::test]
async fn stuck_task_recovery_chain() {
    let adapters = FakeAdapters::new();
    let store = JsonStore::open_temp().unwrap();
    let clock = FakeClock::new();

    let mut engine = Engine::new(adapters.clone(), store, clock.clone());
    let mut scheduler = Scheduler::new();
    scheduler.init_defaults(&clock);

    // Create pipeline and start task
    let pipeline = Pipeline::new_build("test-stuck", "test");
    engine.add_pipeline(pipeline.clone());
    engine.start_phase_task(&pipeline.id).await.unwrap();

    // Advance time past stuck threshold
    clock.advance(Duration::from_secs(150));
    engine.process_scheduled(&mut scheduler).await.unwrap();

    // Task should be stuck
    let task = engine.current_task_for_pipeline(&pipeline.id).unwrap();
    assert!(task.is_stuck());

    // Verify nudge was sent
    let calls = adapters.calls();
    assert!(calls.iter().any(|c| matches!(c, AdapterCall::SendToSession { .. })));

    // Advance time and trigger more recovery
    for _ in 0..3 {
        clock.advance(Duration::from_secs(120));
        engine.process_scheduled(&mut scheduler).await.unwrap();
    }

    // Should have attempted restart
    let calls = adapters.calls();
    assert!(calls.iter().filter(|c| matches!(c, AdapterCall::KillSession { .. })).count() >= 1);
}

#[tokio::test]
async fn signal_done_advances_pipeline() {
    let adapters = FakeAdapters::new();
    let store = JsonStore::open_temp().unwrap();
    let clock = FakeClock::new();

    let mut engine = Engine::new(adapters, store, clock.clone());

    let pipeline = Pipeline::new_build("test-signal", "test");
    let workspace_id = pipeline.workspace_id.clone().unwrap();
    engine.add_pipeline(pipeline.clone());
    engine.start_phase_task(&pipeline.id).await.unwrap();

    // Get initial phase
    let initial_phase = engine.get_pipeline(&pipeline.id).unwrap().current_phase_name();

    // Signal done
    engine.signal_done(&workspace_id, None).await.unwrap();

    // Phase should have advanced
    let new_phase = engine.get_pipeline(&pipeline.id).unwrap().current_phase_name();
    assert_ne!(initial_phase, new_phase);
}

#[tokio::test]
async fn effect_failure_triggers_recovery() {
    let adapters = FakeAdapters::new();
    adapters.set_send_fails(true);

    let store = JsonStore::open_temp().unwrap();
    let clock = FakeClock::new();

    let mut engine = Engine::new(adapters, store, clock.clone());

    let pipeline = Pipeline::new_build("test-failure", "test");
    engine.add_pipeline(pipeline.clone());
    engine.start_phase_task(&pipeline.id).await.unwrap();

    // Make task stuck
    clock.advance(Duration::from_secs(150));
    engine.tick_all_tasks().await.unwrap();

    // Nudge should fail but be handled gracefully
    engine.handle_stuck_task(&engine.current_task_for_pipeline(&pipeline.id).unwrap().id).await.unwrap();

    // Engine should still be functional
    assert!(engine.get_pipeline(&pipeline.id).is_some());
}
```

**Verification**:
- Full pipeline completes all phases
- Recovery chain executes nudge → restart → escalate
- Signals correctly advance state
- Effect failures don't crash engine

## Key Implementation Details

### Engine Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Engine                              │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌───────────┐  │
│  │Pipeline │   │  Task    │   │ Session │   │ Workspace │  │
│  │  State  │   │  State   │   │  State  │   │   State   │  │
│  └────┬────┘   └────┬─────┘   └────┬────┘   └─────┬─────┘  │
│       │             │              │              │         │
│       └─────────────┴──────────────┴──────────────┘         │
│                         │                                   │
│                    ┌────▼────┐                              │
│                    │ Effects │                              │
│                    └────┬────┘                              │
│                         │                                   │
├─────────────────────────┼───────────────────────────────────┤
│                    ┌────▼────┐                              │
│                    │Executor │                              │
│                    └────┬────┘                              │
│                         │                                   │
│  ┌──────────────────────┼────────────────────────────────┐  │
│  │                 Adapters                               │  │
│  │  ┌────────┐  ┌──────┐  ┌──────┐  ┌───────┐           │  │
│  │  │Sessions│  │ Repo │  │Issues│  │ Agent │           │  │
│  │  │ (tmux) │  │(git) │  │ (wk) │  │(claude)│          │  │
│  │  └────────┘  └──────┘  └──────┘  └───────┘           │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Feedback Loop Pattern

```rust
// Effect execution returns EffectResult, not just success/failure
enum EffectResult {
    Ok,
    Failed { event: Event },  // Generate recovery event
    Retry { after: Duration }, // Retry later
}

// Engine processes effects and handles feedback
loop {
    let (new_state, effects) = state.transition(event, &clock);
    persist(new_state);

    for effect in effects {
        match execute(effect) {
            EffectResult::Failed { event } => {
                // Feed back into state machine
                process_event(event);
            }
            ...
        }
    }
}
```

### Recovery Chain

```
Task Stuck
    │
    ▼
┌─────────────────┐
│ Nudge (x3)      │ ◄─── Send message to session
└────────┬────────┘
         │ max nudges reached
         ▼
┌─────────────────┐
│ Restart (x2)    │ ◄─── Kill session, spawn new
└────────┬────────┘
         │ max restarts reached
         ▼
┌─────────────────┐
│ Escalate        │ ◄─── Fail task, notify user
└─────────────────┘
```

### Scheduler Design

The scheduler uses a min-heap ordered by fire time, enabling efficient polling:

```rust
let ready = scheduler.poll(now);  // O(k log n) for k ready items
for item in ready {
    process(item);
}
```

Repeating timers automatically re-insert themselves with the next fire time.

## Verification Plan

### Unit Tests

| Module | Key Tests |
|--------|-----------|
| `engine` | Effect execution, state persistence, feedback loop |
| `scheduler` | Timer ordering, cancel, repeat |
| `recovery` | Action selection, cooldowns, state tracking |

### Integration Tests

| Test | Description |
|------|-------------|
| `full_build_pipeline_lifecycle` | Pipeline from Init to Done |
| `stuck_task_recovery_chain` | Nudge → Restart → Escalate |
| `signal_done_advances_pipeline` | External signal handling |
| `effect_failure_triggers_recovery` | Graceful failure handling |
| `concurrent_pipelines` | Multiple pipelines running |

### Contract Tests

| Adapter | Contracts |
|---------|-----------|
| SessionAdapter | spawn/kill lifecycle, send errors, capture content |
| RepoAdapter | worktree lifecycle, merge strategies |
| IssueAdapter | CRUD operations |

### Manual Verification Checklist

- [ ] `cargo test` passes all tests
- [ ] `cargo test -- --ignored` passes contract tests against real adapters
- [ ] Engine recovers from adapter failures gracefully
- [ ] Stuck tasks are detected within threshold
- [ ] Recovery chain progresses correctly
- [ ] State persists correctly across restarts
- [ ] No regressions in Epic 1/2 functionality

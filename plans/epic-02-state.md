# Epic 2: Core State Machines

**Root Feature:** `oj-0eb1`

## Overview

Implement the pure functional core with explicit state machines for Pipeline, Queue, and Task. All state transitions are pure functions returning `(NewState, Vec<Effect>)`. This establishes the foundation for high test coverage and deterministic behavior.

The functional core has zero external dependencies and is 100% unit testable. Effects are data structures describing side effects, not actual I/O. This separation enables property-based testing of state machine invariants and makes the system easier to reason about.

**Key Changes from Epic 1:**
- Add new `Task` state machine (Pending → Running → Stuck → Done)
- Enhance `Queue` with visibility timeout for claimed items
- Add checkpoint/recovery support to `Pipeline`
- Comprehensive property-based tests with proptest
- Parametrized tests with yare

## Project Structure

```
crates/core/src/
├── lib.rs                      # Update exports
│
├── # Pure State Machines (enhanced)
├── task.rs                     # NEW: Task state machine
├── task_tests.rs               # NEW: Task unit tests
├── pipeline.rs                 # ENHANCE: Add checkpoints, pure transitions
├── pipeline_tests.rs           # ENHANCE: Property-based tests
├── queue.rs                    # ENHANCE: Add visibility timeout
├── queue_tests.rs              # ENHANCE: Property-based tests
│
├── # Existing (minor updates)
├── effect.rs                   # Add new effect variants
├── clock.rs                    # Already complete
├── id.rs                       # Already complete
├── workspace.rs                # Already complete
├── session.rs                  # Already complete
│
└── # Proptest infrastructure
    └── proptest/
        ├── mod.rs              # NEW: Proptest module
        ├── generators.rs       # NEW: Arbitrary implementations
        └── invariants.rs       # NEW: State machine invariants

crates/core/Cargo.toml          # Add proptest, yare deps
```

## Dependencies

### Additions to Core Crate

```toml
[dev-dependencies]
proptest = "1"                  # Property-based testing
yare = "3"                      # Parametrized tests
test-strategy = "0.4"           # Proptest derive macros
```

## Implementation Phases

### Phase 1: Task State Machine

**Goal**: Implement the Task primitive with heartbeat-based state transitions.

**Deliverables**:
1. `Task` struct with state machine
2. Task states: Pending → Running → Stuck → Done
3. Heartbeat evaluation logic
4. Effect generation on state changes
5. Unit tests for all transitions

**Key Code**:

```rust
// core/src/task.rs

/// A task represents a unit of work assigned to a session.
/// Unlike Session (which tracks tmux process state), Task tracks
/// the logical work being performed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub pipeline_id: PipelineId,
    pub phase: String,
    pub state: TaskState,
    pub session_id: Option<SessionId>,
    pub heartbeat_interval: Duration,
    pub stuck_threshold: Duration,
    pub last_heartbeat: Option<Instant>,
    pub created_at: Instant,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task created but not yet assigned to a session
    Pending,
    /// Task is actively being worked on
    Running,
    /// Task has not received heartbeat within threshold
    Stuck { since: Instant, nudge_count: u32 },
    /// Task completed successfully
    Done { output: Option<String> },
    /// Task failed
    Failed { reason: String },
}

#[derive(Clone, Debug)]
pub enum TaskEvent {
    /// Session assigned, begin work
    Start { session_id: SessionId },
    /// Heartbeat received from session
    Heartbeat { timestamp: Instant },
    /// Work completed successfully
    Complete { output: Option<String> },
    /// Work failed
    Fail { reason: String },
    /// Nudge attempt made (for stuck tasks)
    Nudged,
    /// Task restarted after being stuck
    Restart { session_id: SessionId },
    /// Evaluate current state (called periodically)
    Tick,
}

impl Task {
    pub fn new(
        id: TaskId,
        pipeline_id: PipelineId,
        phase: String,
        heartbeat_interval: Duration,
        stuck_threshold: Duration,
        clock: &impl Clock,
    ) -> Self {
        Task {
            id,
            pipeline_id,
            phase,
            state: TaskState::Pending,
            session_id: None,
            heartbeat_interval,
            stuck_threshold,
            last_heartbeat: None,
            created_at: clock.now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Pure transition function - returns new state and effects
    pub fn transition(
        &self,
        event: TaskEvent,
        clock: &impl Clock,
    ) -> (Task, Vec<Effect>) {
        let now = clock.now();

        match (&self.state, event) {
            // Pending → Running
            (TaskState::Pending, TaskEvent::Start { session_id }) => {
                let task = Task {
                    state: TaskState::Running,
                    session_id: Some(session_id.clone()),
                    last_heartbeat: Some(now),
                    started_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::TaskStarted {
                        id: self.id.clone(),
                        session_id,
                    }),
                ];
                (task, effects)
            }

            // Running: heartbeat refreshes timer
            (TaskState::Running, TaskEvent::Heartbeat { timestamp }) => {
                let task = Task {
                    last_heartbeat: Some(timestamp),
                    ..self.clone()
                };
                (task, vec![])
            }

            // Running: tick evaluates if stuck
            (TaskState::Running, TaskEvent::Tick) => {
                if let Some(last) = self.last_heartbeat {
                    if now.duration_since(last) > self.stuck_threshold {
                        let task = Task {
                            state: TaskState::Stuck {
                                since: now,
                                nudge_count: 0,
                            },
                            ..self.clone()
                        };
                        let effects = vec![
                            Effect::Emit(Event::TaskStuck {
                                id: self.id.clone(),
                                since: now,
                            }),
                        ];
                        return (task, effects);
                    }
                }
                (self.clone(), vec![])
            }

            // Running/Stuck → Done
            (TaskState::Running | TaskState::Stuck { .. }, TaskEvent::Complete { output }) => {
                let task = Task {
                    state: TaskState::Done { output: output.clone() },
                    completed_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::TaskComplete {
                        id: self.id.clone(),
                        output,
                    }),
                ];
                (task, effects)
            }

            // Running/Stuck → Failed
            (TaskState::Running | TaskState::Stuck { .. }, TaskEvent::Fail { reason }) => {
                let task = Task {
                    state: TaskState::Failed { reason: reason.clone() },
                    completed_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::TaskFailed {
                        id: self.id.clone(),
                        reason,
                    }),
                ];
                (task, effects)
            }

            // Stuck: nudge increments counter
            (TaskState::Stuck { since, nudge_count }, TaskEvent::Nudged) => {
                let task = Task {
                    state: TaskState::Stuck {
                        since: *since,
                        nudge_count: nudge_count + 1,
                    },
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::TaskNudged {
                        id: self.id.clone(),
                        count: nudge_count + 1,
                    }),
                ];
                (task, effects)
            }

            // Stuck: restart with new session
            (TaskState::Stuck { .. }, TaskEvent::Restart { session_id }) => {
                let task = Task {
                    state: TaskState::Running,
                    session_id: Some(session_id.clone()),
                    last_heartbeat: Some(now),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::TaskRestarted {
                        id: self.id.clone(),
                        session_id,
                    }),
                ];
                (task, effects)
            }

            // Invalid transitions - no change
            _ => (self.clone(), vec![]),
        }
    }

    /// Check if task should be considered stuck
    pub fn is_stuck(&self) -> bool {
        matches!(self.state, TaskState::Stuck { .. })
    }

    /// Check if task is terminal (done or failed)
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, TaskState::Done { .. } | TaskState::Failed { .. })
    }
}
```

**Verification**:
- `cargo test task` passes all unit tests
- All state transitions covered
- Invalid transitions return unchanged state
- Effects emitted correctly for each transition

---

### Phase 2: Queue with Visibility Timeout

**Goal**: Enhance Queue to support visibility timeout for claimed items.

**Deliverables**:
1. Add `claimed_at` and `visible_after` to processing items
2. `claim()` operation that sets visibility timeout
3. `release()` operation for explicit return
4. `reclaim_expired()` for automatic timeout handling
5. Property-based tests for queue invariants

**Key Code**:

```rust
// core/src/queue.rs (enhancements)

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub data: HashMap<String, String>,
    pub priority: i32,
    pub created_at: Instant,
    pub attempts: u32,
    pub max_attempts: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimedItem {
    pub item: QueueItem,
    pub claimed_at: Instant,
    pub visible_after: Instant,
    pub claim_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Queue {
    pub name: String,
    pub items: Vec<QueueItem>,
    pub claimed: Vec<ClaimedItem>,
    pub dead_letters: Vec<DeadLetter>,
    pub default_visibility_timeout: Duration,
}

#[derive(Clone, Debug)]
pub enum QueueEvent {
    Push { item: QueueItem },
    Claim { claim_id: String, visibility_timeout: Option<Duration> },
    Complete { claim_id: String },
    Fail { claim_id: String, reason: String },
    Release { claim_id: String },
    Tick, // Check for expired claims
}

impl Queue {
    /// Pure transition function
    pub fn transition(
        &self,
        event: QueueEvent,
        clock: &impl Clock,
    ) -> (Queue, Vec<Effect>) {
        let now = clock.now();

        match event {
            QueueEvent::Push { item } => {
                let mut items = self.items.clone();
                items.push(item.clone());
                items.sort_by(|a, b| {
                    b.priority.cmp(&a.priority)
                        .then(a.created_at.cmp(&b.created_at))
                });
                let queue = Queue { items, ..self.clone() };
                let effects = vec![
                    Effect::Emit(Event::QueueItemAdded {
                        queue: self.name.clone(),
                        item_id: item.id.clone(),
                    }),
                ];
                (queue, effects)
            }

            QueueEvent::Claim { claim_id, visibility_timeout } => {
                if self.items.is_empty() {
                    return (self.clone(), vec![]);
                }

                let mut items = self.items.clone();
                let item = items.remove(0);
                let timeout = visibility_timeout.unwrap_or(self.default_visibility_timeout);

                let claimed_item = ClaimedItem {
                    item: item.clone(),
                    claimed_at: now,
                    visible_after: now + timeout,
                    claim_id: claim_id.clone(),
                };

                let mut claimed = self.claimed.clone();
                claimed.push(claimed_item);

                let queue = Queue { items, claimed, ..self.clone() };
                let effects = vec![
                    Effect::Emit(Event::QueueItemClaimed {
                        queue: self.name.clone(),
                        item_id: item.id.clone(),
                        claim_id,
                    }),
                ];
                (queue, effects)
            }

            QueueEvent::Complete { claim_id } => {
                let (completed, remaining): (Vec<_>, Vec<_>) = self.claimed.iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if completed.is_empty() {
                    return (self.clone(), vec![]);
                }

                let item_id = completed[0].item.id.clone();
                let queue = Queue {
                    claimed: remaining,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::QueueItemComplete {
                        queue: self.name.clone(),
                        item_id,
                    }),
                ];
                (queue, effects)
            }

            QueueEvent::Fail { claim_id, reason } => {
                let (failed, remaining): (Vec<_>, Vec<_>) = self.claimed.iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if failed.is_empty() {
                    return (self.clone(), vec![]);
                }

                let mut item = failed[0].item.clone();
                item.attempts += 1;

                // Requeue or dead-letter based on attempts
                let (items, dead_letters, effects) = if item.attempts >= item.max_attempts {
                    let mut dead = self.dead_letters.clone();
                    dead.push(DeadLetter {
                        item: item.clone(),
                        reason: reason.clone(),
                        failed_at: now,
                    });
                    (
                        self.items.clone(),
                        dead,
                        vec![Effect::Emit(Event::QueueItemDeadLettered {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason,
                        })],
                    )
                } else {
                    let mut items = self.items.clone();
                    items.push(item.clone());
                    items.sort_by(|a, b| {
                        b.priority.cmp(&a.priority)
                            .then(a.created_at.cmp(&b.created_at))
                    });
                    (
                        items,
                        self.dead_letters.clone(),
                        vec![Effect::Emit(Event::QueueItemFailed {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason,
                        })],
                    )
                };

                let queue = Queue {
                    items,
                    claimed: remaining,
                    dead_letters,
                    ..self.clone()
                };
                (queue, effects)
            }

            QueueEvent::Release { claim_id } => {
                let (released, remaining): (Vec<_>, Vec<_>) = self.claimed.iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if released.is_empty() {
                    return (self.clone(), vec![]);
                }

                let mut items = self.items.clone();
                items.push(released[0].item.clone());
                items.sort_by(|a, b| {
                    b.priority.cmp(&a.priority)
                        .then(a.created_at.cmp(&b.created_at))
                });

                let queue = Queue {
                    items,
                    claimed: remaining,
                    ..self.clone()
                };
                (queue, vec![])
            }

            QueueEvent::Tick => {
                // Find expired claims
                let (expired, active): (Vec<_>, Vec<_>) = self.claimed.iter()
                    .cloned()
                    .partition(|c| now >= c.visible_after);

                if expired.is_empty() {
                    return (self.clone(), vec![]);
                }

                // Return expired items to queue with incremented attempts
                let mut items = self.items.clone();
                let mut effects = vec![];

                for claim in &expired {
                    let mut item = claim.item.clone();
                    item.attempts += 1;

                    if item.attempts >= item.max_attempts {
                        let mut dead = self.dead_letters.clone();
                        dead.push(DeadLetter {
                            item: item.clone(),
                            reason: "visibility timeout exceeded max attempts".into(),
                            failed_at: now,
                        });
                    } else {
                        items.push(item.clone());
                        effects.push(Effect::Emit(Event::QueueItemReleased {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason: "visibility timeout".into(),
                        }));
                    }
                }

                items.sort_by(|a, b| {
                    b.priority.cmp(&a.priority)
                        .then(a.created_at.cmp(&b.created_at))
                });

                let queue = Queue {
                    items,
                    claimed: active,
                    ..self.clone()
                };
                (queue, effects)
            }
        }
    }

    /// Get count of available items (not claimed)
    pub fn available_count(&self) -> usize {
        self.items.len()
    }

    /// Get count of claimed items
    pub fn claimed_count(&self) -> usize {
        self.claimed.len()
    }
}
```

**Verification**:
- Unit tests for all queue operations
- Property test: `total_items == available + claimed + dead_letters`
- Property test: items always sorted by priority
- Visibility timeout correctly expires claims

---

### Phase 3: Pipeline Checkpoints & Recovery

**Goal**: Add checkpoint support to Pipeline for recovery after failures.

**Deliverables**:
1. `Checkpoint` struct capturing pipeline state
2. `checkpoint()` method on Pipeline
3. `restore()` class method to rebuild from checkpoint
4. Recovery logic for interrupted pipelines
5. Checkpoint effect for persistence

**Key Code**:

```rust
// core/src/pipeline.rs (enhancements)

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub pipeline_id: PipelineId,
    pub phase: Phase,
    pub inputs: HashMap<String, String>,
    pub outputs: HashMap<String, String>,
    pub created_at: Instant,
    pub sequence: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: PipelineId,
    pub kind: PipelineKind,
    pub name: String,
    pub phase: Phase,
    pub inputs: HashMap<String, String>,
    pub outputs: HashMap<String, String>,
    pub workspace_id: Option<WorkspaceId>,
    pub current_task_id: Option<TaskId>,
    pub created_at: Instant,
    pub updated_at: Instant,
    pub checkpoint_sequence: u64,
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    Init,
    Blocked { waiting_on: String, guard_id: Option<String> },
    Running { phase_name: String },
    Done,
    Failed { reason: String, recoverable: bool },
}

#[derive(Clone, Debug)]
pub enum PipelineEvent {
    /// Phase completed successfully
    PhaseComplete { outputs: HashMap<String, String> },
    /// Phase failed
    PhaseFailed { reason: String, recoverable: bool },
    /// Blocking condition cleared
    Unblocked,
    /// Task assigned to pipeline phase
    TaskAssigned { task_id: TaskId },
    /// Task completed
    TaskComplete { task_id: TaskId, output: Option<String> },
    /// Task failed
    TaskFailed { task_id: TaskId, reason: String },
    /// Request checkpoint
    RequestCheckpoint,
    /// Restore from checkpoint
    Restore { checkpoint: Checkpoint },
}

impl Pipeline {
    /// Pure transition function
    pub fn transition(
        &self,
        event: PipelineEvent,
        clock: &impl Clock,
    ) -> (Pipeline, Vec<Effect>) {
        let now = clock.now();

        match (&self.phase, event) {
            // Init → first phase (Running)
            (Phase::Init, PipelineEvent::PhaseComplete { outputs }) => {
                let next_phase = self.next_phase_for_kind();
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Running { phase_name: next_phase.clone() };
                pipeline.outputs.extend(outputs);
                pipeline.updated_at = now;

                let effects = vec![
                    Effect::Emit(Event::PipelinePhase {
                        id: self.id.clone(),
                        phase: next_phase,
                    }),
                ];
                (pipeline, effects)
            }

            // Running → next phase or Done
            (Phase::Running { phase_name }, PipelineEvent::PhaseComplete { outputs }) => {
                let mut pipeline = self.clone();
                pipeline.outputs.extend(outputs);
                pipeline.updated_at = now;
                pipeline.current_task_id = None;

                match self.next_phase_after(phase_name) {
                    Some(next) => {
                        pipeline.phase = Phase::Running { phase_name: next.clone() };
                        let effects = vec![
                            Effect::Emit(Event::PipelinePhase {
                                id: self.id.clone(),
                                phase: next,
                            }),
                        ];
                        (pipeline, effects)
                    }
                    None => {
                        pipeline.phase = Phase::Done;
                        let effects = vec![
                            Effect::Emit(Event::PipelineComplete {
                                id: self.id.clone(),
                            }),
                        ];
                        (pipeline, effects)
                    }
                }
            }

            // Running → Blocked
            (Phase::Running { .. }, PipelineEvent::PhaseFailed { reason, recoverable: true }) => {
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Blocked {
                    waiting_on: reason.clone(),
                    guard_id: None,
                };
                pipeline.updated_at = now;

                let effects = vec![
                    Effect::Emit(Event::PipelineBlocked {
                        id: self.id.clone(),
                        reason,
                    }),
                ];
                (pipeline, effects)
            }

            // Running → Failed (non-recoverable)
            (Phase::Running { .. }, PipelineEvent::PhaseFailed { reason, recoverable: false }) => {
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Failed {
                    reason: reason.clone(),
                    recoverable: false,
                };
                pipeline.updated_at = now;

                let effects = vec![
                    Effect::Emit(Event::PipelineFailed {
                        id: self.id.clone(),
                        reason,
                    }),
                ];
                (pipeline, effects)
            }

            // Blocked → Running (resume)
            (Phase::Blocked { .. }, PipelineEvent::Unblocked) => {
                // Resume from checkpoint or last known phase
                let resumed_phase = self.last_running_phase();
                let mut pipeline = self.clone();
                pipeline.phase = Phase::Running { phase_name: resumed_phase.clone() };
                pipeline.updated_at = now;

                let effects = vec![
                    Effect::Emit(Event::PipelineResumed {
                        id: self.id.clone(),
                        phase: resumed_phase,
                    }),
                ];
                (pipeline, effects)
            }

            // Task assignment
            (Phase::Running { .. }, PipelineEvent::TaskAssigned { task_id }) => {
                let mut pipeline = self.clone();
                pipeline.current_task_id = Some(task_id.clone());
                pipeline.updated_at = now;
                (pipeline, vec![])
            }

            // Checkpoint request
            (_, PipelineEvent::RequestCheckpoint) => {
                let checkpoint = Checkpoint {
                    pipeline_id: self.id.clone(),
                    phase: self.phase.clone(),
                    inputs: self.inputs.clone(),
                    outputs: self.outputs.clone(),
                    created_at: now,
                    sequence: self.checkpoint_sequence + 1,
                };

                let mut pipeline = self.clone();
                pipeline.checkpoint_sequence += 1;
                pipeline.checkpoints.push(checkpoint.clone());
                pipeline.updated_at = now;

                // Keep only last 5 checkpoints
                if pipeline.checkpoints.len() > 5 {
                    pipeline.checkpoints.remove(0);
                }

                let effects = vec![
                    Effect::SaveCheckpoint {
                        pipeline_id: self.id.clone(),
                        checkpoint,
                    },
                ];
                (pipeline, effects)
            }

            // Restore from checkpoint
            (Phase::Failed { recoverable: true, .. }, PipelineEvent::Restore { checkpoint }) => {
                let mut pipeline = self.clone();
                pipeline.phase = checkpoint.phase;
                pipeline.inputs = checkpoint.inputs;
                pipeline.outputs = checkpoint.outputs;
                pipeline.updated_at = now;

                let effects = vec![
                    Effect::Emit(Event::PipelineRestored {
                        id: self.id.clone(),
                        from_sequence: checkpoint.sequence,
                    }),
                ];
                (pipeline, effects)
            }

            // Invalid transitions
            _ => (self.clone(), vec![]),
        }
    }

    /// Get the initial phase for the pipeline kind
    fn next_phase_for_kind(&self) -> String {
        match self.kind {
            PipelineKind::Build => "plan".to_string(),
            PipelineKind::Bugfix => "fix".to_string(),
        }
    }

    /// Get the next phase after the current one
    fn next_phase_after(&self, current: &str) -> Option<String> {
        let phases = match self.kind {
            PipelineKind::Build => vec!["plan", "decompose", "execute", "merge"],
            PipelineKind::Bugfix => vec!["fix", "verify", "merge", "cleanup"],
        };

        phases.iter()
            .position(|&p| p == current)
            .and_then(|i| phases.get(i + 1))
            .map(|s| s.to_string())
    }

    /// Get the last running phase from checkpoints
    fn last_running_phase(&self) -> String {
        self.checkpoints.last()
            .and_then(|cp| match &cp.phase {
                Phase::Running { phase_name } => Some(phase_name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.next_phase_for_kind())
    }

    /// Create a checkpoint of current state
    pub fn checkpoint(&self, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        self.transition(PipelineEvent::RequestCheckpoint, clock)
    }

    /// Restore pipeline from a checkpoint
    pub fn restore_from(checkpoint: Checkpoint, clock: &impl Clock) -> Pipeline {
        Pipeline {
            id: checkpoint.pipeline_id,
            kind: PipelineKind::Build, // Would need to persist this
            name: String::new(),
            phase: checkpoint.phase,
            inputs: checkpoint.inputs,
            outputs: checkpoint.outputs,
            workspace_id: None,
            current_task_id: None,
            created_at: checkpoint.created_at,
            updated_at: clock.now(),
            checkpoint_sequence: checkpoint.sequence,
            checkpoints: vec![checkpoint],
        }
    }
}
```

**Verification**:
- Checkpoint captures complete pipeline state
- Restore rebuilds pipeline correctly
- Checkpoint sequence monotonically increases
- Old checkpoints pruned automatically

---

### Phase 4: Effect Type Expansion

**Goal**: Add new effect variants for Task and enhanced Queue operations.

**Deliverables**:
1. Task-related effects and events
2. Queue visibility timeout effects
3. Checkpoint persistence effects
4. Update existing code to use new effect types

**Key Code**:

```rust
// core/src/effect.rs (additions)

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    // Event emission
    Emit(Event),

    // Session management (existing)
    SpawnSession { name: String, cwd: PathBuf, command: String },
    KillSession { name: String },
    SendToSession { name: String, input: String },

    // Git operations (existing)
    CreateWorktree { branch: String, path: PathBuf },
    RemoveWorktree { path: PathBuf },
    Merge { path: PathBuf, branch: String, strategy: MergeStrategy },

    // State persistence (existing)
    SaveState { kind: String, id: String },

    // NEW: Checkpoint persistence
    SaveCheckpoint { pipeline_id: PipelineId, checkpoint: Checkpoint },

    // NEW: Task operations
    ScheduleTask { task: Task, delay: Option<Duration> },
    CancelTask { task_id: TaskId },

    // NEW: Timer operations
    SetTimer { id: String, duration: Duration },
    CancelTimer { id: String },

    // Logging (existing)
    Log { level: LogLevel, message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    // Workspace events (existing)
    WorkspaceCreated { id: String, name: String },
    WorkspaceReady { id: String },
    WorkspaceDeleted { id: String },

    // Session events (existing)
    SessionStarted { id: String, workspace_id: String },
    SessionActive { id: String },
    SessionIdle { id: String },
    SessionDead { id: String, reason: String },

    // Pipeline events (existing + new)
    PipelineCreated { id: String, kind: String },
    PipelinePhase { id: String, phase: String },
    PipelineComplete { id: String },
    PipelineFailed { id: String, reason: String },
    PipelineBlocked { id: String, reason: String },     // NEW
    PipelineResumed { id: String, phase: String },       // NEW
    PipelineRestored { id: String, from_sequence: u64 }, // NEW

    // Queue events (existing + new)
    QueueItemAdded { queue: String, item_id: String },
    QueueItemClaimed { queue: String, item_id: String, claim_id: String }, // NEW
    QueueItemComplete { queue: String, item_id: String },
    QueueItemFailed { queue: String, item_id: String, reason: String },
    QueueItemReleased { queue: String, item_id: String, reason: String },  // NEW
    QueueItemDeadLettered { queue: String, item_id: String, reason: String }, // NEW

    // NEW: Task events
    TaskStarted { id: TaskId, session_id: SessionId },
    TaskComplete { id: TaskId, output: Option<String> },
    TaskFailed { id: TaskId, reason: String },
    TaskStuck { id: TaskId, since: Instant },
    TaskNudged { id: TaskId, count: u32 },
    TaskRestarted { id: TaskId, session_id: SessionId },

    // NEW: Timer events
    TimerFired { id: String },
}
```

**Verification**:
- All new effect variants compile
- Serialization round-trips correctly
- Events match expected patterns

---

### Phase 5: Property-Based Tests

**Goal**: Add proptest infrastructure and property-based tests for state machine invariants.

**Deliverables**:
1. Proptest module with generators
2. State machine invariant definitions
3. Property tests for Task, Queue, Pipeline
4. Shrinking support for failure cases

**Key Code**:

```rust
// core/src/proptest/mod.rs
pub mod generators;
pub mod invariants;

// core/src/proptest/generators.rs
use proptest::prelude::*;
use crate::{Task, TaskState, TaskEvent, Queue, QueueItem, Pipeline, Phase};

/// Generate arbitrary TaskState
pub fn arb_task_state() -> impl Strategy<Value = TaskState> {
    prop_oneof![
        Just(TaskState::Pending),
        Just(TaskState::Running),
        (any::<u64>(), any::<u32>()).prop_map(|(since_nanos, nudge_count)| {
            TaskState::Stuck {
                since: Instant::now(), // Will be replaced in tests
                nudge_count,
            }
        }),
        any::<Option<String>>().prop_map(|output| TaskState::Done { output }),
        any::<String>().prop_map(|reason| TaskState::Failed { reason }),
    ]
}

/// Generate arbitrary TaskEvent
pub fn arb_task_event() -> impl Strategy<Value = TaskEvent> {
    prop_oneof![
        any::<String>().prop_map(|id| TaskEvent::Start {
            session_id: SessionId(id)
        }),
        Just(TaskEvent::Tick),
        any::<Option<String>>().prop_map(|output| TaskEvent::Complete { output }),
        any::<String>().prop_map(|reason| TaskEvent::Fail { reason }),
        Just(TaskEvent::Nudged),
    ]
}

/// Generate arbitrary QueueItem
pub fn arb_queue_item() -> impl Strategy<Value = QueueItem> {
    (
        "[a-z]{8}",           // id
        any::<i32>(),         // priority
        0u32..5u32,           // attempts
        1u32..10u32,          // max_attempts
    ).prop_map(|(id, priority, attempts, max_attempts)| {
        QueueItem {
            id,
            data: HashMap::new(),
            priority,
            created_at: Instant::now(),
            attempts,
            max_attempts: max_attempts.max(attempts + 1),
        }
    })
}

/// Generate a sequence of queue events
pub fn arb_queue_events(max_len: usize) -> impl Strategy<Value = Vec<QueueEvent>> {
    prop::collection::vec(
        prop_oneof![
            arb_queue_item().prop_map(|item| QueueEvent::Push { item }),
            "[a-z]{8}".prop_map(|id| QueueEvent::Claim {
                claim_id: id,
                visibility_timeout: None
            }),
            "[a-z]{8}".prop_map(|id| QueueEvent::Complete { claim_id: id }),
            ("[a-z]{8}", any::<String>()).prop_map(|(id, reason)| {
                QueueEvent::Fail { claim_id: id, reason }
            }),
            Just(QueueEvent::Tick),
        ],
        0..max_len
    )
}

// core/src/proptest/invariants.rs
use crate::{Queue, Task, Pipeline};

/// Queue invariants that must always hold
pub mod queue {
    use super::*;

    /// Total items = available + claimed + dead_letters
    pub fn items_conserved(queue: &Queue, initial_count: usize, pushes: usize, completes: usize) -> bool {
        let total = queue.items.len() + queue.claimed.len() + queue.dead_letters.len();
        // Items added by push, removed by complete
        total == initial_count + pushes - completes
    }

    /// Items are always sorted by priority (desc) then created_at (asc)
    pub fn items_sorted(queue: &Queue) -> bool {
        queue.items.windows(2).all(|w| {
            w[0].priority > w[1].priority ||
            (w[0].priority == w[1].priority && w[0].created_at <= w[1].created_at)
        })
    }

    /// No duplicate claim IDs in claimed list
    pub fn no_duplicate_claims(queue: &Queue) -> bool {
        let ids: Vec<_> = queue.claimed.iter().map(|c| &c.claim_id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        ids.len() == unique.len()
    }
}

/// Task invariants
pub mod task {
    use super::*;

    /// Terminal states are truly terminal
    pub fn terminal_is_final(task: &Task, event: &TaskEvent) -> bool {
        if task.is_terminal() {
            // Transition should return same state (no change)
            let clock = FakeClock::new();
            let (new_task, _) = task.transition(event.clone(), &clock);
            new_task.state == task.state
        } else {
            true
        }
    }

    /// Nudge count only increases
    pub fn nudge_count_monotonic(before: &Task, after: &Task) -> bool {
        match (&before.state, &after.state) {
            (TaskState::Stuck { nudge_count: a, .. }, TaskState::Stuck { nudge_count: b, .. }) => {
                b >= a
            }
            _ => true
        }
    }
}

/// Pipeline invariants
pub mod pipeline {
    use super::*;

    /// Checkpoint sequence monotonically increases
    pub fn checkpoint_sequence_monotonic(before: &Pipeline, after: &Pipeline) -> bool {
        after.checkpoint_sequence >= before.checkpoint_sequence
    }

    /// Done state is terminal
    pub fn done_is_terminal(pipeline: &Pipeline, event: &PipelineEvent) -> bool {
        if matches!(pipeline.phase, Phase::Done) {
            let clock = FakeClock::new();
            let (new_pipeline, _) = pipeline.transition(event.clone(), &clock);
            matches!(new_pipeline.phase, Phase::Done)
        } else {
            true
        }
    }
}
```

```rust
// core/src/task_tests.rs (property tests)
use proptest::prelude::*;
use crate::proptest::{generators::*, invariants::task::*};

proptest! {
    #[test]
    fn task_terminal_states_are_final(
        task in arb_task().prop_filter("must be terminal", |t| t.is_terminal()),
        event in arb_task_event()
    ) {
        prop_assert!(terminal_is_final(&task, &event));
    }

    #[test]
    fn task_nudge_count_never_decreases(
        initial_state in arb_task_state(),
        events in prop::collection::vec(arb_task_event(), 0..20)
    ) {
        let clock = FakeClock::new();
        let mut task = Task::new(/* ... */);

        let mut prev_nudge_count = 0u32;
        for event in events {
            let (new_task, _) = task.transition(event, &clock);
            if let TaskState::Stuck { nudge_count, .. } = &new_task.state {
                prop_assert!(*nudge_count >= prev_nudge_count);
                prev_nudge_count = *nudge_count;
            }
            task = new_task;
        }
    }
}

// core/src/queue_tests.rs (property tests)
proptest! {
    #[test]
    fn queue_items_always_sorted(
        events in arb_queue_events(50)
    ) {
        let clock = FakeClock::new();
        let mut queue = Queue::new("test");

        for event in events {
            let (new_queue, _) = queue.transition(event, &clock);
            prop_assert!(invariants::queue::items_sorted(&new_queue));
            queue = new_queue;
        }
    }

    #[test]
    fn queue_no_duplicate_claims(
        events in arb_queue_events(50)
    ) {
        let clock = FakeClock::new();
        let mut queue = Queue::new("test");

        for event in events {
            let (new_queue, _) = queue.transition(event, &clock);
            prop_assert!(invariants::queue::no_duplicate_claims(&new_queue));
            queue = new_queue;
        }
    }
}
```

**Verification**:
- `cargo test --lib proptest` finds no invariant violations
- Shrinking produces minimal failing cases
- All state machines have at least 3 property tests

---

### Phase 6: Parametrized Tests with Yare

**Goal**: Add parametrized tests for edge cases using yare.

**Deliverables**:
1. Parametrized tests for Task transitions
2. Parametrized tests for Queue operations
3. Parametrized tests for Pipeline phase logic
4. Edge case coverage

**Key Code**:

```rust
// core/src/task_tests.rs (yare tests)
use yare::parameterized;

#[parameterized(
    pending_to_running = { TaskState::Pending, TaskEvent::Start { session_id: SessionId("s1".into()) }, TaskState::Running },
    running_to_done = { TaskState::Running, TaskEvent::Complete { output: None }, TaskState::Done { output: None } },
    running_to_failed = { TaskState::Running, TaskEvent::Fail { reason: "error".into() }, TaskState::Failed { reason: "error".into() } },
    stuck_to_done = { TaskState::Stuck { since: Instant::now(), nudge_count: 2 }, TaskEvent::Complete { output: Some("ok".into()) }, TaskState::Done { output: Some("ok".into()) } },
    stuck_nudged = { TaskState::Stuck { since: Instant::now(), nudge_count: 0 }, TaskEvent::Nudged, TaskState::Stuck { since: Instant::now(), nudge_count: 1 } },
)]
fn task_valid_transitions(initial: TaskState, event: TaskEvent, expected: TaskState) {
    let clock = FakeClock::new();
    let task = Task {
        state: initial,
        ..Task::default_for_test()
    };
    let (new_task, _) = task.transition(event, &clock);

    // Compare variant (not exact instant values)
    assert!(std::mem::discriminant(&new_task.state) == std::mem::discriminant(&expected));
}

#[parameterized(
    pending_cannot_complete = { TaskState::Pending, TaskEvent::Complete { output: None } },
    pending_cannot_fail = { TaskState::Pending, TaskEvent::Fail { reason: "x".into() } },
    done_cannot_start = { TaskState::Done { output: None }, TaskEvent::Start { session_id: SessionId("s1".into()) } },
    failed_cannot_restart = { TaskState::Failed { reason: "x".into() }, TaskEvent::Restart { session_id: SessionId("s1".into()) } },
)]
fn task_invalid_transitions_are_no_op(initial: TaskState, event: TaskEvent) {
    let clock = FakeClock::new();
    let task = Task {
        state: initial.clone(),
        ..Task::default_for_test()
    };
    let (new_task, effects) = task.transition(event, &clock);

    assert!(std::mem::discriminant(&new_task.state) == std::mem::discriminant(&initial));
    assert!(effects.is_empty());
}

// core/src/queue_tests.rs (yare tests)
#[parameterized(
    empty_claim_returns_none = { vec![], 0 },
    single_item_claims = { vec![item(1)], 1 },
    highest_priority_first = { vec![item(1), item(5), item(3)], 1 }, // claims priority 5
)]
fn queue_claim_order(items: Vec<QueueItem>, expected_claims: usize) {
    let clock = FakeClock::new();
    let mut queue = Queue::new("test");

    for item in items {
        let (q, _) = queue.transition(QueueEvent::Push { item }, &clock);
        queue = q;
    }

    let (queue, _) = queue.transition(
        QueueEvent::Claim { claim_id: "c1".into(), visibility_timeout: None },
        &clock
    );

    assert_eq!(queue.claimed.len(), expected_claims);
}

// core/src/pipeline_tests.rs (yare tests)
#[parameterized(
    build_plan_to_decompose = { PipelineKind::Build, "plan", Some("decompose") },
    build_decompose_to_execute = { PipelineKind::Build, "decompose", Some("execute") },
    build_execute_to_merge = { PipelineKind::Build, "execute", Some("merge") },
    build_merge_to_done = { PipelineKind::Build, "merge", None },
    bugfix_fix_to_verify = { PipelineKind::Bugfix, "fix", Some("verify") },
    bugfix_verify_to_merge = { PipelineKind::Bugfix, "verify", Some("merge") },
    bugfix_cleanup_to_done = { PipelineKind::Bugfix, "cleanup", None },
)]
fn pipeline_phase_progression(kind: PipelineKind, current: &str, expected_next: Option<&str>) {
    let pipeline = Pipeline {
        kind,
        phase: Phase::Running { phase_name: current.into() },
        ..Pipeline::default_for_test()
    };

    let next = pipeline.next_phase_after(current);
    assert_eq!(next.as_deref(), expected_next);
}
```

**Verification**:
- All parametrized tests pass
- Edge cases explicitly documented in test names
- Invalid transitions verified as no-ops

## Key Implementation Details

### Pure Functional Pattern

All state machines follow the same pattern:

```rust
impl StateMachine {
    /// Pure transition: (CurrentState, Event) → (NewState, Effects)
    pub fn transition(&self, event: Event, clock: &impl Clock) -> (Self, Vec<Effect>) {
        // Match on current state and event
        // Return new state and any effects to execute
        // Never perform I/O directly
    }
}
```

This enables:
- 100% unit testability without mocks
- Deterministic replay from event logs
- Property-based testing of invariants

### Visibility Timeout Design

Queue claims have automatic expiration:

```
Push(item) → Queue { items: [item], claimed: [] }
Claim(c1)  → Queue { items: [], claimed: [item@c1] }
...time passes...
Tick       → Queue { items: [item], claimed: [] }  // Expired, returned
```

The visibility timeout ensures work-in-progress isn't lost if a worker crashes.

### Checkpoint Strategy

Pipelines checkpoint at phase boundaries:

```
Init → [checkpoint] → Plan → [checkpoint] → Decompose → ...
```

On recovery:
1. Load latest checkpoint
2. Rebuild pipeline state from checkpoint
3. Resume from last known phase
4. Effects replay handles idempotency

### Effect Execution Separation

The state machines produce effects but never execute them:

```rust
let (new_state, effects) = state.transition(event, &clock);

// State machines stop here - effects go to executor
for effect in effects {
    executor.execute(effect).await?;
}
```

This separation is critical for testability and is the foundation for Epic 3 (Engine & Execution Loop).

## Verification Plan

### Unit Tests (Target: 95%+ coverage for state machines)

Run with: `cargo test --lib`

| Module | Key Tests |
|--------|-----------|
| `task` | All state transitions, heartbeat evaluation, stuck detection |
| `queue` | Push ordering, claim/release, visibility timeout, dead letters |
| `pipeline` | Phase progression, checkpoints, restore, blocking |
| `effect` | Serialization round-trip, event type coverage |

### Property-Based Tests

Run with: `cargo test proptest`

| Property | Description |
|----------|-------------|
| `task_terminal_is_final` | Terminal states cannot transition |
| `task_nudge_monotonic` | Nudge count never decreases |
| `queue_items_sorted` | Priority ordering always maintained |
| `queue_no_duplicate_claims` | Claim IDs are unique |
| `queue_items_conserved` | Items not created or destroyed incorrectly |
| `pipeline_checkpoint_monotonic` | Sequence numbers increase |
| `pipeline_done_is_terminal` | Done state is final |

### Parametrized Tests

Run with: `cargo test` (yare tests run with regular tests)

| Test Group | Coverage |
|------------|----------|
| Task valid transitions | 5+ cases |
| Task invalid transitions | 4+ cases |
| Queue claim ordering | 3+ cases |
| Pipeline phase progression | 7+ cases |

### Integration with Existing Tests

Ensure Epic 1 tests still pass:
- `cargo test --lib` (all existing tests)
- Adapter contract tests still work
- CLI integration tests unaffected

### Manual Verification Checklist

- [ ] `Task` state machine handles all transitions correctly
- [ ] `Queue` visibility timeout expires claims properly
- [ ] `Pipeline` checkpoint/restore works across phases
- [ ] Property tests run without finding counterexamples
- [ ] All parametrized edge cases pass
- [ ] No regressions in Epic 1 functionality

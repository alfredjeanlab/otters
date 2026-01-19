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

#[derive(Debug, Error)]
pub enum EngineError {
    Storage(#[from] StorageError),
    Adapter(String),
    PipelineNotFound(PipelineId),
    TaskNotFound(TaskId),
    WorkspaceNotFound(WorkspaceId),
}

#[derive(Debug)]
pub enum EffectResult {
    Ok,
    Failed { event: Event },
    Retry { after: Duration },
}

pub struct Engine<A: Adapters, C: Clock> {
    adapters: A,
    store: JsonStore,
    clock: C,
    // In-memory caches: pipelines, tasks, workspaces, sessions
}

impl<A: Adapters, C: Clock> Engine<A, C> {
    pub fn new(adapters: A, store: JsonStore, clock: C) -> Self;
    pub fn load(&mut self) -> Result<(), EngineError>;  // Load pipelines/tasks/etc from store

    pub async fn process_pipeline_event(&mut self, pipeline_id: &PipelineId, event: PipelineEvent) -> Result<(), EngineError> {
        // 1. Transition pipeline state
        // 2. Persist state first (crash safety)
        // 3. Execute effects, collect failures as feedback events
        // 4. Process feedback events recursively
    }

    async fn execute_effect(&self, effect: Effect) -> EffectResult {
        // Match each Effect variant, call appropriate adapter
        // SpawnSession → sessions().spawn() → Failed{SessionDead} on error
        // KillSession/RemoveWorktree → best-effort, always Ok
        // SendToSession/CreateWorktree → Failed{event} on error
        // Merge → Ok on success, Failed{PipelineFailed} on conflict
        // SaveState/SaveCheckpoint/ScheduleTask/Timers → log and Ok
    }

    async fn process_event(&mut self, event: Event) -> Result<(), EngineError> {
        // TaskStuck → handle_stuck_task (recovery chain)
        // SessionDead → find task by session, fail it
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
    pub async fn signal_done(&mut self, workspace_id: &WorkspaceId, error: Option<String>) -> Result<()> {
        // Find pipeline by workspace, complete or fail its current task
    }

    pub async fn signal_checkpoint(&mut self, workspace_id: &WorkspaceId) -> Result<()> {
        // Send RequestCheckpoint event to pipeline
    }

    pub async fn process_heartbeat(&mut self, session_id: &SessionId) -> Result<()> {
        // Update session.last_output, send Heartbeat to associated task
    }

    pub async fn poll_sessions(&mut self) -> Result<()> {
        // For each session: check is_alive (→ SessionDead if not)
        // Capture pane, hash output, if changed → process_heartbeat
    }

    pub async fn process_task_event(&mut self, task_id: &TaskId, event: TaskEvent) -> Result<()> {
        // Transition task, persist, execute effects
        // If terminal → cascade TaskComplete/TaskFailed to pipeline
    }
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

pub struct ScheduledItem {
    pub id: String,
    pub fire_at: Instant,
    pub kind: ScheduledKind,
    pub repeat: Option<Duration>,
}

pub enum ScheduledKind {
    TaskTick,                        // Evaluate all tasks for stuck detection
    QueueTick { queue_name: String }, // Evaluate queue for visibility timeout
    Timer { id: String },            // Custom timer
    HeartbeatPoll,                   // Heartbeat check for sessions
}

pub struct Scheduler {
    items: BinaryHeap<ScheduledItem>,  // Min-heap by fire_at
    cancelled: HashSet<String>,
}

impl Scheduler {
    pub fn schedule(&mut self, id: impl Into<String>, fire_at: Instant, kind: ScheduledKind);
    pub fn schedule_repeating(&mut self, id: impl Into<String>, fire_at: Instant, interval: Duration, kind: ScheduledKind);
    pub fn cancel(&mut self, id: &str);
    pub fn poll(&mut self, now: Instant) -> Vec<ScheduledItem>;  // Pop ready items, reschedule repeating
    pub fn init_defaults(&mut self, clock: &impl Clock);  // TaskTick@30s, QueueTick@10s, HeartbeatPoll@5s
}

// Engine integration
impl<A: Adapters, C: Clock> Engine<A, C> {
    pub async fn process_scheduled(&mut self, scheduler: &mut Scheduler) -> Result<()> {
        // Poll scheduler, dispatch: TaskTick→tick_all_tasks, QueueTick→tick_queue, etc.
    }
    async fn tick_all_tasks(&mut self) -> Result<()>;  // Send Tick to running/stuck tasks
    fn tick_queue(&mut self, queue_name: &str) -> Result<()>;  // Send Tick to queue
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

pub struct RecoveryConfig {
    pub max_nudges: u32,           // Default: 3
    pub nudge_cooldown: Duration,  // Default: 60s
    pub max_restarts: u32,         // Default: 2
    pub restart_cooldown: Duration, // Default: 300s
    pub nudge_message: String,
}

pub struct RecoveryState {
    pub nudge_count: u32,
    pub restart_count: u32,
    pub last_nudge: Option<Instant>,
    pub last_restart: Option<Instant>,
    pub escalated: bool,
}

pub enum RecoveryAction { Nudge, Restart, Escalate, Wait { until: Instant }, None }

impl RecoveryState {
    pub fn next_action(&self, task: &Task, config: &RecoveryConfig, now: Instant) -> RecoveryAction {
        // Only act on stuck tasks, check escalated flag
        // If nudge_count < max_nudges && cooldown passed → Nudge
        // If restart_count < max_restarts && cooldown passed → Restart
        // Otherwise → Escalate
    }
    pub fn record_nudge(&mut self, now: Instant);   // Increment, update last_nudge
    pub fn record_restart(&mut self, now: Instant); // Increment, reset nudge state
    pub fn record_escalation(&mut self);
}

// Engine integration
impl<A: Adapters, C: Clock> Engine<A, C> {
    pub async fn handle_stuck_task(&mut self, task_id: &TaskId) -> Result<()> {
        // Get recovery state, call next_action
        // Nudge: send message to session, record, emit Nudged
        // Restart: kill session, spawn new, record, emit Restart
        // Escalate: record, emit TaskFailed
    }
    async fn spawn_task_session(&self, task: &Task) -> Result<SessionId>;
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

pub struct FakeConfig {
    pub spawn_responses: VecDeque<Result<SessionId, SessionError>>,
    pub send_fails: bool,
    pub merge_conflicts: bool,
    pub dead_sessions: HashSet<String>,
}

impl FakeAdapters {
    pub fn queue_spawn_response(&self, result: Result<SessionId, SessionError>);
    pub fn set_send_fails(&self, fails: bool);
    pub fn set_merge_conflicts(&self, conflicts: bool);
    pub fn mark_session_dead(&self, session_id: &str);
}

// core/tests/contract_tests.rs
// Generic contract tests run against both fake and real adapters:
// - spawn_then_kill_contract: spawn → is_alive → kill → !is_alive
// - send_to_nonexistent_fails: send to fake id → error
// - capture_returns_content: spawn echo → capture → non-empty
// - worktree_lifecycle: add → list contains → remove
// Real adapter tests marked #[ignore], run with --ignored
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
// All tests use FakeAdapters + FakeClock + JsonStore::open_temp()

// full_build_pipeline_lifecycle: create pipeline → signal_done 4x → assert complete
// stuck_task_recovery_chain: create pipeline → advance time → assert stuck → verify nudge sent → advance more → verify restart
// signal_done_advances_pipeline: start task → signal_done → assert phase changed
// effect_failure_triggers_recovery: set_send_fails(true) → stuck task → handle_stuck_task → engine still functional
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

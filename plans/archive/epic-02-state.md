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
    Pending,
    Running,
    Stuck { since: Instant, nudge_count: u32 },
    Done { output: Option<String> },
    Failed { reason: String },
}

#[derive(Clone, Debug)]
pub enum TaskEvent {
    Start { session_id: SessionId },
    Heartbeat { timestamp: Instant },
    Complete { output: Option<String> },
    Fail { reason: String },
    Nudged,
    Restart { session_id: SessionId },
    Tick,
}

impl Task {
    pub fn transition(&self, event: TaskEvent, clock: &impl Clock) -> (Task, Vec<Effect>) {
        // Pending + Start → Running, emit TaskStarted
        // Running + Heartbeat → update last_heartbeat
        // Running + Tick → check stuck_threshold, maybe → Stuck
        // Running/Stuck + Complete → Done, emit TaskComplete
        // Running/Stuck + Fail → Failed, emit TaskFailed
        // Stuck + Nudged → increment nudge_count, emit TaskNudged
        // Stuck + Restart → Running with new session, emit TaskRestarted
        // Invalid transitions → no change
    }

    pub fn is_stuck(&self) -> bool { matches!(self.state, TaskState::Stuck { .. }) }
    pub fn is_terminal(&self) -> bool { matches!(self.state, TaskState::Done { .. } | TaskState::Failed { .. }) }
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
    Tick,
}

impl Queue {
    pub fn transition(&self, event: QueueEvent, clock: &impl Clock) -> (Queue, Vec<Effect>) {
        // Push: add item, sort by priority desc + created_at asc, emit QueueItemAdded
        // Claim: move first item to claimed with visibility timeout, emit QueueItemClaimed
        // Complete: remove from claimed, emit QueueItemComplete
        // Fail: increment attempts, requeue or dead-letter based on max_attempts
        // Release: return claimed item to queue
        // Tick: find expired claims, requeue or dead-letter
    }

    pub fn available_count(&self) -> usize { self.items.len() }
    pub fn claimed_count(&self) -> usize { self.claimed.len() }
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
    PhaseComplete { outputs: HashMap<String, String> },
    PhaseFailed { reason: String, recoverable: bool },
    Unblocked,
    TaskAssigned { task_id: TaskId },
    TaskComplete { task_id: TaskId, output: Option<String> },
    TaskFailed { task_id: TaskId, reason: String },
    RequestCheckpoint,
    Restore { checkpoint: Checkpoint },
}

impl Pipeline {
    pub fn transition(&self, event: PipelineEvent, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        // Init + PhaseComplete → Running{first_phase}
        // Running + PhaseComplete → next phase or Done
        // Running + PhaseFailed{recoverable:true} → Blocked
        // Running + PhaseFailed{recoverable:false} → Failed
        // Blocked + Unblocked → Running{last_phase}
        // Running + TaskAssigned → set current_task_id
        // RequestCheckpoint → save checkpoint, keep last 5
        // Failed{recoverable} + Restore → restore from checkpoint
    }

    fn next_phase_for_kind(&self) -> String;  // Build→"plan", Bugfix→"fix"
    fn next_phase_after(&self, current: &str) -> Option<String>;  // Phase sequence by kind
    fn last_running_phase(&self) -> String;  // From checkpoints
    pub fn checkpoint(&self, clock: &impl Clock) -> (Pipeline, Vec<Effect>);
    pub fn restore_from(checkpoint: Checkpoint, clock: &impl Clock) -> Pipeline;
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
// core/src/effect.rs (summary of variants)

pub enum Effect {
    Emit(Event),
    // Session: SpawnSession, KillSession, SendToSession
    // Git: CreateWorktree, RemoveWorktree, Merge
    // State: SaveState, SaveCheckpoint
    // Task: ScheduleTask, CancelTask
    // Timer: SetTimer, CancelTimer
    // Log { level, message }
}

pub enum Event {
    // Workspace: Created, Ready, Deleted
    // Session: Started, Active, Idle, Dead
    // Pipeline: Created, Phase, Complete, Failed, Blocked, Resumed, Restored
    // Queue: ItemAdded, ItemClaimed, ItemComplete, ItemFailed, ItemReleased, ItemDeadLettered
    // Task: Started, Complete, Failed, Stuck, Nudged, Restarted
    // Timer: Fired
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
// core/src/proptest/generators.rs
pub fn arb_task_state() -> impl Strategy<Value = TaskState>;  // All TaskState variants
pub fn arb_task_event() -> impl Strategy<Value = TaskEvent>;  // All TaskEvent variants
pub fn arb_queue_item() -> impl Strategy<Value = QueueItem>;  // Random id, priority, attempts
pub fn arb_queue_events(max_len: usize) -> impl Strategy<Value = Vec<QueueEvent>>;

// core/src/proptest/invariants.rs
pub mod queue {
    pub fn items_conserved(queue: &Queue, ...) -> bool;  // total == available + claimed + dead
    pub fn items_sorted(queue: &Queue) -> bool;          // priority desc, created_at asc
    pub fn no_duplicate_claims(queue: &Queue) -> bool;
}
pub mod task {
    pub fn terminal_is_final(task: &Task, event: &TaskEvent) -> bool;
    pub fn nudge_count_monotonic(before: &Task, after: &Task) -> bool;
}
pub mod pipeline {
    pub fn checkpoint_sequence_monotonic(before: &Pipeline, after: &Pipeline) -> bool;
    pub fn done_is_terminal(pipeline: &Pipeline, event: &PipelineEvent) -> bool;
}
```

Property tests verify: terminal states are final, nudge count never decreases, queue items always sorted, no duplicate claims.

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
// Yare parametrized tests using #[parameterized(...)]

// task_valid_transitions: Pending→Running, Running→Done, Running→Failed, Stuck→Done, Stuck+Nudged
// task_invalid_transitions_are_no_op: Pending can't Complete/Fail, Done can't Start, Failed can't Restart
// queue_claim_order: empty→none, single→claims, multiple→highest priority first
// pipeline_phase_progression: Build plan→decompose→execute→merge→Done, Bugfix fix→verify→merge→cleanup→Done
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

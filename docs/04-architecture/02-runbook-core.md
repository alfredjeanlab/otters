# Runbook Core Primitives

This document details the design of core runbook primitives: Pipeline, Queue, Task, Strategy, and Event.

## Design Principles

All primitives follow these rules:

1. **Immutable state** - State is never mutated, only replaced
2. **Pure transitions** - `(State, Event) → (NewState, Effects)`
3. **Explicit effects** - All side effects returned as data
4. **Injectable time** - Clock passed in, never accessed globally
5. **Serializable** - All state can be persisted/restored

## Pipeline

Pipelines are the central orchestration primitive, managing multi-phase workflows.

### State Machine

```
     ┌─────────────────────────────────────────────────────────┐
     │                                                         │
     ▼                                                         │
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐       │
│  init   │───▶│ blocked │───▶│ phase_n │───▶│  done   │       │
└─────────┘    └─────────┘    └─────────┘    └─────────┘       │
     │              │              │                           │
     │              │              │         ┌─────────┐       │
     └──────────────┴──────────────┴────────▶│ failed  │───────┘
                                             └─────────┘
                                             (can retry)
```

### Data Structure

```rust
pub struct Pipeline {
    id: PipelineId,
    runbook: RunbookId,
    phase: Phase,
    phase_started_at: Instant,
    attempt: u32,
    context: Context,           // Variables passed between phases
    checkpoints: Vec<Checkpoint>,
    created_at: Instant,
}

pub enum Phase {
    Init,
    Blocked { reason: BlockReason },
    Running { name: String },
    Done,
    Failed { error: String, recoverable: bool },
}

pub struct Checkpoint {
    phase: String,
    message: String,
    timestamp: Instant,
}
```

### State Transitions

```
transition(event, clock) → (Pipeline, Vec<Effect>):
    match (phase, event):
        (Init, Start):
            if has_unmet_dependencies → Blocked, emit pipeline:blocked
            else → Running(first_phase), emit pipeline:started, pipeline:phase

        (Running, PhaseComplete):
            if has_next_phase → Running(next), emit pipeline:phase
            else → Done, emit pipeline:complete

        (Running, PhaseFailed) → Failed, emit pipeline:failed
        (Failed(recoverable), Retry) → Running(current), emit pipeline:retry
        (Blocked, Unblock) → Running(first_phase), emit pipeline:phase
        _ → no change
```

## Queue

Queues hold work items with ordering, visibility, and retry semantics.

### Data Structure

```rust
pub struct Queue {
    id: QueueId,
    items: Vec<QueueItem>,
    config: QueueConfig,
}

pub struct QueueItem {
    id: ItemId,
    data: Value,
    priority: i32,
    created_at: Instant,
    state: ItemState,
}

pub enum ItemState {
    Pending,
    Processing { holder: HolderId, taken_at: Instant, attempts: u32 },
    Completed { completed_at: Instant },
    Failed { error: String, attempts: u32 },
    Dead { reason: String },  // Exceeded max retries
}

pub struct QueueConfig {
    visibility_timeout: Duration,
    max_attempts: u32,
    dead_letter_queue: Option<QueueId>,
}
```

### Operations

All operations are pure: `(Queue, ...) → (Queue, Vec<Effect>)`

- **push(data, priority)** - Add item, emit `queue:push`
- **take(holder)** - Take highest-priority pending item (or timed-out item past visibility_timeout), emit `queue:take`
- **complete(item_id)** - Mark done, emit `queue:complete`
- **fail(item_id, error)** - If under max_attempts, return to pending; otherwise move to dead letter queue and emit `queue:dead`

## Task

Tasks represent single units of work, typically agent invocations.

### State Machine

```
┌─────────┐    ┌─────────┐    ┌─────────┐
│ pending │───▶│ running │───▶│  done   │
└─────────┘    └─────────┘    └─────────┘
                    │              ▲
                    │              │
                    ▼              │
               ┌─────────┐        │
               │  stuck  │────────┘
               └─────────┘  (after recovery)
```

### Data Structure

```rust
pub struct Task {
    id: TaskId,
    pipeline_id: PipelineId,
    session: Option<SessionId>,
    state: TaskState,
    last_heartbeat: Option<Instant>,
    recovery_attempts: u32,
    config: TaskConfig,
}

pub enum TaskState {
    Pending,
    Running { started_at: Instant },
    Stuck { detected_at: Instant, reason: StuckReason },
    Done { result: TaskResult },
}

pub enum StuckReason {
    NoHeartbeat { last_seen: Instant },
    NoProgress { duration: Duration },
    SessionDead,
}

pub enum TaskResult {
    Success { output: Value },
    Failed { error: String },
    Cancelled,
}

pub struct TaskConfig {
    heartbeat_timeout: Duration,
    max_recovery_attempts: u32,
    checkpoint_interval: Option<Duration>,
}
```

### Heartbeat Logic

```
heartbeat(status, clock) → (Task, Vec<Effect>):
    match (state, status):
        (Running, Active) → update last_heartbeat
        (Running, Idle) where idle > timeout → Stuck, emit task:stuck
        (Running, SessionDead) → Stuck(SessionDead), emit task:stuck
        (Stuck, Active) → Running, emit task:recovered

recover(action, clock) → (Task, Vec<Effect>):
    match action:
        Nudge → increment attempts, effect Nudge(session)
        Restart:
            if attempts >= max → Done(Failed), emit task:failed
            else → Pending, effect Kill(session), emit task:restarting
        Escalate → emit escalate
```

## Strategy

Strategies define ordered fallback chains for operations.

### Data Structure

```rust
pub struct Strategy {
    approaches: Vec<Approach>,
    current: usize,
}

pub struct Approach {
    name: String,
    action: Action,
    fallback_on: Vec<FallbackCondition>,
}

pub enum FallbackCondition {
    Error(String),         // Specific error pattern
    AnyError,              // Any error
    Timeout(Duration),     // Operation timeout
}

pub enum Action {
    Command(String),
    MergeStrategy(MergeStrategy),
    AgentPrompt(String),
}
```

### Evaluation

```
evaluate(result) → (Strategy, Outcome, Vec<Effect>):
    match result:
        Success(output) → Outcome::Success(output)
        Failed(error):
            if error matches fallback_on and has_next_approach:
                → advance to next, Outcome::TryNext, emit strategy:fallback
            else:
                → Outcome::Exhausted, emit strategy:exhausted
```

**Example: Merge strategy chain**
1. Try fast-forward → fallback on "not a fast-forward"
2. Try rebase → fallback on "conflict"
3. Try agent resolution → terminal

## Event

Events enable loose coupling and observability.

### Data Structure

```rust
pub struct Event {
    id: EventId,
    kind: EventKind,
    timestamp: Instant,
    source: EventSource,
    data: Value,
}

pub enum EventKind {
    // Pipeline events
    PipelineStarted,
    PipelinePhase { phase: String },
    PipelineComplete,
    PipelineFailed { error: String },

    // Task events
    TaskStarted,
    TaskStuck,
    TaskRecovered,
    TaskComplete,

    // Queue events
    QueuePush,
    QueueTake,
    QueueComplete,
    QueueDead,

    // System events
    Escalate { message: String },
    Heartbeat,

    // Custom runbook events
    Custom { category: String, action: String },
}

pub struct EventSource {
    component: String,
    id: Option<String>,
}
```

### Event Bus

The EventBus routes events to subscribers. Each subscriber defines `matches(event)` and `handle(event) → Vec<Effect>`. Routing is pure - it returns effects rather than executing them.

Common subscriber: **WorkerWakeSubscriber** - wakes a worker when specific events occur (e.g., wake merge worker on `pipeline:phase(merge)`).

## Effect Types

All effects that the core can produce:

```rust
pub enum Effect {
    // Events
    Emit(Event),

    // Session management
    Spawn { workspace: WorkspaceId, command: String },
    Send { session: SessionId, input: String },
    Kill { session: SessionId },
    Nudge { session: SessionId },

    // Git operations
    WorktreeAdd { branch: String, path: PathBuf },
    WorktreeRemove { path: PathBuf },
    Merge { branch: String, strategy: MergeStrategy },

    // Lock/Semaphore
    AcquireLock { name: String, holder: HolderId },
    ReleaseLock { name: String },
    AcquireSemaphore { name: String, holder: HolderId },
    ReleaseSemaphore { name: String, holder: HolderId },

    // Queue
    DeadLetter { queue: Option<QueueId>, item: QueueItem },

    // Worker
    WakeWorker { worker: WorkerId },

    // Notification
    Notify { title: String, message: String, sound: bool },

    // Storage
    Checkpoint { pipeline: PipelineId, message: String },
}
```

## See Also

- [Execution Layer](03-execution.md) - Workspace and Session design
- [Coordination](04-coordination.md) - Lock and Semaphore design
- [Testing Strategy](07-testing.md) - Testing these primitives

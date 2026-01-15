# Coordination Primitives

This document details Lock, Semaphore, and Guard designs for resource coordination.

## Overview

Coordination primitives control concurrent access to shared resources:

```
┌─────────────────────────────────────────────────────────┐
│                    Coordination Layer                    │
│                                                          │
│  ┌──────────┐    ┌──────────────┐    ┌──────────┐       │
│  │   Lock   │    │  Semaphore   │    │  Guard   │       │
│  │ (1 hold) │    │ (N holders)  │    │ (check)  │       │
│  └──────────┘    └──────────────┘    └──────────┘       │
│                                                          │
│  Use cases:      Use cases:          Use cases:          │
│  - Merges        - Agent slots       - Pre-conditions    │
│  - Deploys       - Build workers     - Post-conditions   │
│  - Migrations    - API rate limit    - Phase gates       │
└─────────────────────────────────────────────────────────┘
```

## Lock

Locks provide exclusive access to a resource with stale detection.

### Data Structure

```rust
pub struct Lock {
    name: String,
    state: LockState,
    config: LockConfig,
}

pub enum LockState {
    Free,
    Held {
        holder: HolderId,
        acquired_at: Instant,
        last_heartbeat: Instant,
        metadata: Value,
    },
}

pub struct LockConfig {
    heartbeat_timeout: Duration,
    max_hold_duration: Option<Duration>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct HolderId(String);

impl HolderId {
    pub fn from_pipeline(id: &PipelineId) -> Self {
        HolderId(format!("pipeline:{}", id))
    }

    pub fn from_worker(id: &WorkerId) -> Self {
        HolderId(format!("worker:{}", id))
    }
}
```

### Operations

```
acquire(holder, metadata, clock) → (Lock, AcquireResult, Vec<Effect>):
    match state:
        Free → Held(holder), Acquired, emit lock:acquired
        Held(same holder) → refresh heartbeat, Extended
        Held(other) if stale → Held(holder), Reclaimed, emit lock:reclaimed
        Held(other) → Busy

release(holder) → (Lock, ReleaseResult, Vec<Effect>):
    match state:
        Held(same holder) → Free, Released, emit lock:released
        Held(other) → NotOwner
        Free → AlreadyFree

heartbeat(holder, clock) → update last_heartbeat if holder matches
```

Key behavior: Locks can be **reclaimed** if the holder stops sending heartbeats past `heartbeat_timeout`. This prevents deadlocks from crashed processes.

## Semaphore

Semaphores limit concurrent access to N holders.

### Data Structure

```rust
pub struct Semaphore {
    name: String,
    capacity: u32,
    holders: Vec<SemaphoreHolder>,
    config: SemaphoreConfig,
}

pub struct SemaphoreHolder {
    id: HolderId,
    acquired_at: Instant,
    last_heartbeat: Instant,
    weight: u32,  // Can acquire multiple slots
}

pub struct SemaphoreConfig {
    heartbeat_timeout: Duration,
}
```

### Operations

```
acquire(holder, weight, clock) → (Semaphore, SemaphoreResult, Vec<Effect>):
    if holder already holding:
        if existing.weight >= weight → refresh heartbeat, AlreadyHeld
        if can increase → update weight, Increased
    if weight <= available → add holder, Acquired, emit semaphore:acquired
    else → Full { available }

release(holder) → (Semaphore, SemaphoreResult, Vec<Effect>):
    if holder in holders → remove, Released, emit semaphore:released
    else → NotHolder

reclaim_stale(clock) → (Semaphore, Vec<Effect>):
    partition holders by heartbeat > timeout
    remove stale holders, emit semaphore:reclaimed for each
```

Key behaviors:
- **Weighted slots** - Holders can acquire multiple slots at once
- **Heartbeat refresh** - Re-acquiring updates heartbeat without consuming more slots
- **Stale reclaim** - Like locks, holders that stop heartbeating are reclaimed

## Guard

Guards are pre/post conditions for phase transitions.

### Data Structure

```rust
pub struct Guard {
    name: String,
    condition: Condition,
    on_failure: GuardAction,
}

pub enum Condition {
    // Simple checks
    LockFree { lock: String },
    LockHeld { lock: String, by: Option<HolderId> },
    SemaphoreAvailable { semaphore: String, slots: u32 },

    // Issue checks
    IssuesComplete { filter: IssueFilter },
    NoBlockedIssues { filter: IssueFilter },

    // Git checks
    BranchExists { branch: String },
    BranchClean { branch: String },
    BranchMerged { branch: String, into: String },

    // File checks
    FileExists { path: String },
    FileContains { path: String, pattern: String },

    // Pipeline checks
    PipelineInPhase { pipeline: PipelineId, phase: String },
    PipelineComplete { pipeline: PipelineId },

    // Composite
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),

    // Custom
    Command { cmd: String, expect_success: bool },
}

pub enum GuardAction {
    Block,              // Prevent transition
    Warn,               // Log warning but allow
    Retry { delay: Duration }, // Retry after delay
    Fail { message: String },  // Fail the pipeline
}
```

### Evaluation

Guard evaluation is split: condition checking is pure, but gathering inputs requires I/O.

```rust
/// Inputs gathered by imperative shell
pub struct GuardInputs {
    pub locks: HashMap<String, Lock>,
    pub semaphores: HashMap<String, Semaphore>,
    pub issues: Vec<Issue>,
    pub branches: HashMap<String, BranchInfo>,
    pub files: HashMap<String, FileInfo>,
    pub pipelines: HashMap<PipelineId, Pipeline>,
    pub command_results: HashMap<String, CommandResult>,
}

pub enum GuardResult {
    Passed,
    Failed { guard: String, action: GuardAction },
}
```

```
evaluate(inputs) → GuardResult:
    if check_condition(condition, inputs) → Passed
    else → Failed { guard, action: on_failure }

check_condition(cond, inputs) → bool:
    match cond:
        LockFree { lock } → inputs.locks[lock] is Free (missing = free)
        LockHeld { lock, by } → inputs.locks[lock] held by expected holder
        SemaphoreAvailable { sem, slots } → inputs.semaphores[sem].available >= slots
        IssuesComplete { filter } → all matching issues are Done
        BranchExists/Clean { branch } → check inputs.branches
        FileExists { path } → check inputs.files
        PipelineComplete { pipeline } → inputs.pipelines[pipeline] is Done
        All(conds) → all conditions pass
        Any(conds) → any condition passes
        Not(cond) → condition fails
        Command { cmd, expect } → inputs.command_results[cmd].success == expect
```

### Guard Executor (Imperative Shell)

The executor gathers inputs via adapters, then calls pure evaluation:
1. Recursively walk the condition tree to determine what data is needed
2. Fetch required data via adapters (locks, issues, branches, etc.)
3. Call pure `evaluate(inputs)` function

## Coordination Manager

Combines all coordination primitives into a single state container:

```rust
pub struct CoordinationState {
    locks: HashMap<String, Lock>,
    semaphores: HashMap<String, Semaphore>,
}
```

```
handle(cmd, clock) → (CoordinationState, CoordinationResult, Vec<Effect>):
    match cmd:
        AcquireLock { name, holder, metadata } → delegate to lock.acquire
        ReleaseLock { name, holder } → delegate to lock.release
        AcquireSemaphore { name, holder, slots } → delegate to semaphore.acquire
        ReleaseSemaphore { name, holder } → delegate to semaphore.release

maintain(clock) → (CoordinationState, Vec<Effect>):
    for each semaphore: reclaim_stale
    collect all effects
```

The manager provides a unified interface and handles periodic maintenance (stale reclaim) for all primitives.

## See Also

- [Runbook Core](02-runbook-core.md) - Pipeline using guards
- [Storage](05-storage.md) - Persisting coordination state
- [Testing Strategy](07-testing.md) - Testing coordination

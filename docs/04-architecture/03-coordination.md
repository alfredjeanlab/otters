# Coordination Primitives

Lock, Semaphore, and Guard for resource coordination.

## Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Coordination Layer                   │
│                                                         │
│  ┌──────────┐    ┌──────────────┐    ┌──────────┐       │
│  │   Lock   │    │  Semaphore   │    │  Guard   │       │
│  │ (1 hold) │    │ (N holders)  │    │ (check)  │       │
│  └──────────┘    └──────────────┘    └──────────┘       │
│                                                         │
│  Use cases:      Use cases:          Use cases:         │
│  - Merges        - Agent slots       - Pre-conditions   │
│  - Deploys       - Build workers     - Post-conditions  │
│  - Migrations    - API rate limit    - Phase gates      │
└─────────────────────────────────────────────────────────┘
```

## Lock

Exclusive access to a resource with stale detection.

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
    },
}

pub struct LockConfig {
    heartbeat_timeout: Duration,
    max_hold_duration: Option<Duration>,
}
```

### Operations

```
acquire(holder, clock) → AcquireResult:
    Free → Held(holder), Acquired
    Held(same) → refresh heartbeat, Extended
    Held(other) if stale → Held(holder), Reclaimed
    Held(other) → Busy

release(holder) → ReleaseResult:
    Held(same) → Free, Released
    Held(other) → NotOwner
    Free → AlreadyFree

heartbeat(holder, clock) → update last_heartbeat if holder matches
```

Locks can be **reclaimed** if the holder stops heartbeating past `heartbeat_timeout`. This prevents deadlocks from crashed processes.

## Semaphore

Limited concurrent access to N holders.

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
```

### Operations

```
acquire(holder, weight, clock) → SemaphoreResult:
    if already holding with weight >= requested → refresh, AlreadyHeld
    if weight <= available → add holder, Acquired
    else → Full { available }

release(holder) → SemaphoreResult:
    if holder in holders → remove, Released
    else → NotHolder

reclaim_stale(clock):
    remove holders past heartbeat_timeout
```

Key behaviors:
- **Weighted slots** - Holders can acquire multiple slots at once
- **Heartbeat refresh** - Re-acquiring updates heartbeat without consuming more slots
- **Stale reclaim** - Holders that stop heartbeating are reclaimed

## Guard

Shell condition that gates phase transitions.

```rust
pub struct Guard {
    name: String,
    condition: String,      // Shell command, exit 0 = pass
    timeout: Duration,
    retry: Option<RetryConfig>,
    wake_on: Vec<String>,   // Events that trigger re-check
    on_timeout: GuardAction,
}

pub struct RetryConfig {
    max: u32,
    interval: Duration,
}

pub enum GuardAction {
    Block,      // Keep waiting
    Fail,       // Fail the pipeline
    Escalate,   // Alert human
}
```

### Evaluation

Guards are simple: run the shell command, check exit code.

```
evaluate(guard, context) → GuardResult:
    cmd = interpolate(guard.condition, context)
    result = run_shell(cmd)
    if result.exit_code == 0 → Passed
    else → Failed
```

Context provides interpolation variables: `{name}`, `{workspace}`, `{branch}`, pipeline inputs, etc.

### Wake-on Events

Guards can subscribe to events instead of polling:

```toml
[guard.blocker_merged]
condition = "oj pipeline show {after} --phase | grep -q done"
wake_on = ["pipeline:{after}:complete"]
```

When the event fires, the guard re-evaluates immediately rather than waiting for the next poll interval.

### Examples

```toml
# File exists
[guard.plan_exists]
condition = "test -f plans/{name}.md"
timeout = "30m"
on_timeout = "escalate"

# All issues closed
[guard.issues_closed]
condition = "wok list -l plan:{name} -s todo,in_progress --count | grep -q '^0$'"
retry = { max = 3, interval = "10s" }
timeout = "5m"

# Branch exists on remote
[guard.branch_exists]
condition = "git ls-remote --heads origin {branch} | grep -q {branch}"
timeout = "1m"

# Dependency complete
[guard.after_done]
condition = "test -z '{after}' || oj pipeline show {after} --phase | grep -q done"
wake_on = ["pipeline:{after}:complete"]
```

## Coordination State

Combined state container:

```rust
pub struct CoordinationState {
    locks: HashMap<String, Lock>,
    semaphores: HashMap<String, Semaphore>,
}
```

Guards don't store state—they're evaluated on demand during phase transitions.

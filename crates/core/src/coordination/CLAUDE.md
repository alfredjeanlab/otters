# Coordination Module

Distributed resource management primitives for multi-worker systems.

## State Machine Invariants

### Lock Invariants

```
INVARIANT: A lock has at most one holder at any time
INVARIANT: Stale locks (no heartbeat for `stale_after`) can be reclaimed
INVARIANT: Lock state transitions are atomic
```

```mermaid
stateDiagram-v2
    Unlocked --> Locked: acquire(holder)
    Locked --> Unlocked: release(holder)
    Locked --> Locked: reclaim(new_holder) [if stale]
```

### Semaphore Invariants

```
INVARIANT: Sum of holder weights <= capacity
INVARIANT: Stale holders are removed on tick()
INVARIANT: Weight must be > 0
```

```mermaid
stateDiagram-v2
    Available --> Available: acquire(holder, weight)
    Available --> Blocked: acquire when capacity=0
    Blocked --> Available: release(holder, weight)
```

### Guard Invariants

```
INVARIANT: Guards are evaluated atomically
INVARIANT: NeedsInput returns specific data requirements
INVARIANT: Composition (and/or) preserves invariants
```

```mermaid
stateDiagram-v2
    [*] --> Evaluate
    Evaluate --> Passed: Condition met
    Evaluate --> Failed: Condition not met
    Evaluate --> NeedsInput: More data needed
    Passed --> [*]
    Failed --> [*]
    NeedsInput --> [*]
```

## Effect Ordering

Coordination effects must execute in causal order:
1. `CheckGuard` before `AcquireLock`
2. `AcquireLock` before `StartWork`
3. `ReleaseResource` after work completion

## Landing Checklist

- [ ] State transitions are deterministic (same input -> same output)
- [ ] Invariants documented in code comments
- [ ] Property tests for state machine transitions
- [ ] Staleness thresholds are configurable

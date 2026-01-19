# Scheduling Module

Time-driven execution primitives for proactive system health.

## Primitives

| Primitive | Purpose | State Machine |
|-----------|---------|---------------|
| **Cron** | Fixed-interval scheduled tasks | Disabled ↔ Enabled → Running → Enabled |
| **Action** | Cooldown-enforced operations | Ready → Executing → Cooling → Ready |
| **Watcher** | Condition monitoring | Active → Triggered → Active/Escalated |
| **Scanner** | Resource cleanup | Idle → Scanning → Cleaning → Idle |

## Invariants

### State Machine Invariants

```
INVARIANT: All state transitions are pure (State, Vec<Effect>) -> (State, Vec<Effect>)
INVARIANT: All time-dependent code accepts &impl Clock for testability
INVARIANT: Invalid transitions are no-ops (return original state, empty effects)
INVARIANT: Running cron prevents overlapping execution
INVARIANT: Action cooldowns are always enforced
INVARIANT: Watcher response chains escalate on failure
INVARIANT: Scanner cleanup is idempotent
```

### Integration Layer Invariants

```
INVARIANT: CronController orchestrates watchers/scanners linked to crons
INVARIANT: SourceFetcher provides values for watcher condition evaluation
INVARIANT: ResourceScanner discovers resources for scanner evaluation
INVARIANT: ActionExecutor handles Command/Task/Rules execution
INVARIANT: WatcherEventBridge enables event-driven wake (bypass polling)
INVARIANT: CleanupExecutor maps scanner effects to adapter calls
```

## Timer ID Conventions

```
cron:{id}              - Cron interval timer
action:{id}:cooldown   - Action cooldown timer
watcher:{id}:check     - Watcher check interval
watcher:{id}:response  - Watcher response delay
scanner:{id}           - Scanner scan interval
```

## Effect Patterns

All scheduling primitives emit effects, never execute side effects directly:

```rust
// Setting timers
Effect::SetTimer { id: timer_id, duration }

// Canceling timers
Effect::CancelTimer { id: timer_id }

// Emitting events
Effect::Emit(Event::CronTriggered { id })
```

## Testing

Use `FakeClock` for all tests:

```rust
#[test]
fn cron_schedules_on_enable() {
    let clock = FakeClock::new();
    let cron = Cron::new(id, config, &clock);
    let (cron, effects) = cron.transition(CronEvent::Enable, &clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
}
```

## Landing Checklist

- [ ] State transitions return (State, Vec<Effect>)
- [ ] All time code uses &impl Clock
- [ ] Tests use FakeClock
- [ ] WAL operations added for persistence
- [ ] Events emitted for observability
- [ ] Invalid transitions are silent no-ops

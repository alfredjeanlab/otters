# Epic 9b: Engine Scheduling Integration

**Root Feature:** `otters-199f`

**Depends on**: Epic 9a (Runbook Types), Epic 9c (Production Implementations), Epic 9d (WAL Execution Tracking)
**Blocks**: None (final integration layer)

## Problem Statement

The engine's `tick_scheduling()` routes timers to `process_timer` but doesn't:
- Use `CronController` for orchestrating linked watchers/scanners
- Execute action effects when `ActionTriggered` events occur
- Wake watchers on matching events via `WatcherEventBridge`

The integration layer components exist (from Epic 8b) but aren't wired into the engine.

## Goal

Connect scheduling primitives to the engine execution loop so crons trigger watchers/scanners, actions execute, and watchers wake on events.

## Implementation

### 1. Update `tick_scheduling()` in `crates/core/src/engine/runtime.rs`

```rust
pub fn tick_scheduling(&mut self) -> Vec<Effect> {
    // Poll scheduler for due items
    // For each item: route cron timers to on_cron_tick(), others to scheduling_manager
    // Execute effects in loop until no nested effects remain
}
```

### 2. Add `on_cron_tick()` Method

```rust
/// Handle a cron tick with full integration
fn on_cron_tick(&mut self, cron_id: &CronId) -> Vec<Effect> {
    // 1. Tick cron to update state
    // 2. Create CronController with source_fetcher and resource_scanner
    // 3. Call controller.on_cron_tick() for orchestration
    // 4. Reschedule cron if still enabled
    // Return combined effects
}
```

### 3. Add `execute_scheduling_effect()` Method

```rust
/// Execute a scheduling effect, returning any nested effects
fn execute_scheduling_effect(&mut self, effect: &Effect) -> Vec<Effect> {
    // Match on effect type:
    // - ActionTriggered: delegate to handle_action_triggered
    // - Other events: wake subscribed watchers, execute cleanup, publish to bus
    // - SetTimer/CancelTimer: schedule/cancel via scheduler
}

/// Check a watcher immediately (event-driven wake)
fn check_watcher_immediate(&mut self, watcher_id: &WatcherId) -> Vec<Effect> {
    // Create CronController and call check_watcher()
}
```

### 4. Add `handle_action_triggered()` Method

```rust
/// Handle action triggered event
fn handle_action_triggered(&mut self, action_id: &str, source: &str) -> Vec<Effect> {
    // 1. Look up action, return early if not found
    // 2. Record execution start in WAL
    // 3. Execute via ActionExecutor
    // 4. On success: record completion, update state, emit ActionExecuted
    // 5. On failure: record failure, update state, emit ActionExecuted(success=false)
}
```

### 5. Add `execute_cleanup_effect()` Method

```rust
/// Execute cleanup effects from scanners
fn execute_cleanup_effect(&mut self, effect: &Effect) -> Result<(), CleanupError> {
    // Execute via cleanup_executor, record result in WAL on success
}

fn extract_scanner_id(&self, event: &Event) -> ScannerId {
    // Match Scanner*Resource events to extract scanner_id
}
```

### 6. Add Factory Methods for Adapters

```rust
impl<A: Adapters, C: Clock> Engine<A, C> {
    fn create_source_fetcher(&self) -> Box<dyn SourceFetcher + '_> { /* DefaultSourceFetcher */ }
    fn create_resource_scanner(&self) -> Box<dyn ResourceScanner + '_> { /* DefaultResourceScanner */ }
    fn create_action_executor(&self) -> ActionExecutor<'_> { /* ActionExecutor with defaults */ }
    fn create_cleanup_executor(&self) -> CleanupExecutor<'_> { /* CleanupExecutor with adapters */ }
}
```

### 7. Add Initialization Method

```rust
/// Initialize scheduling from loaded runbooks
pub fn init_scheduling(&mut self) {
    // Register watchers with event bridge for wake_on patterns
    // Schedule enabled crons at their intervals
    // Log counts of registered primitives
}
```

## Files

- `crates/core/src/engine/runtime.rs` - Integration methods
- `crates/core/src/engine/scheduling.rs` - NEW: Scheduling-specific engine methods (optional split)
- `crates/core/src/engine/runtime_tests.rs` - Integration tests

## Tests

```rust
#[test]
fn cron_tick_triggers_linked_watchers() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    // Create cron with linked watcher
    let watcher_id = engine.scheduling_manager_mut()
        .create_watcher(WatcherConfig::new(
            "idle-check",
            WatcherSource::Session { name: "agent-1".into() },
            WatcherCondition::Idle { threshold: Duration::from_secs(300) },
            Duration::from_secs(60),
        ));

    engine.scheduling_manager_mut()
        .create_cron(CronConfig::new("health", Duration::from_secs(60))
            .enabled()
            .with_watchers(vec![watcher_id.clone()]));

    engine.init_scheduling();

    // Advance past cron interval
    clock.advance(Duration::from_secs(61));
    let effects = engine.tick_scheduling();

    // Watcher should have been checked
    assert!(effects.iter().any(|e| matches!(e,
        Effect::Emit(Event::WatcherChecked { id, .. }) if id == &watcher_id.0
    )));
}

#[test]
fn action_triggered_executes_command() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    engine.scheduling_manager_mut()
        .create_action(ActionConfig::new("notify", Duration::from_secs(60))
            .with_command("echo 'hello'"));

    // Trigger action
    let effect = Effect::Emit(Event::ActionTriggered {
        id: "notify".into(),
        source: "test".into(),
    });

    engine.execute_scheduling_effect(&effect);

    // Command should have been executed
    assert!(engine.fake_adapters().commands_run()
        .iter()
        .any(|c| c.contains("echo")));
}

#[test]
fn watcher_wakes_on_event() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    let watcher_id = engine.scheduling_manager_mut()
        .create_watcher(WatcherConfig::new(
            "failure-monitor",
            WatcherSource::Events { pattern: "task:failed:*".into() },
            WatcherCondition::ConsecutiveFailures { count: 3 },
            Duration::from_secs(60),
        ).with_wake_on(vec!["task:failed:*".into()]));

    engine.init_scheduling();

    // Emit matching event
    let effect = Effect::Emit(Event::TaskFailed {
        id: "task-1".into(),
        error: "test".into(),
    });

    let nested = engine.execute_scheduling_effect(&effect);

    // Watcher should have been checked
    assert!(nested.iter().any(|e| matches!(e,
        Effect::Emit(Event::WatcherChecked { id, .. }) if id == &watcher_id.0
    )));
}

#[test]
fn full_watchdog_flow() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    // Load watchdog runbook
    engine.load_runbook("runbooks/watchdog.toml").unwrap();
    engine.init_scheduling();

    // Simulate idle session
    engine.fake_adapters_mut().set_session_idle("agent-1", Duration::from_secs(360));

    // Tick scheduling
    clock.advance(Duration::from_secs(60));
    engine.tick_scheduling();

    // Verify nudge action was triggered
    assert!(engine.fake_adapters().commands_run()
        .iter()
        .any(|c| c.contains("nudge") || c.contains("Are you still working")));
}
```

## Landing Checklist

- [ ] `tick_scheduling()` uses `CronController` for orchestration
- [ ] Actions execute when `ActionTriggered` events are emitted
- [ ] Watchers wake immediately on matching events via bridge
- [ ] Cleanup effects are executed and recorded
- [ ] `init_scheduling()` registers watchers and schedules crons
- [ ] Full watchdog flow integration test passes
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

# Epic 8b: Scheduling Integration Layer

**Root Feature:** `otters-f5ad`

Rectify the gap between Epic 8's pure state machines and the RUNBOOKS.md design by adding execution, event-driven wake, and cron-as-entrypoint semantics.

## 1. Problem Statement

Epic 8 implemented **scheduling primitives** (Cron, Action, Watcher, Scanner) as pure state machines that emit effects. However, these primitives are disconnected from:

1. **Execution** - Actions emit `ActionTriggered` but nothing executes them
2. **Source fetching** - Watchers need `SourceValue` but can't fetch it
3. **Resource scanning** - Scanners filter resources but can't discover them
4. **Event-driven wake** - Watchers poll on timers, can't subscribe to events
5. **Cron→Watcher binding** - Crons are standalone, not entrypoints that run watchers

The RUNBOOKS.md design expects:
```toml
[cron.watchdog]
interval = "30s"
watchers = ["agent_idle", "phase_timeout"]

[watcher.agent_idle]
source = "oj pipeline list --phase execute --json"
condition = "oj session idle-time {session} > 5m"
response = ["nudge", "restart:2", "escalate"]

[action.nudge]
run = "oj session nudge {session}"
cooldown = "30s"
```

This requires an **integration layer** between the state machines and the outside world.

## 2. Design Principles

1. **Keep state machines pure** - Don't add side effects to existing primitives
2. **Support both command and typed sources** - Shell commands for flexibility, typed for performance
3. **Event-driven where possible** - Reduce polling, use `wake_on` patterns
4. **Adapters for external I/O** - Testable with fakes
5. **Crons are entrypoints** - They orchestrate watchers/scanners, not standalone timers

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│ ENTRYPOINTS (things that run)                                       │
│                                                                     │
│   Cron ─────► ticks ─────► triggers watchers/scanners               │
│                              │                                      │
└──────────────────────────────┼──────────────────────────────────────┘
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│ INTEGRATION LAYER (new)                                             │
│                                                                     │
│   SourceFetcher ──► fetch source value for watcher                  │
│   ResourceScanner ─► discover resources for scanner                 │
│   ActionExecutor ──► execute action commands/tasks                  │
│   EventWatcher ────► subscribe to events, wake watchers             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│ PRIMITIVES (Epic 8 - unchanged)                                     │
│                                                                     │
│   Cron ────► state machine (Enabled/Running/Disabled)               │
│   Action ──► state machine (Ready/Executing/Cooling)                │
│   Watcher ─► state machine (Active/Triggered/Paused)                │
│   Scanner ─► state machine (Idle/Scanning/Cleaning)                 │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## 4. Implementation Phases

### Phase 1: Cron as Entrypoint

**Goal**: Crons trigger watchers/scanners on tick, not just emit events.

**Changes**:

1. Add `CronConfig` fields:
```rust
pub struct CronConfig {
    pub name: String,
    pub interval: Duration,
    pub enabled: bool,
    // NEW: What this cron runs
    pub watchers: Vec<WatcherId>,
    pub scanners: Vec<ScannerId>,
}
```

2. Add `CronController` to orchestrate:
```rust
/// Orchestrates cron execution - connects cron ticks to watcher/scanner checks
pub struct CronController<'a, C: Clock> {
    manager: &'a mut SchedulingManager,
    source_fetcher: &'a dyn SourceFetcher,
    resource_scanner: &'a dyn ResourceScanner,
    clock: &'a C,
}

impl<'a, C: Clock> CronController<'a, C> {
    /// Called when a cron timer fires
    pub fn on_cron_tick(&mut self, cron_id: &CronId) -> Vec<Effect> {
        let mut effects = self.manager.tick_cron(cron_id, self.clock);

        // Get cron config to find linked watchers/scanners
        if let Some(cron) = self.manager.get_cron(cron_id) {
            // Trigger each linked watcher
            for watcher_id in &cron.watchers {
                let source_effects = self.check_watcher(watcher_id);
                effects.extend(source_effects);
            }

            // Trigger each linked scanner
            for scanner_id in &cron.scanners {
                let scan_effects = self.run_scanner(scanner_id);
                effects.extend(scan_effects);
            }
        }

        effects
    }
}
```

3. Update `Cron` struct to store watcher/scanner references:
```rust
pub struct Cron {
    // ... existing fields ...
    pub watchers: Vec<WatcherId>,
    pub scanners: Vec<ScannerId>,
}
```

**Files**:
- `crates/core/src/scheduling/cron.rs` - Add watchers/scanners fields
- `crates/core/src/scheduling/controller.rs` - New CronController
- `crates/core/src/scheduling/controller_tests.rs` - Tests

**Milestone**: Cron tick triggers linked watchers and scanners.

---

### Phase 2: Source Fetching

**Goal**: Automatically fetch source values for watcher condition evaluation.

**Design**:

```rust
/// Fetches source values for watcher condition evaluation
pub trait SourceFetcher: Send + Sync {
    /// Fetch the current value of a watcher source
    fn fetch(&self, source: &WatcherSource, context: &FetchContext) -> Result<SourceValue, FetchError>;
}

/// Context for template interpolation
pub struct FetchContext {
    pub variables: HashMap<String, String>,
}

/// Production implementation
pub struct DefaultSourceFetcher {
    session_adapter: Arc<dyn SessionAdapter>,
    command_runner: Arc<dyn CommandRunner>,
}

impl SourceFetcher for DefaultSourceFetcher {
    fn fetch(&self, source: &WatcherSource, context: &FetchContext) -> Result<SourceValue, FetchError> {
        match source {
            WatcherSource::Task { id } => {
                // Query task state from engine
                self.fetch_task_state(id)
            }
            WatcherSource::Session { name } => {
                // Query session idle time
                let idle = self.session_adapter.idle_time(name)?;
                Ok(SourceValue::Idle { duration: idle })
            }
            WatcherSource::Queue { name } => {
                // Query queue depth
                let depth = self.fetch_queue_depth(name)?;
                Ok(SourceValue::Numeric { value: depth })
            }
            WatcherSource::Events { pattern } => {
                // Query event log for recent matches
                self.fetch_event_matches(pattern)
            }
            WatcherSource::Command { command } => {
                // Execute shell command, parse output
                let interpolated = self.interpolate(command, context)?;
                let output = self.command_runner.run(&interpolated)?;
                self.parse_command_output(&output)
            }
        }
    }
}

/// Fake for testing
pub struct FakeSourceFetcher {
    responses: HashMap<String, SourceValue>,
}
```

**Integration with watcher check**:

```rust
impl<'a, C: Clock> CronController<'a, C> {
    fn check_watcher(&mut self, watcher_id: &WatcherId) -> Vec<Effect> {
        let Some(watcher) = self.manager.get_watcher(watcher_id) else {
            return vec![];
        };

        // Fetch source value
        let context = self.build_context(watcher);
        let source_value = match self.source_fetcher.fetch(&watcher.source, &context) {
            Ok(value) => value,
            Err(e) => SourceValue::Error { message: e.to_string() },
        };

        // Check watcher with fetched value
        self.manager.check_watcher(watcher_id, source_value, self.clock)
    }
}
```

**Files**:
- `crates/core/src/scheduling/source.rs` - SourceFetcher trait + implementations
- `crates/core/src/scheduling/source_tests.rs` - Tests
- `crates/core/src/scheduling/controller.rs` - Integration

**Milestone**: Watchers automatically fetch source values on check.

---

### Phase 3: Action Execution

**Goal**: Execute action commands/tasks when `ActionTriggered` events are emitted.

**Design**:

```rust
/// Configuration for an action's execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionExecution {
    /// Run a shell command
    Command {
        run: String,
        #[serde(default)]
        timeout: Option<Duration>,
    },
    /// Invoke a task
    Task {
        task: String,
        #[serde(default)]
        inputs: HashMap<String, String>,
    },
    /// Decision rules
    Rules {
        rules: Vec<DecisionRule>,
    },
    /// No execution (just state tracking)
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRule {
    #[serde(rename = "if")]
    pub condition: Option<String>,
    #[serde(rename = "else")]
    pub is_else: Option<bool>,
    pub then: String,
    #[serde(default)]
    pub delay: Option<Duration>,
}

/// Executes actions when triggered
pub struct ActionExecutor<A: Adapters> {
    adapters: A,
    command_runner: Arc<dyn CommandRunner>,
}

impl<A: Adapters> ActionExecutor<A> {
    /// Handle an ActionTriggered event
    pub fn on_action_triggered(
        &self,
        action: &Action,
        source: &str,
        context: &ExecutionContext,
    ) -> Result<ActionResult, ExecutionError> {
        match &action.execution {
            ActionExecution::Command { run, timeout } => {
                let interpolated = self.interpolate(run, context)?;
                let result = self.command_runner.run_with_timeout(&interpolated, *timeout)?;
                Ok(ActionResult::CommandOutput(result))
            }
            ActionExecution::Task { task, inputs } => {
                // Create and start task
                let task_id = self.start_task(task, inputs, context)?;
                Ok(ActionResult::TaskStarted(task_id))
            }
            ActionExecution::Rules { rules } => {
                // Evaluate rules in order
                for rule in rules {
                    if self.evaluate_rule_condition(rule, context)? {
                        return Ok(ActionResult::RuleMatched {
                            action: rule.then.clone(),
                            delay: rule.delay,
                        });
                    }
                }
                Err(ExecutionError::NoRuleMatched)
            }
            ActionExecution::None => Ok(ActionResult::NoOp),
        }
    }
}
```

**Event listener integration**:

```rust
/// Listens for action events and executes them
pub struct ActionEventHandler {
    executor: ActionExecutor,
    manager: SchedulingManager,
}

impl ActionEventHandler {
    pub fn handle_event(&mut self, event: &Event, clock: &impl Clock) -> Vec<Effect> {
        match event {
            Event::ActionTriggered { id, source } => {
                let action_id = ActionId::new(id);
                if let Some(action) = self.manager.get_action(&action_id) {
                    let context = self.build_context(source);
                    match self.executor.on_action_triggered(action, source, &context) {
                        Ok(result) => {
                            // Mark action as completed
                            self.manager.complete_action(&action_id, clock)
                        }
                        Err(e) => {
                            // Mark action as failed
                            self.manager.fail_action(&action_id, e.to_string(), clock)
                        }
                    }
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}
```

**Files**:
- `crates/core/src/scheduling/action.rs` - Add ActionExecution enum
- `crates/core/src/scheduling/executor.rs` - ActionExecutor
- `crates/core/src/scheduling/executor_tests.rs` - Tests

**Milestone**: Actions execute commands/tasks when triggered.

---

### Phase 4: Event-Driven Wake (wake_on)

**Goal**: Watchers can subscribe to events instead of polling.

**Design**:

Add `wake_on` to watcher config:

```rust
pub struct WatcherConfig {
    pub name: String,
    pub source: WatcherSource,
    pub condition: WatcherCondition,
    pub response_chain: Vec<WatcherResponse>,
    pub check_interval: Duration,
    // NEW: Event patterns that trigger immediate check
    pub wake_on: Vec<String>,
}

pub struct Watcher {
    // ... existing fields ...
    pub wake_on: Vec<EventPattern>,
}
```

Add `WatcherEventBridge` to connect EventBus to watchers:

```rust
/// Bridges EventBus to watchers for event-driven wake
pub struct WatcherEventBridge {
    /// Map from event pattern to watcher IDs that should wake
    subscriptions: HashMap<EventPattern, Vec<WatcherId>>,
}

impl WatcherEventBridge {
    /// Register a watcher's wake_on patterns
    pub fn register(&mut self, watcher_id: WatcherId, patterns: Vec<EventPattern>) {
        for pattern in patterns {
            self.subscriptions
                .entry(pattern)
                .or_default()
                .push(watcher_id.clone());
        }
    }

    /// Unregister a watcher
    pub fn unregister(&mut self, watcher_id: &WatcherId) {
        for watchers in self.subscriptions.values_mut() {
            watchers.retain(|id| id != watcher_id);
        }
    }

    /// Get watchers that should wake for an event
    pub fn watchers_for_event(&self, event_name: &str) -> Vec<WatcherId> {
        self.subscriptions
            .iter()
            .filter(|(pattern, _)| pattern.matches(event_name))
            .flat_map(|(_, watchers)| watchers.clone())
            .collect()
    }
}
```

Integrate with engine event handling:

```rust
impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Handle an event, including waking watchers
    fn handle_event(&mut self, event: &Event) -> Vec<Effect> {
        let mut effects = vec![];

        // Existing event handling...
        self.event_bus.publish(event.clone());

        // NEW: Wake watchers subscribed to this event
        let event_name = event.name();
        let watchers_to_wake = self.watcher_bridge.watchers_for_event(&event_name);

        for watcher_id in watchers_to_wake {
            let watcher_effects = self.cron_controller.check_watcher(&watcher_id);
            effects.extend(watcher_effects);
        }

        effects
    }
}
```

**Example usage**:

```toml
[watcher.task-failure]
source = { type = "events", pattern = "task:failed:*" }
condition = { type = "consecutive_failures", count = 3 }
wake_on = ["task:failed"]  # Wake immediately on task failure
check_interval = "5m"      # Fallback polling
response = ["notify-team", "pause-pipelines"]
```

**Files**:
- `crates/core/src/scheduling/watcher.rs` - Add wake_on field
- `crates/core/src/scheduling/bridge.rs` - WatcherEventBridge
- `crates/core/src/scheduling/bridge_tests.rs` - Tests
- `crates/core/src/engine/runtime.rs` - Integration

**Milestone**: Watchers wake immediately on matching events.

---

### Phase 5: Resource Scanning

**Goal**: Scanners automatically discover and filter resources.

**Design**:

```rust
/// Discovers resources for scanner condition evaluation
pub trait ResourceScanner: Send + Sync {
    /// Scan for resources matching the source type
    fn scan(&self, source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError>;
}

/// Production implementation
pub struct DefaultResourceScanner {
    coordination: Arc<CoordinationManager>,
    storage: Arc<WalStore>,
    adapters: Arc<dyn Adapters>,
}

impl ResourceScanner for DefaultResourceScanner {
    fn scan(&self, source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        match source {
            ScannerSource::Locks => {
                self.coordination.list_locks()
                    .map(|locks| locks.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Semaphores => {
                self.coordination.list_semaphore_slots()
                    .map(|slots| slots.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Queue { name } => {
                self.storage.list_queue_items(name)
                    .map(|items| items.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Worktrees => {
                self.adapters.repo().list_worktrees()
                    .map(|wts| wts.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Pipelines => {
                self.storage.list_pipelines()
                    .map(|pls| pls.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Sessions => {
                self.adapters.session().list_sessions()
                    .map(|ss| ss.into_iter().map(ResourceInfo::from).collect())
            }
            ScannerSource::Command { command } => {
                // Execute command, parse JSON output
                self.run_and_parse(command)
            }
        }
    }
}
```

**Integration with scanner tick**:

```rust
impl<'a, C: Clock> CronController<'a, C> {
    fn run_scanner(&mut self, scanner_id: &ScannerId) -> Vec<Effect> {
        let Some(scanner) = self.manager.get_scanner(scanner_id) else {
            return vec![];
        };

        // Start scanning
        let mut effects = self.manager.tick_scanner(scanner_id, self.clock);

        // Fetch resources
        let resources = match self.resource_scanner.scan(&scanner.source) {
            Ok(r) => r,
            Err(e) => {
                // Fail the scan
                return self.manager.cleanup_failed(
                    scanner_id,
                    e.to_string(),
                    self.clock,
                );
            }
        };

        // Complete scan with discovered resources
        effects.extend(self.manager.scan_complete(scanner_id, resources, self.clock));

        effects
    }
}
```

**Files**:
- `crates/core/src/scheduling/resource.rs` - ResourceScanner trait + implementations
- `crates/core/src/scheduling/resource_tests.rs` - Tests
- `crates/core/src/scheduling/controller.rs` - Integration

**Milestone**: Scanners automatically discover and clean resources.

---

### Phase 6: Cleanup Effect Execution

**Goal**: Execute scanner cleanup effects (delete, release, archive, etc.).

**Design**:

```rust
/// Executes cleanup effects from scanners
pub struct CleanupExecutor<A: Adapters> {
    adapters: A,
    coordination: CoordinationManager,
    storage: WalStore,
}

impl<A: Adapters> CleanupExecutor<A> {
    pub fn execute(&mut self, effect: &Effect) -> Result<(), CleanupError> {
        match effect {
            Effect::Emit(Event::ScannerDeleteResource { resource_id, .. }) => {
                self.delete_resource(resource_id)
            }
            Effect::Emit(Event::ScannerReleaseResource { resource_id, .. }) => {
                self.release_resource(resource_id)
            }
            Effect::Emit(Event::ScannerArchiveResource { resource_id, destination, .. }) => {
                self.archive_resource(resource_id, destination)
            }
            Effect::Emit(Event::ScannerFailResource { resource_id, reason, .. }) => {
                self.fail_resource(resource_id, reason)
            }
            Effect::Emit(Event::ScannerDeadLetterResource { resource_id, .. }) => {
                self.dead_letter_resource(resource_id)
            }
            _ => Ok(()),
        }
    }

    fn delete_resource(&mut self, resource_id: &str) -> Result<(), CleanupError> {
        // Parse resource type from ID prefix
        if resource_id.starts_with("lock:") {
            self.coordination.force_release_lock(&resource_id[5..])
        } else if resource_id.starts_with("worktree:") {
            self.adapters.repo().remove_worktree(&resource_id[9..])
        } else if resource_id.starts_with("session:") {
            self.adapters.session().kill(&resource_id[8..])
        } else {
            Err(CleanupError::UnknownResourceType(resource_id.to_string()))
        }
    }
}
```

**Files**:
- `crates/core/src/scheduling/cleanup.rs` - CleanupExecutor
- `crates/core/src/scheduling/cleanup_tests.rs` - Tests

**Milestone**: Scanner cleanup effects are executed.

---

### Phase 7: Engine Integration

**Goal**: Wire everything together in the engine.

**Changes to Engine struct**:

```rust
pub struct Engine<A: Adapters, C: Clock> {
    // ... existing fields ...

    // Scheduling system (Epic 8)
    scheduler: Scheduler,
    scheduling_manager: SchedulingManager,

    // NEW: Integration layer (Epic 8b)
    source_fetcher: Box<dyn SourceFetcher>,
    resource_scanner: Box<dyn ResourceScanner>,
    action_executor: ActionExecutor<A>,
    cleanup_executor: CleanupExecutor<A>,
    watcher_bridge: WatcherEventBridge,
}
```

**Updated tick_scheduling**:

```rust
impl<A: Adapters, C: Clock> Engine<A, C> {
    pub fn tick_scheduling(&mut self) -> Vec<Effect> {
        let now = self.clock.now();
        let due_items = self.scheduler.poll(now);

        let mut all_effects = Vec::new();

        for item in due_items {
            match &item.kind {
                ScheduledKind::Timer { id } => {
                    // Use CronController for integrated handling
                    let mut controller = CronController {
                        manager: &mut self.scheduling_manager,
                        source_fetcher: self.source_fetcher.as_ref(),
                        resource_scanner: self.resource_scanner.as_ref(),
                        clock: &self.clock,
                    };

                    let effects = if id.starts_with("cron:") {
                        let cron_id = CronId::new(&id[5..]);
                        controller.on_cron_tick(&cron_id)
                    } else {
                        self.scheduling_manager.process_timer(id, &self.clock)
                    };

                    all_effects.extend(effects);
                }
                // ... other kinds
            }
        }

        // Execute effects
        for effect in &all_effects {
            self.execute_effect(effect);
        }

        all_effects
    }

    fn execute_effect(&mut self, effect: &Effect) {
        match effect {
            Effect::Emit(event) => {
                // Handle action triggered events
                if let Event::ActionTriggered { id, source } = event {
                    self.handle_action_triggered(id, source);
                }

                // Handle cleanup events
                if let Err(e) = self.cleanup_executor.execute(effect) {
                    tracing::error!(?e, "cleanup failed");
                }

                // Publish to event bus (wakes watchers via bridge)
                let wake_effects = self.handle_event(event);
                for wake_effect in wake_effects {
                    self.execute_effect(&wake_effect);
                }
            }
            // ... other effects
        }
    }
}
```

**Files**:
- `crates/core/src/engine/runtime.rs` - Updated integration
- `crates/core/src/engine/runtime_tests.rs` - Integration tests

**Milestone**: Full end-to-end scheduling flow works.

---

## 5. File Structure

```
crates/core/src/scheduling/
├── mod.rs                    # Module exports (updated)
├── cron.rs                   # Add watchers/scanners fields
├── action.rs                 # Add ActionExecution enum
├── watcher.rs                # Add wake_on field
├── scanner.rs                # (unchanged)
├── manager.rs                # (minor updates)
├── controller.rs             # NEW: CronController
├── controller_tests.rs       # NEW: Controller tests
├── source.rs                 # NEW: SourceFetcher trait
├── source_tests.rs           # NEW: Source tests
├── resource.rs               # NEW: ResourceScanner trait
├── resource_tests.rs         # NEW: Resource tests
├── executor.rs               # NEW: ActionExecutor
├── executor_tests.rs         # NEW: Executor tests
├── cleanup.rs                # NEW: CleanupExecutor
├── cleanup_tests.rs          # NEW: Cleanup tests
├── bridge.rs                 # NEW: WatcherEventBridge
├── bridge_tests.rs           # NEW: Bridge tests
└── CLAUDE.md                 # Update with new invariants
```

## 6. WAL Operations

Add operations for new execution tracking:

```rust
// In operation.rs
pub enum Operation {
    // ... existing ...

    // Action execution tracking
    ActionExecutionStarted(ActionExecutionStartedOp),
    ActionExecutionCompleted(ActionExecutionCompletedOp),

    // Scanner cleanup tracking
    CleanupExecuted(CleanupExecutedOp),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionStartedOp {
    pub action_id: String,
    pub source: String,
    pub execution_type: String, // "command", "task", "rules"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionCompletedOp {
    pub action_id: String,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupExecutedOp {
    pub scanner_id: String,
    pub resource_id: String,
    pub action: String, // "delete", "release", "archive", etc.
    pub success: bool,
}
```

## 7. Testing Strategy

### Unit Tests

Each new component gets comprehensive unit tests:

1. **CronController** - Test cron→watcher→scanner flow
2. **SourceFetcher** - Test each source type
3. **ActionExecutor** - Test command, task, rules execution
4. **WatcherEventBridge** - Test pattern matching and wake
5. **ResourceScanner** - Test each resource type
6. **CleanupExecutor** - Test each cleanup action

### Integration Tests

```rust
#[test]
fn full_watchdog_flow() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    // Set up watchdog cron with watchers
    engine.load_runbook("watchdog.toml");

    // Simulate idle session
    engine.fake_session_idle("agent-1", Duration::from_secs(360));

    // Tick cron
    clock.advance(Duration::from_secs(30));
    engine.tick_scheduling();

    // Verify nudge action was executed
    assert!(engine.fake_adapters().commands_run().contains(&"oj session nudge agent-1"));
}

#[test]
fn event_driven_watcher_wake() {
    let clock = FakeClock::new();
    let mut engine = Engine::new_with_fakes(&clock);

    // Set up watcher with wake_on
    engine.add_watcher(WatcherConfig {
        wake_on: vec!["task:failed".to_string()],
        ..Default::default()
    });

    // Emit task failed event
    engine.emit(Event::TaskFailed { id: "task-1".into(), reason: "error".into() });

    // Verify watcher was checked immediately (not waiting for timer)
    assert!(engine.scheduling_manager().get_watcher(&watcher_id).unwrap().last_check.is_some());
}
```

### Property Tests

```rust
proptest! {
    #[test]
    fn action_cooldown_respected_during_execution(
        triggers in 1..20u32,
        cooldown_secs in 1..60u64,
    ) {
        let clock = FakeClock::new();
        let mut executor = ActionExecutor::new_with_fakes();
        let action = Action::new(
            ActionId::new("test"),
            ActionConfig::new("test", Duration::from_secs(cooldown_secs)),
        );

        let mut successful = 0;
        for _ in 0..triggers {
            if executor.try_execute(&action, "test", &clock).is_ok() {
                successful += 1;
            }
            clock.advance(Duration::from_secs(cooldown_secs / 2));
        }

        // Only first trigger should succeed within cooldown period
        prop_assert!(successful <= 2);
    }
}
```

## 8. Migration Path

1. **Backward compatible** - Existing crons/watchers/scanners continue to work
2. **Opt-in integration** - New fields (watchers, wake_on, execution) are optional
3. **Gradual adoption** - Can enable integration features per-runbook

## 9. Landing Checklist

- [ ] All new code follows pure state machine pattern where applicable
- [ ] Adapters have fake implementations for testing
- [ ] FakeClock used for all time-dependent tests
- [ ] WAL operations added for execution tracking
- [ ] Events emitted for observability
- [ ] CLAUDE.md updated with new invariants
- [ ] Integration tests cover full flows
- [ ] Runbook TOML files work with new features
- [ ] `./checks/lint.sh` passes
- [ ] `make check` passes

## 10. Open Questions

1. **Template engine** - Use existing TemplateEngine for variable interpolation in commands?
2. **Timeout handling** - How to handle long-running action commands?
3. **Retry semantics** - Should action execution have its own retry logic?
4. **Event ordering** - How to ensure watcher wakes don't cause infinite loops?
5. **Resource ID format** - Standardize `{type}:{id}` format across all resources?

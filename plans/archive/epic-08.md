# Epic 8: Cron, Watchers & Scanners

Implement time-driven execution, resource monitoring, and cleanup for proactive system health.

## 1. Overview

This epic adds proactive health management capabilities through scheduled tasks, condition monitoring, and automated cleanup. The system will maintain itself without manual intervention by detecting stuck processes, cleaning stale resources, and analyzing failures.

Key components:

- **Cron primitive**: Named, enable/disable scheduled tasks running at fixed intervals
- **Action primitive**: Named operations with cooldown enforcement to prevent rapid-fire triggers
- **Watcher primitive**: Monitor conditions and trigger response chains when thresholds are crossed
- **Scanner primitive**: Find stale resources matching conditions and execute cleanup actions
- **Scheduler enhancements**: Unified event loop handling cron ticks, events, and health checks
- **System runbooks**: Pre-built Watchdog, Janitor, and Triager for common maintenance patterns

The architecture follows existing patterns: pure state machines with `(State, Vec<Effect>)` transitions, injectable `Clock` for deterministic testing, and WAL persistence for durability.

## 2. Project Structure

```
crates/core/src/
├── scheduling/                    # New module for time-driven primitives
│   ├── mod.rs                     # Module exports
│   ├── cron.rs                    # Cron state machine
│   ├── cron_tests.rs              # Cron unit tests
│   ├── action.rs                  # Action with cooldowns
│   ├── action_tests.rs            # Action unit tests
│   ├── watcher.rs                 # Watcher state machine
│   ├── watcher_tests.rs           # Watcher unit tests
│   ├── scanner.rs                 # Scanner state machine
│   ├── scanner_tests.rs           # Scanner unit tests
│   ├── manager.rs                 # SchedulingManager (unified interface)
│   ├── manager_tests.rs           # Manager integration tests
│   └── CLAUDE.md                  # Module invariants
├── engine/
│   ├── scheduler.rs               # Extend with cron scheduling
│   ├── scheduler_tests.rs         # Additional scheduler tests
│   ├── runtime.rs                 # Integrate scheduling primitives
│   └── CLAUDE.md                  # Update with scheduling invariants
├── runbooks/
│   ├── watchdog.toml              # Agent idle detection
│   ├── janitor.toml               # Stale resource cleanup
│   └── triager.toml               # Failure analysis
└── storage/
    └── wal/
        └── operation.rs           # Add scheduling operations
```

## 3. Dependencies

No new external dependencies required. The implementation uses:
- Existing `Clock` trait (`SystemClock`, `FakeClock`)
- Existing `Scheduler` binary heap
- Existing event system for watcher triggers
- WAL operations for persistence

## 4. Implementation Phases

### Phase 1: Cron Primitive

**Goal**: Define the Cron state machine with enable/disable and interval-based scheduling.

**Files**:
- `crates/core/src/scheduling/mod.rs`
- `crates/core/src/scheduling/cron.rs`
- `crates/core/src/scheduling/cron_tests.rs`
- `crates/core/src/scheduling/CLAUDE.md`
- `crates/core/src/storage/wal/operation.rs` (extend)

**Cron state machine**:
```rust
/// A named scheduled task that runs at fixed intervals
#[derive(Debug, Clone)]
pub struct Cron {
    pub id: CronId,
    pub name: String,
    pub interval: Duration,
    pub state: CronState,
    pub last_run: Option<Instant>,
    pub next_run: Option<Instant>,
    pub run_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronState {
    /// Cron is active and will fire on schedule
    Enabled,
    /// Cron is paused and will not fire
    Disabled,
    /// Cron is currently executing (prevents overlap)
    Running,
}

#[derive(Debug, Clone)]
pub enum CronEvent {
    /// Enable a disabled cron
    Enable,
    /// Disable an enabled cron
    Disable,
    /// Timer fired, start execution
    Tick,
    /// Execution completed successfully
    Complete,
    /// Execution failed
    Fail { error: String },
}

impl Cron {
    pub fn new(id: CronId, name: &str, interval: Duration, clock: &impl Clock) -> Self;

    /// Pure state transition returning new state and effects
    pub fn transition(
        &self,
        event: CronEvent,
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            (CronState::Disabled, CronEvent::Enable) => {
                let next_run = clock.now() + self.interval;
                let new_state = Cron {
                    state: CronState::Enabled,
                    next_run: Some(next_run),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: TimerId::Cron(self.id.clone()),
                        duration: self.interval,
                    },
                    Effect::Emit(Event::CronEnabled { id: self.id.clone() }),
                ];
                (new_state, effects)
            }
            (CronState::Enabled, CronEvent::Disable) => {
                let new_state = Cron {
                    state: CronState::Disabled,
                    next_run: None,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::CancelTimer { id: TimerId::Cron(self.id.clone()) },
                    Effect::Emit(Event::CronDisabled { id: self.id.clone() }),
                ];
                (new_state, effects)
            }
            (CronState::Enabled, CronEvent::Tick) => {
                let new_state = Cron {
                    state: CronState::Running,
                    last_run: Some(clock.now()),
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::CronTriggered { id: self.id.clone() }),
                ];
                (new_state, effects)
            }
            (CronState::Running, CronEvent::Complete) => {
                let next_run = clock.now() + self.interval;
                let new_state = Cron {
                    state: CronState::Enabled,
                    next_run: Some(next_run),
                    run_count: self.run_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: TimerId::Cron(self.id.clone()),
                        duration: self.interval,
                    },
                    Effect::Emit(Event::CronCompleted {
                        id: self.id.clone(),
                        run_count: new_state.run_count,
                    }),
                ];
                (new_state, effects)
            }
            // ... other transitions
            _ => (self.clone(), vec![]) // Invalid transitions are no-ops
        }
    }
}
```

**WAL operations** (add to `operation.rs`):
```rust
// Cron operations
CronCreate(CronCreateOp),
CronTransition(CronTransitionOp),
CronDelete(CronDeleteOp),

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronCreateOp {
    pub id: String,
    pub name: String,
    pub interval_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTransitionOp {
    pub id: String,
    pub event: String, // "enable", "disable", "tick", "complete", "fail"
    pub error: Option<String>,
}
```

**Milestone**: Cron state machine passes all transition tests with FakeClock.

---

### Phase 2: Actions & Cooldowns

**Goal**: Define named operations with cooldown enforcement to prevent rapid-fire execution.

**Files**:
- `crates/core/src/scheduling/action.rs`
- `crates/core/src/scheduling/action_tests.rs`
- `crates/core/src/storage/wal/operation.rs` (extend)

**Action state machine**:
```rust
/// A named operation with cooldown to prevent rapid execution
#[derive(Debug, Clone)]
pub struct Action {
    pub id: ActionId,
    pub name: String,
    pub cooldown: Duration,
    pub state: ActionState,
    pub last_executed: Option<Instant>,
    pub execution_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    /// Action is ready to execute
    Ready,
    /// Action is on cooldown, cannot execute
    Cooling { until: Instant },
    /// Action is currently executing
    Executing,
}

#[derive(Debug, Clone)]
pub enum ActionEvent {
    /// Attempt to trigger the action
    Trigger { source: String },
    /// Execution completed
    Complete,
    /// Execution failed
    Fail { error: String },
    /// Cooldown period elapsed
    CooldownExpired,
}

impl Action {
    pub fn new(id: ActionId, name: &str, cooldown: Duration) -> Self;

    /// Check if action can be triggered now
    pub fn can_trigger(&self, clock: &impl Clock) -> bool {
        matches!(self.state, ActionState::Ready)
    }

    /// Pure state transition
    pub fn transition(
        &self,
        event: ActionEvent,
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            (ActionState::Ready, ActionEvent::Trigger { source }) => {
                let new_state = Action {
                    state: ActionState::Executing,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::ActionTriggered {
                        id: self.id.clone(),
                        source,
                    }),
                ];
                (new_state, effects)
            }
            (ActionState::Executing, ActionEvent::Complete) => {
                let until = clock.now() + self.cooldown;
                let new_state = Action {
                    state: ActionState::Cooling { until },
                    last_executed: Some(clock.now()),
                    execution_count: self.execution_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: TimerId::ActionCooldown(self.id.clone()),
                        duration: self.cooldown,
                    },
                    Effect::Emit(Event::ActionCompleted { id: self.id.clone() }),
                ];
                (new_state, effects)
            }
            (ActionState::Cooling { .. }, ActionEvent::CooldownExpired) => {
                let new_state = Action {
                    state: ActionState::Ready,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::Emit(Event::ActionReady { id: self.id.clone() }),
                ];
                (new_state, effects)
            }
            (ActionState::Cooling { .. }, ActionEvent::Trigger { source }) => {
                // Rejected due to cooldown
                let effects = vec![
                    Effect::Emit(Event::ActionRejected {
                        id: self.id.clone(),
                        source,
                        reason: "cooldown".to_string(),
                    }),
                ];
                (self.clone(), effects)
            }
            _ => (self.clone(), vec![])
        }
    }
}
```

**Cooldown use cases**:
- Nudge action: 30s cooldown (don't spam the agent)
- Restart action: 5m cooldown (allow time for restart to take effect)
- Escalate action: 1h cooldown (human needs time to respond)
- Cleanup action: 10m cooldown (prevent rapid deletions)

**Milestone**: Actions respect cooldowns and emit appropriate events.

---

### Phase 3: Watchers

**Goal**: Implement condition monitoring with response chains.

**Files**:
- `crates/core/src/scheduling/watcher.rs`
- `crates/core/src/scheduling/watcher_tests.rs`
- `crates/core/src/storage/wal/operation.rs` (extend)

**Watcher state machine**:
```rust
/// Monitors a condition and triggers responses when thresholds are crossed
#[derive(Debug, Clone)]
pub struct Watcher {
    pub id: WatcherId,
    pub name: String,
    pub source: WatcherSource,
    pub condition: WatcherCondition,
    pub response_chain: Vec<WatcherResponse>,
    pub state: WatcherState,
    pub check_interval: Duration,
    pub consecutive_triggers: u32,
    pub last_check: Option<Instant>,
}

/// What the watcher monitors
#[derive(Debug, Clone)]
pub enum WatcherSource {
    /// Monitor a specific task's state
    Task { id: TaskId },
    /// Monitor a pipeline's progress
    Pipeline { id: PipelineId },
    /// Monitor a session's output
    Session { name: String },
    /// Monitor an event stream
    Events { pattern: String },
    /// Custom source with shell command
    Command { command: String },
}

/// When the watcher triggers
#[derive(Debug, Clone)]
pub enum WatcherCondition {
    /// Source hasn't produced output in duration
    Idle { threshold: Duration },
    /// Source matches a pattern
    Matches { pattern: String },
    /// Source value exceeds threshold
    Exceeds { threshold: u64 },
    /// Source has been in state for duration
    StuckInState { state: String, threshold: Duration },
    /// Consecutive check failures
    ConsecutiveFailures { count: u32 },
}

/// What happens when condition is met
#[derive(Debug, Clone)]
pub struct WatcherResponse {
    pub action: ActionId,
    pub delay: Duration,
    pub requires_previous_failure: bool,
}

#[derive(Debug, Clone)]
pub enum WatcherState {
    /// Actively monitoring
    Active,
    /// Condition met, executing response chain
    Triggered { response_index: usize },
    /// Paused monitoring
    Paused,
}

impl Watcher {
    /// Check the source and evaluate condition
    pub fn check(
        &self,
        source_value: &SourceValue,
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        let condition_met = self.evaluate_condition(source_value, clock);

        if condition_met {
            let consecutive = self.consecutive_triggers + 1;
            let response = self.response_chain.get(0);

            match response {
                Some(resp) => {
                    let new_state = Watcher {
                        state: WatcherState::Triggered { response_index: 0 },
                        consecutive_triggers: consecutive,
                        last_check: Some(clock.now()),
                        ..self.clone()
                    };
                    let effects = vec![
                        Effect::TriggerAction {
                            action_id: resp.action.clone(),
                            source: format!("watcher:{}", self.name),
                        },
                        Effect::Emit(Event::WatcherTriggered {
                            id: self.id.clone(),
                            consecutive: consecutive,
                        }),
                    ];
                    (new_state, effects)
                }
                None => (self.clone(), vec![])
            }
        } else {
            // Condition not met, reset consecutive count
            let new_state = Watcher {
                consecutive_triggers: 0,
                last_check: Some(clock.now()),
                ..self.clone()
            };
            (new_state, vec![])
        }
    }

    /// Response completed, advance to next in chain or return to active
    pub fn response_completed(
        &self,
        success: bool,
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        match &self.state {
            WatcherState::Triggered { response_index } => {
                let next_index = response_index + 1;

                if success {
                    // Success - return to active monitoring
                    let new_state = Watcher {
                        state: WatcherState::Active,
                        consecutive_triggers: 0,
                        ..self.clone()
                    };
                    (new_state, vec![Effect::Emit(Event::WatcherResolved {
                        id: self.id.clone(),
                    })])
                } else if let Some(resp) = self.response_chain.get(next_index) {
                    // Failed - try next response in chain
                    let new_state = Watcher {
                        state: WatcherState::Triggered { response_index: next_index },
                        ..self.clone()
                    };
                    let effects = vec![
                        Effect::SetTimer {
                            id: TimerId::WatcherResponse(self.id.clone()),
                            duration: resp.delay,
                        },
                    ];
                    (new_state, effects)
                } else {
                    // Chain exhausted - escalate
                    let new_state = Watcher {
                        state: WatcherState::Active,
                        ..self.clone()
                    };
                    (new_state, vec![Effect::Emit(Event::WatcherEscalated {
                        id: self.id.clone(),
                    })])
                }
            }
            _ => (self.clone(), vec![])
        }
    }
}
```

**Event-driven wake** (integrate with existing event system):
```rust
/// Watchers can subscribe to events instead of polling
pub struct WatcherSubscription {
    pub watcher_id: WatcherId,
    pub pattern: EventPattern,
}

impl Engine {
    fn handle_event(&mut self, event: &Event) {
        // Check if any watchers are subscribed to this event type
        for subscription in &self.watcher_subscriptions {
            if subscription.pattern.matches(event) {
                if let Some(watcher) = self.watchers.get(&subscription.watcher_id) {
                    let source_value = SourceValue::from_event(event);
                    let (new_watcher, effects) = watcher.check(&source_value, &self.clock);
                    self.watchers.insert(subscription.watcher_id.clone(), new_watcher);
                    self.execute_effects(effects);
                }
            }
        }
    }
}
```

**Milestone**: Watchers detect conditions and execute response chains with proper escalation.

---

### Phase 4: Scanners

**Goal**: Implement resource scanning with condition-based cleanup.

**Files**:
- `crates/core/src/scheduling/scanner.rs`
- `crates/core/src/scheduling/scanner_tests.rs`
- `crates/core/src/storage/wal/operation.rs` (extend)

**Scanner state machine**:
```rust
/// Scans for resources matching conditions and executes cleanup
#[derive(Debug, Clone)]
pub struct Scanner {
    pub id: ScannerId,
    pub name: String,
    pub source: ScannerSource,
    pub condition: ScannerCondition,
    pub cleanup_action: CleanupAction,
    pub state: ScannerState,
    pub scan_interval: Duration,
    pub last_scan: Option<Instant>,
    pub total_cleaned: u64,
}

/// What the scanner looks for
#[derive(Debug, Clone)]
pub enum ScannerSource {
    /// Scan locks in coordination
    Locks,
    /// Scan queue items
    Queue { name: String },
    /// Scan worktrees on disk
    Worktrees,
    /// Scan tasks
    Tasks,
    /// Scan pipelines
    Pipelines,
    /// Scan sessions
    Sessions,
}

/// Which resources to clean up
#[derive(Debug, Clone)]
pub enum ScannerCondition {
    /// Resource is stale (no heartbeat beyond threshold)
    Stale { threshold: Duration },
    /// Resource is in terminal state for too long
    TerminalFor { threshold: Duration },
    /// Resource matches pattern
    Matches { pattern: String },
    /// Resource has exceeded max attempts
    ExceededAttempts { max: u32 },
    /// Orphaned (no parent reference)
    Orphaned,
}

/// What to do with matching resources
#[derive(Debug, Clone)]
pub enum CleanupAction {
    /// Delete the resource
    Delete,
    /// Archive to a secondary location
    Archive { destination: String },
    /// Release (for locks/semaphores)
    Release,
    /// Fail (for queue items)
    Fail { reason: String },
    /// Move to dead letter queue
    DeadLetter,
    /// Custom action
    Custom { action_id: ActionId },
}

#[derive(Debug, Clone)]
pub enum ScannerState {
    /// Waiting for next scan interval
    Idle,
    /// Currently scanning
    Scanning,
    /// Executing cleanup
    Cleaning { items: Vec<ResourceId> },
}

impl Scanner {
    /// Execute a scan and return resources to clean
    pub fn scan(
        &self,
        resources: &[ResourceInfo],
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        let matching: Vec<ResourceId> = resources
            .iter()
            .filter(|r| self.matches_condition(r, clock))
            .map(|r| r.id.clone())
            .collect();

        if matching.is_empty() {
            let new_state = Scanner {
                state: ScannerState::Idle,
                last_scan: Some(clock.now()),
                ..self.clone()
            };
            (new_state, vec![Effect::SetTimer {
                id: TimerId::Scanner(self.id.clone()),
                duration: self.scan_interval,
            }])
        } else {
            let new_state = Scanner {
                state: ScannerState::Cleaning { items: matching.clone() },
                last_scan: Some(clock.now()),
                ..self.clone()
            };
            let effects = matching
                .iter()
                .map(|id| self.cleanup_effect(id))
                .chain(std::iter::once(Effect::Emit(Event::ScannerFound {
                    id: self.id.clone(),
                    count: matching.len() as u32,
                })))
                .collect();
            (new_state, effects)
        }
    }

    fn cleanup_effect(&self, resource_id: &ResourceId) -> Effect {
        match &self.cleanup_action {
            CleanupAction::Delete => Effect::DeleteResource {
                id: resource_id.clone(),
            },
            CleanupAction::Release => Effect::ReleaseResource {
                id: resource_id.clone(),
            },
            CleanupAction::Fail { reason } => Effect::FailResource {
                id: resource_id.clone(),
                reason: reason.clone(),
            },
            CleanupAction::DeadLetter => Effect::DeadLetterResource {
                id: resource_id.clone(),
            },
            CleanupAction::Custom { action_id } => Effect::TriggerAction {
                action_id: action_id.clone(),
                source: format!("scanner:{}:{}", self.name, resource_id),
            },
            CleanupAction::Archive { destination } => Effect::ArchiveResource {
                id: resource_id.clone(),
                destination: destination.clone(),
            },
        }
    }

    /// Cleanup completed, update stats
    pub fn cleanup_completed(
        &self,
        count: u64,
        clock: &impl Clock,
    ) -> (Self, Vec<Effect>) {
        let new_state = Scanner {
            state: ScannerState::Idle,
            total_cleaned: self.total_cleaned + count,
            ..self.clone()
        };
        let effects = vec![
            Effect::SetTimer {
                id: TimerId::Scanner(self.id.clone()),
                duration: self.scan_interval,
            },
            Effect::Emit(Event::ScannerCleaned {
                id: self.id.clone(),
                count,
                total: new_state.total_cleaned,
            }),
        ];
        (new_state, effects)
    }
}
```

**Milestone**: Scanners find and clean stale resources with proper metrics.

---

### Phase 5: System Runbooks & Integration

**Goal**: Create built-in runbooks for common maintenance patterns and integrate all primitives.

**Files**:
- `crates/core/src/scheduling/manager.rs`
- `crates/core/src/scheduling/manager_tests.rs`
- `crates/core/src/engine/runtime.rs` (update)
- `crates/core/src/engine/scheduler.rs` (update)
- `crates/core/src/runbooks/watchdog.toml`
- `crates/core/src/runbooks/janitor.toml`
- `crates/core/src/runbooks/triager.toml`

**SchedulingManager** (unified interface):
```rust
/// Unified manager for all scheduling primitives
pub struct SchedulingManager {
    pub crons: HashMap<CronId, Cron>,
    pub actions: HashMap<ActionId, Action>,
    pub watchers: HashMap<WatcherId, Watcher>,
    pub scanners: HashMap<ScannerId, Scanner>,
}

impl SchedulingManager {
    pub fn new() -> Self;

    // Cron operations
    pub fn add_cron(&mut self, cron: Cron);
    pub fn enable_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect>;
    pub fn disable_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect>;
    pub fn tick_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect>;

    // Action operations
    pub fn add_action(&mut self, action: Action);
    pub fn trigger_action(
        &mut self,
        id: &ActionId,
        source: &str,
        clock: &impl Clock,
    ) -> Vec<Effect>;
    pub fn complete_action(&mut self, id: &ActionId, clock: &impl Clock) -> Vec<Effect>;

    // Watcher operations
    pub fn add_watcher(&mut self, watcher: Watcher);
    pub fn check_watcher(
        &mut self,
        id: &WatcherId,
        source_value: &SourceValue,
        clock: &impl Clock,
    ) -> Vec<Effect>;

    // Scanner operations
    pub fn add_scanner(&mut self, scanner: Scanner);
    pub fn run_scanner(
        &mut self,
        id: &ScannerId,
        resources: &[ResourceInfo],
        clock: &impl Clock,
    ) -> Vec<Effect>;

    // Bulk operations
    pub fn tick(&mut self, clock: &impl Clock) -> Vec<Effect>;
    pub fn stats(&self) -> SchedulingStats;
}
```

**Engine integration**:
```rust
impl<A: Adapters, C: Clock> Engine<A, C> {
    /// Initialize scheduling primitives from runbooks
    pub fn init_scheduling(&mut self) -> Result<(), EngineError> {
        // Load watchdog runbook
        let watchdog_watcher = Watcher {
            id: WatcherId::new("watchdog"),
            name: "agent-watchdog".to_string(),
            source: WatcherSource::Session { name: "*".to_string() },
            condition: WatcherCondition::Idle {
                threshold: Duration::from_secs(300), // 5 min idle
            },
            response_chain: vec![
                WatcherResponse {
                    action: ActionId::new("nudge"),
                    delay: Duration::ZERO,
                    requires_previous_failure: false,
                },
                WatcherResponse {
                    action: ActionId::new("restart"),
                    delay: Duration::from_secs(120), // 2 min delay
                    requires_previous_failure: true,
                },
                WatcherResponse {
                    action: ActionId::new("escalate"),
                    delay: Duration::from_secs(300), // 5 min delay
                    requires_previous_failure: true,
                },
            ],
            state: WatcherState::Active,
            check_interval: Duration::from_secs(60),
            consecutive_triggers: 0,
            last_check: None,
        };
        self.scheduling.add_watcher(watchdog_watcher);

        // Load janitor scanners
        let lock_scanner = Scanner {
            id: ScannerId::new("stale-locks"),
            name: "stale-lock-cleanup".to_string(),
            source: ScannerSource::Locks,
            condition: ScannerCondition::Stale {
                threshold: Duration::from_secs(3600), // 1 hour
            },
            cleanup_action: CleanupAction::Release,
            state: ScannerState::Idle,
            scan_interval: Duration::from_secs(600), // 10 min
            last_scan: None,
            total_cleaned: 0,
        };
        self.scheduling.add_scanner(lock_scanner);

        // Add more scanners for queue, worktree cleanup...

        Ok(())
    }

    /// Main scheduling tick (called from event loop)
    pub fn tick_scheduling(&mut self) -> Vec<Effect> {
        self.scheduling.tick(&self.clock)
    }
}
```

**Watchdog runbook** (`watchdog.toml`):
```toml
[meta]
name = "watchdog"
description = "Agent idle detection with escalating responses"

[[actions]]
id = "nudge"
cooldown = "30s"
command = "send_to_session"
args = { message = "Are you still working? Please respond with your current status." }

[[actions]]
id = "restart"
cooldown = "5m"
command = "restart_session"

[[actions]]
id = "escalate"
cooldown = "1h"
command = "notify"
args = { level = "critical", message = "Agent unresponsive after restart" }

[[watchers]]
id = "agent-idle"
source = { type = "session", pattern = "*" }
condition = { type = "idle", threshold = "5m" }
responses = [
    { action = "nudge" },
    { action = "restart", delay = "2m", requires_previous_failure = true },
    { action = "escalate", delay = "5m", requires_previous_failure = true },
]
```

**Janitor runbook** (`janitor.toml`):
```toml
[meta]
name = "janitor"
description = "Stale resource cleanup"

[[scanners]]
id = "stale-locks"
source = { type = "locks" }
condition = { type = "stale", threshold = "1h" }
cleanup = { type = "release" }
interval = "10m"

[[scanners]]
id = "dead-queue-items"
source = { type = "queue", name = "merge" }
condition = { type = "exceeded_attempts", max = 3 }
cleanup = { type = "dead_letter" }
interval = "5m"

[[scanners]]
id = "orphan-worktrees"
source = { type = "worktrees" }
condition = { type = "orphaned" }
cleanup = { type = "delete" }
interval = "30m"

[[scanners]]
id = "old-completed-pipelines"
source = { type = "pipelines" }
condition = { type = "terminal_for", threshold = "24h" }
cleanup = { type = "archive", destination = ".oj/archive" }
interval = "1h"
```

**Triager runbook** (`triager.toml`):
```toml
[meta]
name = "triager"
description = "Failure analysis and decision rules"

[[watchers]]
id = "build-failures"
source = { type = "events", pattern = "pipeline:failed:*" }
condition = { type = "consecutive_failures", count = 3 }
responses = [
    { action = "notify-team", delay = "0s" },
    { action = "pause-pipelines", delay = "5m" },
]

[[watchers]]
id = "test-flakiness"
source = { type = "events", pattern = "task:failed:test:*" }
condition = { type = "matches", pattern = "flaky|timeout|intermittent" }
responses = [
    { action = "retry-with-isolation" },
    { action = "mark-flaky", delay = "30s", requires_previous_failure = true },
]

[[actions]]
id = "notify-team"
cooldown = "15m"
command = "notify"
args = { level = "warning", message = "Multiple consecutive build failures" }

[[actions]]
id = "pause-pipelines"
cooldown = "1h"
command = "disable_cron"
args = { pattern = "auto-*" }

[[actions]]
id = "retry-with-isolation"
cooldown = "5m"
command = "retry_task"
args = { isolation = true }

[[actions]]
id = "mark-flaky"
cooldown = "1h"
command = "tag_test"
args = { tag = "flaky" }
```

**Milestone**: All system runbooks loaded and integrated with engine event loop.

---

## 5. Key Implementation Details

### Timer ID Management

All scheduling primitives use typed timer IDs for proper cancellation:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimerId {
    // Existing
    TaskTick,
    QueueTick { queue_name: String },
    HeartbeatPoll,
    // New for Epic 8
    Cron(CronId),
    ActionCooldown(ActionId),
    WatcherCheck(WatcherId),
    WatcherResponse(WatcherId),
    Scanner(ScannerId),
}
```

### Event Integration

Watchers integrate with the existing event system for efficient wake:
```rust
// In effect.rs, extend Event enum
pub enum Event {
    // ... existing events

    // Cron events
    CronEnabled { id: CronId },
    CronDisabled { id: CronId },
    CronTriggered { id: CronId },
    CronCompleted { id: CronId, run_count: u64 },
    CronFailed { id: CronId, error: String },

    // Action events
    ActionTriggered { id: ActionId, source: String },
    ActionCompleted { id: ActionId },
    ActionRejected { id: ActionId, source: String, reason: String },
    ActionReady { id: ActionId },

    // Watcher events
    WatcherTriggered { id: WatcherId, consecutive: u32 },
    WatcherResolved { id: WatcherId },
    WatcherEscalated { id: WatcherId },

    // Scanner events
    ScannerFound { id: ScannerId, count: u32 },
    ScannerCleaned { id: ScannerId, count: u64, total: u64 },
}
```

### Recovery State Machine Integration

The Watcher response chain builds on existing recovery infrastructure:
```rust
// From recovery.rs - reuse the pattern
pub enum RecoveryAction {
    Nudge,
    Restart,
    Escalate,
}

// Watcher responses map to recovery actions
impl WatcherResponse {
    pub fn to_recovery_action(&self, actions: &HashMap<ActionId, Action>) -> Option<RecoveryAction> {
        let action_name = actions.get(&self.action)?.name.as_str();
        match action_name {
            "nudge" => Some(RecoveryAction::Nudge),
            "restart" => Some(RecoveryAction::Restart),
            "escalate" => Some(RecoveryAction::Escalate),
            _ => None,
        }
    }
}
```

### WAL Operations for Scheduling

All scheduling state changes are persisted:
```rust
// In operation.rs
pub enum Operation {
    // ... existing operations

    // Cron operations
    CronCreate(CronCreateOp),
    CronTransition(CronTransitionOp),
    CronDelete(CronDeleteOp),

    // Action operations
    ActionCreate(ActionCreateOp),
    ActionTransition(ActionTransitionOp),
    ActionDelete(ActionDeleteOp),

    // Watcher operations
    WatcherCreate(WatcherCreateOp),
    WatcherTransition(WatcherTransitionOp),
    WatcherDelete(WatcherDeleteOp),

    // Scanner operations
    ScannerCreate(ScannerCreateOp),
    ScannerTransition(ScannerTransitionOp),
    ScannerDelete(ScannerDeleteOp),
}
```

### Instant Field Handling

Follow the established pattern from coordination for Instant fields:
```rust
// From coordination/storage.rs - use same pattern
pub struct StorableCron {
    pub id: String,
    pub name: String,
    pub interval_secs: u64,
    pub state: String,
    pub last_run_age_micros: Option<u64>,  // Age-based, not absolute
    pub run_count: u64,
}

impl StorableCron {
    pub fn from_cron(cron: &Cron, clock: &impl Clock) -> Self {
        StorableCron {
            id: cron.id.0.clone(),
            name: cron.name.clone(),
            interval_secs: cron.interval.as_secs(),
            state: cron.state.to_string(),
            last_run_age_micros: cron.last_run.map(|t| {
                clock.now().duration_since(t).as_micros() as u64
            }),
            run_count: cron.run_count,
        }
    }

    pub fn to_cron(&self, clock: &impl Clock) -> Cron {
        Cron {
            id: CronId(self.id.clone()),
            name: self.name.clone(),
            interval: Duration::from_secs(self.interval_secs),
            state: CronState::from_str(&self.state),
            last_run: self.last_run_age_micros.map(|age| {
                clock.now() - Duration::from_micros(age)
            }),
            next_run: None, // Recalculated on load
            run_count: self.run_count,
        }
    }
}
```

## 6. Verification Plan

### Unit Tests

**Cron tests** (`cron_tests.rs`):
```rust
#[test]
fn cron_enable_schedules_timer() {
    let clock = FakeClock::new();
    let cron = Cron::new(CronId::new("test"), "test-cron", Duration::from_secs(60), &clock);

    let (cron, effects) = cron.transition(CronEvent::Enable, &clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
}

#[test]
fn cron_tick_while_disabled_is_noop() {
    let clock = FakeClock::new();
    let cron = Cron::new(CronId::new("test"), "test-cron", Duration::from_secs(60), &clock);

    let (cron, effects) = cron.transition(CronEvent::Tick, &clock);

    assert_eq!(cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn cron_reschedules_after_completion() {
    let clock = FakeClock::new();
    let mut cron = Cron::new(CronId::new("test"), "test-cron", Duration::from_secs(60), &clock);

    (cron, _) = cron.transition(CronEvent::Enable, &clock);
    (cron, _) = cron.transition(CronEvent::Tick, &clock);
    let (cron, effects) = cron.transition(CronEvent::Complete, &clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert_eq!(cron.run_count, 1);
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
}
```

**Action tests** (`action_tests.rs`):
```rust
#[test]
fn action_rejects_during_cooldown() {
    let clock = FakeClock::new();
    let mut action = Action::new(ActionId::new("test"), "test-action", Duration::from_secs(60));

    // First trigger succeeds
    (action, _) = action.transition(ActionEvent::Trigger { source: "test".into() }, &clock);
    (action, _) = action.transition(ActionEvent::Complete, &clock);

    // Second trigger during cooldown is rejected
    let (action, effects) = action.transition(ActionEvent::Trigger { source: "test".into() }, &clock);

    assert!(matches!(action.state, ActionState::Cooling { .. }));
    assert!(effects.iter().any(|e| matches!(e, Effect::Emit(Event::ActionRejected { .. }))));
}

#[test]
fn action_ready_after_cooldown() {
    let clock = FakeClock::new();
    let mut action = Action::new(ActionId::new("test"), "test-action", Duration::from_secs(60));

    (action, _) = action.transition(ActionEvent::Trigger { source: "test".into() }, &clock);
    (action, _) = action.transition(ActionEvent::Complete, &clock);

    clock.advance(Duration::from_secs(61));
    let (action, _) = action.transition(ActionEvent::CooldownExpired, &clock);

    assert_eq!(action.state, ActionState::Ready);
}
```

**Watcher tests** (`watcher_tests.rs`):
```rust
#[test]
fn watcher_triggers_on_condition() {
    let clock = FakeClock::new();
    let watcher = Watcher {
        id: WatcherId::new("test"),
        name: "test-watcher".into(),
        source: WatcherSource::Task { id: TaskId::new("task-1") },
        condition: WatcherCondition::Idle { threshold: Duration::from_secs(300) },
        response_chain: vec![
            WatcherResponse {
                action: ActionId::new("nudge"),
                delay: Duration::ZERO,
                requires_previous_failure: false,
            },
        ],
        state: WatcherState::Active,
        check_interval: Duration::from_secs(60),
        consecutive_triggers: 0,
        last_check: None,
    };

    let source_value = SourceValue::Idle { duration: Duration::from_secs(400) };
    let (watcher, effects) = watcher.check(&source_value, &clock);

    assert!(matches!(watcher.state, WatcherState::Triggered { .. }));
    assert!(effects.iter().any(|e| matches!(e, Effect::TriggerAction { .. })));
}

#[test]
fn watcher_escalates_through_response_chain() {
    let clock = FakeClock::new();
    let watcher = create_watcher_with_chain();

    // First response fails
    let (watcher, _) = watcher.response_completed(false, &clock);
    assert!(matches!(watcher.state, WatcherState::Triggered { response_index: 1 }));

    // Second response fails
    let (watcher, effects) = watcher.response_completed(false, &clock);

    // Chain exhausted, emits escalation
    assert!(effects.iter().any(|e| matches!(e, Effect::Emit(Event::WatcherEscalated { .. }))));
}
```

**Scanner tests** (`scanner_tests.rs`):
```rust
#[test]
fn scanner_finds_stale_locks() {
    let clock = FakeClock::new();
    let scanner = Scanner {
        id: ScannerId::new("test"),
        name: "test-scanner".into(),
        source: ScannerSource::Locks,
        condition: ScannerCondition::Stale { threshold: Duration::from_secs(3600) },
        cleanup_action: CleanupAction::Release,
        state: ScannerState::Idle,
        scan_interval: Duration::from_secs(600),
        last_scan: None,
        total_cleaned: 0,
    };

    let resources = vec![
        ResourceInfo {
            id: ResourceId::Lock("lock-1".into()),
            age: Duration::from_secs(7200), // Stale
        },
        ResourceInfo {
            id: ResourceId::Lock("lock-2".into()),
            age: Duration::from_secs(1800), // Fresh
        },
    ];

    let (scanner, effects) = scanner.scan(&resources, &clock);

    assert!(matches!(scanner.state, ScannerState::Cleaning { .. }));
    let release_count = effects.iter().filter(|e| matches!(e, Effect::ReleaseResource { .. })).count();
    assert_eq!(release_count, 1);
}
```

### Integration Tests

**Scheduler integration** (`scheduler_tests.rs`):
```rust
#[test]
fn scheduler_handles_cron_timers() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();
    let mut crons = HashMap::new();

    // Create and enable a cron
    let cron = Cron::new(CronId::new("test"), "test", Duration::from_secs(60), &clock);
    let (cron, effects) = cron.transition(CronEvent::Enable, &clock);
    crons.insert(cron.id.clone(), cron);

    // Execute effects (schedule timer)
    for effect in effects {
        if let Effect::SetTimer { id, duration } = effect {
            scheduler.schedule(id, duration, &clock);
        }
    }

    // Advance time and poll
    clock.advance(Duration::from_secs(61));
    let ready = scheduler.poll(clock.now());

    assert_eq!(ready.len(), 1);
    assert!(matches!(ready[0].kind, ScheduledKind::Timer { id: TimerId::Cron(_) }));
}
```

**Manager integration** (`manager_tests.rs`):
```rust
#[test]
fn manager_coordinates_watcher_and_action() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Add action with cooldown
    let action = Action::new(ActionId::new("nudge"), "nudge", Duration::from_secs(30));
    manager.add_action(action);

    // Add watcher using that action
    let watcher = create_watchdog_watcher();
    manager.add_watcher(watcher);

    // Trigger watcher
    let source_value = SourceValue::Idle { duration: Duration::from_secs(400) };
    let effects = manager.check_watcher(&WatcherId::new("watchdog"), &source_value, &clock);

    // Action should be triggered
    assert!(effects.iter().any(|e| matches!(e, Effect::TriggerAction { action_id, .. } if action_id.0 == "nudge")));
}
```

### Property Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn cron_state_machine_never_panics(events in vec(arb_cron_event(), 0..100)) {
        let clock = FakeClock::new();
        let mut cron = Cron::new(CronId::new("test"), "test", Duration::from_secs(60), &clock);

        for event in events {
            clock.advance(Duration::from_secs(1));
            let (new_cron, _) = cron.transition(event, &clock);
            cron = new_cron;
        }
    }

    #[test]
    fn action_cooldown_always_enforced(triggers in 1..100u32) {
        let clock = FakeClock::new();
        let mut action = Action::new(ActionId::new("test"), "test", Duration::from_secs(60));
        let mut successful_triggers = 0u32;

        for _ in 0..triggers {
            let (new_action, effects) = action.transition(
                ActionEvent::Trigger { source: "test".into() },
                &clock,
            );

            if effects.iter().any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { .. }))) {
                successful_triggers += 1;
                // Complete the action
                let (a, _) = new_action.transition(ActionEvent::Complete, &clock);
                action = a;
            } else {
                action = new_action;
            }

            clock.advance(Duration::from_secs(10)); // Less than cooldown
        }

        // Without cooldown expiry, only first trigger should succeed
        prop_assert_eq!(successful_triggers, 1);
    }
}
```

### Pre-commit Verification

Before each phase commit:
```bash
./checks/lint.sh
make check   # fmt, clippy, test, build, audit, deny
```

### Landing Checklist

- [ ] All new code follows pure state machine pattern
- [ ] All state transitions use `(State, Vec<Effect>)` return
- [ ] All time-dependent code accepts `&impl Clock`
- [ ] Tests use `FakeClock` for determinism
- [ ] WAL operations added for persistence
- [ ] Events emitted for observability
- [ ] No polling fallbacks (use event-driven wake)
- [ ] Integration with existing coordination/recovery
- [ ] System runbooks documented and tested

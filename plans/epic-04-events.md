# Epic 4: Events & Notifications

**Root Feature:** `oj-0eb1`

## Overview

Implement the events system for loose coupling and observability. Events are emitted on state changes and can wake workers, trigger notifications, and feed monitoring. This is foundational infrastructure that later epics (coordination, cron, watchers) build upon.

Currently, events are emitted via `Effect::Emit(Event)` but only logged via tracing (see `engine.rs:254-256`). This epic adds:
- **Event Bus** - Route events to matching subscribers using patterns
- **Worker Wake** - Subscribe workers to events instead of polling
- **Event Logging** - Structured audit trail of all events
- **Notifications** - macOS desktop alerts via osascript
- **NotifyAdapter** - Abstraction for notification delivery with fake implementation

**Key Changes from Epic 3:**
- Add `EventBus` struct to route events to subscribers
- Add `NotifyAdapter` trait and `OsascriptNotifier` implementation
- Modify `Engine` to route emitted events through the bus
- Replace worker polling with event-driven wake-on patterns
- Add structured event log storage

## Project Structure

```
crates/core/src/
├── lib.rs                          # Update exports

├── # Events System (NEW)
├── events/
│   ├── mod.rs                      # Module exports
│   ├── bus.rs                      # EventBus: pattern matching, subscribers
│   ├── subscription.rs             # Subscription: patterns, handlers
│   ├── log.rs                      # EventLog: audit trail storage
│   └── tests.rs                    # Unit tests

├── # Notifications (NEW)
├── adapters/
│   ├── notify.rs                   # NotifyAdapter trait + OsascriptNotifier
│   ├── fake.rs                     # ENHANCE: Add FakeNotifier

├── # Engine Integration (ENHANCE)
├── engine/
│   ├── engine.rs                   # ENHANCE: Route events through bus
│   ├── executor.rs                 # ENHANCE: Add Notify adapter to Adapters trait
│   ├── worker.rs                   # ENHANCE: Replace polling with wake_on

├── # Configuration (NEW)
├── config/
│   ├── mod.rs                      # Module exports
│   └── notify.rs                   # NotifyConfig: event → notification mapping

├── # Existing (minor updates)
├── effect.rs                       # Add Notify effect variant
├── storage/
│   └── json.rs                     # ENHANCE: Add event log persistence
```

## Dependencies

### Additions to Core Crate

```toml
[dependencies]
tokio = { version = "1", features = ["sync"] }  # Already present, need mpsc for bus
glob-match = "0.2"                              # Pattern matching for subscriptions

[dev-dependencies]
# Already have proptest, yare - no new test deps
```

## Implementation Phases

### Phase 1: Event Bus Core

**Goal**: Implement the event bus with pattern-based subscription routing.

**Deliverables**:
1. `EventBus` struct with subscriber registration
2. Pattern matching for event routing (supports `*` and `**` globs)
3. Async event delivery via channels
4. Thread-safe subscriber management
5. Unit tests for pattern matching

**Key Code**:

```rust
// events/subscription.rs

/// Pattern for matching events: exact, single wildcard (*), or category (**)
pub struct EventPattern(String);

impl EventPattern {
    pub fn matches(&self, event_name: &str) -> bool {
        // Split by ':', match segments: * = single segment, ** = all remaining
    }
}

pub struct SubscriberId(pub String);

pub struct Subscription {
    pub id: SubscriberId,
    pub patterns: Vec<EventPattern>,
    pub description: String,
}
```

```rust
// events/bus.rs

pub type EventSender = mpsc::UnboundedSender<Event>;
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<SubscriberId, (Subscription, EventSender)>>>,
    global_handler: Arc<RwLock<Option<EventSender>>>,
}

impl EventBus {
    pub fn subscribe(&self, subscription: Subscription) -> EventReceiver;
    pub fn unsubscribe(&self, id: &SubscriberId);
    pub fn set_global_handler(&self) -> EventReceiver;  // For logging
    pub fn publish(&self, event: Event);  // Send to global + matching subscribers
}
```

```rust
// effect.rs: Event::name() returns "category:action" format
// e.g., "workspace:created", "pipeline:complete", "task:stuck", "queue:item:added"
```

**Verification**:
- `cargo test events` passes all unit tests
- Pattern matching handles exact, wildcard, and category patterns
- Thread-safe concurrent subscribe/publish
- No message loss when publishing to multiple subscribers

---

### Phase 2: Event Logging & Audit Trail

**Goal**: Implement structured event logging for audit and debugging.

**Deliverables**:
1. `EventLog` struct for persisting events
2. Timestamped event records with sequence numbers
3. Query interface for event history
4. Integration with global handler
5. JSON storage for event log

**Key Code**:

```rust
// events/log.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub name: String,
    pub event: Event,
}

pub struct EventLog { path: PathBuf, sequence: u64, start_time: Instant }

impl EventLog {
    pub fn open(path: PathBuf) -> io::Result<Self>;  // Count existing lines for sequence
    pub fn append(&mut self, event: Event) -> io::Result<EventRecord>;  // JSON append
    pub fn read_all(&self) -> io::Result<Vec<EventRecord>>;
    pub fn query(&self, pattern: &EventPattern) -> io::Result<Vec<EventRecord>>;
    pub fn after(&self, sequence: u64) -> io::Result<Vec<EventRecord>>;
}
```

**Verification**:
- Event log persists to disk
- Sequence numbers are monotonic
- Query by pattern returns correct events
- Log survives process restart

---

### Phase 3: NotifyAdapter & macOS Integration

**Goal**: Implement notification system with macOS osascript integration.

**Deliverables**:
1. `NotifyAdapter` trait for notification abstraction
2. `OsascriptNotifier` implementation for macOS
3. `FakeNotifier` for testing
4. Notification types with optional sound
5. Integration tests with fake adapter

**Key Code**:

```rust
// adapters/notify.rs

pub enum NotifyUrgency { Normal, Important, Critical }

pub struct Notification {
    pub title: String,
    pub subtitle: Option<String>,
    pub message: String,
    pub urgency: NotifyUrgency,
}

#[async_trait]
pub trait NotifyAdapter: Clone + Send + Sync + 'static {
    async fn notify(&self, notification: Notification) -> Result<(), NotifyError>;
}

pub struct OsascriptNotifier { app_name: String }
// impl NotifyAdapter: builds AppleScript, runs osascript -e
// Adds sound for Important (default) and Critical (Sosumi)
```

```rust
// adapters/fake.rs (additions)

pub struct FakeNotifier {
    calls: Arc<Mutex<Vec<NotifyCall>>>,
    should_fail: Arc<Mutex<bool>>,
}

impl FakeNotifier {
    pub fn calls(&self) -> Vec<NotifyCall>;
    pub fn was_notified(&self, title: &str) -> bool;
    pub fn set_should_fail(&self, fail: bool);
}
```

**Verification**:
- OsascriptNotifier sends notifications on macOS
- FakeNotifier records all calls
- Escaping handles special characters
- Sound plays for important/critical notifications

---

### Phase 4: Notification Configuration

**Goal**: Configure which events become notifications and how.

**Deliverables**:
1. `NotifyConfig` struct for event → notification mapping
2. Default configuration for common events
3. Support for customizing notification content
4. Urgency mapping for escalations

**Key Code**:

```rust
// config/notify.rs

pub struct NotifyConfig { rules: Vec<NotifyRule> }

pub struct NotifyRule {
    pub pattern: EventPattern,
    pub urgency: NotifyUrgency,
    pub enabled: bool,
}

impl NotifyConfig {
    pub fn default_config() -> Self {
        // pipeline:complete → Normal, pipeline:failed → Important
        // task:stuck → Important, queue:item:deadlettered → Important
    }
    pub fn should_notify(&self, event: &Event) -> Option<NotifyUrgency>;
    pub fn to_notification(&self, event: &Event) -> Option<Notification>;
}

// event_to_notification: PipelineComplete/Failed, TaskStuck, DeadLettered → Notification
```

**Verification**:
- Default config includes sensible notification rules
- Custom rules can override defaults
- Disabled rules suppress notifications
- Event content reflected in notification text

---

### Phase 5: Engine Integration

**Goal**: Integrate event bus and notifications into the engine execution loop.

**Deliverables**:
1. Add `EventBus` to `Engine` struct
2. Route `Effect::Emit` through event bus
3. Add `NotifyAdapter` to `Adapters` trait
4. Notification handler subscribes to configured events
5. Event log subscriber for audit trail

**Key Code**:

```rust
// Adapters trait: add NotifyAdapter
pub trait Adapters: Clone + Send + Sync + 'static {
    type Notify: NotifyAdapter;
    fn notify(&self) -> Self::Notify;
}

// FakeAdapters: add FakeNotifier

// Engine additions:
pub struct Engine<A: Adapters, C: Clock> {
    // ... existing fields ...
    event_bus: EventBus,
    event_log: Option<EventLog>,
    notify_config: NotifyConfig,
}

impl Engine {
    pub fn with_event_log(self, path: impl Into<PathBuf>) -> io::Result<Self>;
    pub fn with_notify_config(self, config: NotifyConfig) -> Self;
    pub fn subscribe(&self, id: &str, patterns: Vec<&str>, description: &str) -> EventReceiver;

    async fn execute_effect(&mut self, effect: Effect) -> EffectResult {
        // Effect::Emit → log to EventLog, check NotifyConfig → notify, publish to EventBus
    }
}
```

**Verification**:
- Events route through bus to subscribers
- Event log captures all events
- Notifications sent for configured events
- FakeAdapters includes FakeNotifier
- Existing tests still pass

---

### Phase 6: Worker Wake-On Events

**Goal**: Replace worker polling with event-driven wake patterns.

**Deliverables**:
1. `WorkerConfig` with `wake_on` patterns
2. Modify `MergeWorker` to wake on `queue:item:added`
3. Support for multiple wake patterns
4. Fallback polling for reliability
5. Integration tests

**Key Code**:

```rust
// engine/worker.rs

pub struct WorkerConfig {
    pub id: String,
    pub queue_name: String,
    pub wake_on: Vec<EventPattern>,
    pub poll_interval: Duration,     // Default 30s fallback
    pub visibility_timeout: Duration,
}

pub struct EventDrivenWorker<A: Adapters> {
    config: WorkerConfig,
    adapters: A,
    store: JsonStore,
    event_rx: EventReceiver,
}

impl EventDrivenWorker {
    pub fn new(config: WorkerConfig, adapters: A, store: JsonStore, event_bus: &EventBus) -> Self;

    pub async fn run(&mut self) -> Result<()> {
        // tokio::select! on event_rx.recv() vs sleep(poll_interval)
        // On wake: process_available() until queue empty
    }
}

// Example: MergeWorker::new_event_driven with wake_on=["queue:item:added"]
```

**Verification**:
- Worker wakes immediately on matching event
- Fallback polling still works
- Multiple workers can subscribe to same events
- Worker processes all available items before sleeping

---

## Key Implementation Details

### Event Name Convention

Events follow a hierarchical naming scheme:

```
category:action          - e.g., "pipeline:complete"
category:subcategory:action - e.g., "queue:item:added"
```

Pattern matching supports:
- **Exact**: `"pipeline:complete"` matches only that event
- **Wildcard**: `"pipeline:*"` matches `pipeline:complete`, `pipeline:failed`, etc.
- **Category**: `"queue:**"` matches all queue events including `queue:item:*`

### Event Flow

```
State Machine Transition
    ↓ generates
Effect::Emit(Event)
    ↓ executed by
Engine::execute_effect()
    ↓ routes to
┌─────────────────────────────────────┐
│ 1. EventLog (audit trail)           │
│ 2. NotifyConfig → Notification      │
│ 3. EventBus → Subscribers           │
└─────────────────────────────────────┘
    ↓ wakes
Workers subscribed to matching patterns
```

### Thread Safety

The `EventBus` uses `Arc<RwLock<...>>` for thread-safe access:
- Multiple readers can subscribe concurrently
- Publishing takes a read lock (doesn't block other publishers)
- Subscribe/unsubscribe take write lock briefly

### Notification Urgency

| Urgency | Sound | Use Case |
|---------|-------|----------|
| Normal | None | Routine completion |
| Important | Default | Failures, stuck tasks |
| Critical | Alert | Escalation, needs intervention |

### Fallback Polling

Workers use event-driven wake with fallback polling for reliability:
- Primary: Wake on subscribed events (immediate)
- Fallback: Poll every N seconds (catches missed events, startup race)

This ensures work is never missed due to event delivery issues.

## Verification Plan

### Unit Tests

Run with: `cargo test --lib`

| Module | Key Tests |
|--------|-----------|
| `events::bus` | Subscribe, unsubscribe, publish, pattern matching |
| `events::subscription` | Pattern matching (exact, wildcard, category) |
| `events::log` | Append, read, query, persistence |
| `adapters::notify` | OsascriptNotifier script building, FakeNotifier recording |
| `config::notify` | Rule matching, notification conversion |

### Integration Tests

Run with: `cargo test --test`

| Test | Description |
|------|-------------|
| `event_routing` | Events from state machines reach subscribers |
| `notification_delivery` | Configured events trigger notifications |
| `worker_wake` | Workers wake on subscribed events |
| `event_log_audit` | All events captured in log |

### Property-Based Tests

- `pattern_matching_consistent`: matching is deterministic
- `event_names_are_valid`: names follow `category:action` format

### Manual Verification Checklist

- [ ] Events route to all matching subscribers
- [ ] EventLog persists across process restart
- [ ] macOS notifications appear with correct content
- [ ] Sound plays for Important/Critical notifications
- [ ] Workers wake immediately on matching events
- [ ] Fallback polling works when events missed
- [ ] FakeNotifier records all notification calls
- [ ] No regressions in Epic 3 functionality
- [ ] Pattern matching handles edge cases (`*`, `**`, empty)

### Test Commands

```bash
# All tests
cargo test

# Specific module
cargo test events

# Integration tests
cargo test --test event_integration

# With logging
RUST_LOG=debug cargo test -- --nocapture
```

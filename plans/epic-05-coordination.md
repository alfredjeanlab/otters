# Epic 5: Coordination Primitives

**Root Feature:** `oj-0185`

## Overview

Implement Lock, Semaphore, and Guard as first-class primitives with proper heartbeat-based stale detection. These enable safe concurrent access to shared resources like the main branch and agent slots.

Currently, the pipeline has a `Phase::Blocked` state with a `guard_id: Option<String>` placeholder (see `pipeline.rs`), but no actual coordination primitives exist. This epic adds:

- **Lock state machine** - Exclusive access with heartbeat refresh and automatic stale reclaim
- **Semaphore state machine** - Multi-holder resource limiting with weighted slots
- **Guard system** - Composable conditions that gate phase transitions
- **Event-driven wake** - Guards wait on events instead of polling
- **Coordination manager** - Unified interface for lock/semaphore operations
- **Phase gating** - Pre/post guards integrated into pipeline transitions

**Key Changes from Epic 4:**
- Add `coordination/` module with Lock, Semaphore, Guard state machines
- Add coordination-related Effect variants (AcquireLock, ReleaseLock, etc.)
- Add coordination Events (lock:acquired, semaphore:released, etc.)
- Integrate guards into Pipeline phase transitions
- Add `CoordinationManager` to Engine for unified coordination operations
- Add periodic maintenance task for stale resource reclaim

## Project Structure

```
crates/core/src/
├── lib.rs                          # Update exports

├── # Coordination System (NEW)
├── coordination/
│   ├── mod.rs                      # Module exports
│   ├── lock.rs                     # Lock state machine
│   ├── semaphore.rs                # Semaphore state machine
│   ├── guard.rs                    # Guard types and evaluation
│   ├── manager.rs                  # CoordinationManager
│   └── tests.rs                    # Unit tests

├── # Engine Integration (ENHANCE)
├── engine/
│   ├── engine.rs                   # ENHANCE: Add CoordinationManager
│   ├── executor.rs                 # ENHANCE: Execute coordination effects
│   ├── maintenance.rs              # NEW: Periodic stale reclaim task

├── # Existing (modifications)
├── effect.rs                       # ENHANCE: Add coordination effects & events
├── pipeline.rs                     # ENHANCE: Integrate guard evaluation
├── storage/
│   └── json.rs                     # ENHANCE: Persist locks & semaphores
```

## Dependencies

### Additions to Core Crate

```toml
[dependencies]
# Already present - no new dependencies needed
tokio = { version = "1", features = ["sync", "time"] }
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
# Already have proptest, yare - no new test deps
```

## Implementation Phases

### Phase 1: Lock State Machine

**Goal**: Implement Lock with heartbeat-based stale detection and automatic reclaim.

**Deliverables**:
1. `Lock` struct with Free/Held states
2. Heartbeat tracking with configurable staleness threshold
3. Pure transition function returning `(Lock, Vec<Effect>)`
4. Stale holder detection and automatic reclaim
5. Lock events (acquired, released, reclaimed)
6. Unit tests for all transitions

**Key Code**:

```rust
// coordination/lock.rs

pub struct HolderId(pub String);

pub struct LockConfig {
    pub name: String,
    pub stale_threshold: Duration,   // Default 60s
    pub heartbeat_interval: Duration, // Default 15s
}

pub enum LockState {
    Free,
    Held { holder: HolderId, metadata: Option<String>, last_heartbeat: Instant },
}

pub struct Lock { pub config: LockConfig, pub state: LockState }

pub enum LockEvent {
    Acquire { holder: HolderId, metadata: Option<String> },
    Release { holder: HolderId },
    Heartbeat { holder: HolderId },
    Tick,
}

impl Lock {
    pub fn is_free(&self) -> bool;
    pub fn is_held_by(&self, holder: &HolderId) -> bool;
    pub fn is_stale(&self, clock: &impl Clock) -> bool;

    pub fn transition(&self, event: LockEvent, clock: &impl Clock) -> (Lock, Vec<Effect>) {
        // Acquire+Free → Held, emit LockAcquired
        // Acquire+Held+stale → reclaim, emit LockReclaimed + LockAcquired
        // Acquire+Held+!stale → emit LockDenied
        // Release (matching holder) → Free, emit LockReleased
        // Heartbeat → refresh last_heartbeat
        // Tick+stale → emit LockStale warning
    }
}

// Events: LockAcquired, LockReleased, LockDenied, LockReclaimed, LockStale
```

**Verification**:
- `cargo test coordination::lock` passes all unit tests
- Lock acquisition succeeds when free
- Lock acquisition fails when held by another
- Stale locks can be reclaimed
- Heartbeat refreshes extend lock validity
- Events emitted correctly for all transitions

---

### Phase 2: Semaphore State Machine

**Goal**: Implement Semaphore with weighted slots for limiting concurrent operations.

**Deliverables**:
1. `Semaphore` struct with configurable slot count
2. Weighted acquisition (holder can take multiple slots)
3. Heartbeat tracking per holder
4. Orphan reclaim for stale holders
5. Semaphore events (acquired, released, reclaimed)
6. Unit tests including capacity edge cases

**Key Code**:

```rust
// coordination/semaphore.rs

pub struct SemaphoreConfig {
    pub name: String,
    pub max_slots: u32,
    pub stale_threshold: Duration,  // Default 60s
}

pub struct SemaphoreHolder {
    pub holder_id: String,
    pub weight: u32,
    pub metadata: Option<String>,
    pub last_heartbeat: Instant,
}

pub struct Semaphore {
    pub config: SemaphoreConfig,
    pub holders: HashMap<String, SemaphoreHolder>,
}

pub enum SemaphoreEvent {
    Acquire { holder_id: String, weight: u32, metadata: Option<String> },
    Release { holder_id: String },
    Heartbeat { holder_id: String },
    Tick,
}

impl Semaphore {
    pub fn used_slots(&self) -> u32;
    pub fn available_slots(&self) -> u32;
    pub fn can_acquire(&self, weight: u32) -> bool;
    pub fn stale_holders(&self, clock: &impl Clock) -> Vec<String>;

    pub fn transition(&self, event: SemaphoreEvent, clock: &impl Clock) -> (Semaphore, Vec<Effect>) {
        // Acquire: reclaim stale slots first, then grant if available → SemaphoreAcquired or SemaphoreDenied
        // Release: remove holder → SemaphoreReleased
        // Heartbeat: refresh last_heartbeat
        // Tick: emit SemaphoreStale warnings for stale holders
    }
}

// Events: SemaphoreAcquired, SemaphoreReleased, SemaphoreDenied, SemaphoreReclaimed, SemaphoreHolderStale
```

**Verification**:
- `cargo test coordination::semaphore` passes all unit tests
- Acquisition succeeds when slots available
- Acquisition fails when insufficient slots
- Weighted acquisition respects weight
- Stale holders are reclaimed on next acquire
- Multiple holders can hold slots concurrently

---

### Phase 3: Guard Conditions

**Goal**: Implement guard condition types and pure evaluation logic.

**Deliverables**:
1. `GuardCondition` enum with various condition types
2. `CompositeGuard` for All/Any/Not combinations
3. Pure evaluation function taking inputs, returning result
4. `GuardInput` struct for adapter-gathered data
5. Unit tests for each condition type

**Key Code**:

```rust
// coordination/guard.rs

pub enum GuardResult {
    Passed,
    Failed { reason: String },
    NeedsInput { input_type: GuardInputType },
}

pub enum GuardInputType {
    LockState { lock_name: String },
    SemaphoreState { semaphore_name: String },
    BranchExists { branch: String },
    BranchMerged { branch: String, into: String },
    IssueStatus { issue_id: String },
    IssuesForFilter { filter: String },
    FileExists { path: String },
    SessionAlive { session_name: String },
    CustomCheck { command: String },
}

pub struct GuardInputs {
    pub locks: HashMap<String, bool>,        // true = free
    pub semaphores: HashMap<String, u32>,    // available slots
    pub branches: HashMap<String, bool>,
    pub branch_merged: HashMap<(String, String), bool>,
    pub issues: HashMap<String, IssueStatus>,
    pub files: HashMap<String, bool>,
    pub sessions: HashMap<String, bool>,
    pub custom_checks: HashMap<String, bool>,
}

pub enum GuardCondition {
    LockFree { lock_name: String },
    LockHeldBy { lock_name: String, holder_id: String },
    SemaphoreAvailable { semaphore_name: String, weight: u32 },
    BranchExists { branch: String },
    BranchNotExists { branch: String },
    BranchMerged { branch: String, into: String },
    IssuesComplete { filter: String },
    IssueStatus { issue_id: String, expected: IssueStatus },
    FileExists { path: String },
    FileNotExists { path: String },
    SessionAlive { session_name: String },
    CustomCheck { command: String, description: String },
    All { conditions: Vec<GuardCondition> },
    Any { conditions: Vec<GuardCondition> },
    Not { condition: Box<GuardCondition> },
}

impl GuardCondition {
    /// Get all input types needed to evaluate this guard
    pub fn required_inputs(&self) -> Vec<GuardInputType> {
        // Match condition type → return corresponding GuardInputType
        // All/Any: collect from all child conditions
        // Not: delegate to inner condition
    }

    /// Evaluate the guard condition given the inputs (pure function)
    pub fn evaluate(&self, inputs: &GuardInputs) -> GuardResult {
        // For each condition type:
        // - Look up value in inputs HashMap
        // - Some(match) → Passed, Some(mismatch) → Failed{reason}, None → NeedsInput
        // All: short-circuit on first non-Passed
        // Any: short-circuit on first Passed, return last failure
        // Not: invert inner result
    }

    // Convenience constructors
    pub fn lock_free(name: impl Into<String>) -> Self;
    pub fn semaphore_available(name: impl Into<String>, weight: u32) -> Self;
    pub fn branch_exists(branch: impl Into<String>) -> Self;
    pub fn issues_complete(filter: impl Into<String>) -> Self;
    pub fn all(conditions: Vec<GuardCondition>) -> Self;
    pub fn any(conditions: Vec<GuardCondition>) -> Self;
    pub fn not(condition: GuardCondition) -> Self;
}
```

**Verification**:
- `cargo test coordination::guard` passes all unit tests
- Each condition type evaluates correctly
- Composite guards (All/Any/Not) work correctly
- `required_inputs()` returns correct input types
- Missing inputs return `NeedsInput` result

---

### Phase 4: Coordination Manager & Guard Executor

**Goal**: Implement the coordination manager that unifies lock/semaphore operations and executes guards.

**Deliverables**:
1. `CoordinationManager` struct managing locks, semaphores, guards
2. `GuardExecutor` that gathers inputs via adapters and evaluates
3. Integration with adapters for input gathering
4. Periodic maintenance interface for stale reclaim
5. Integration tests with FakeAdapters

**Key Code**:

```rust
// coordination/manager.rs

/// Manages all coordination primitives
#[derive(Clone, Debug)]
pub struct CoordinationManager {
    locks: HashMap<String, Lock>,
    semaphores: HashMap<String, Semaphore>,
    guards: HashMap<String, RegisteredGuard>,
}

#[derive(Clone, Debug)]
pub struct RegisteredGuard {
    pub id: String,
    pub condition: GuardCondition,
    pub wake_on: Vec<String>,
}

impl CoordinationManager {
    pub fn new() -> Self;

    // Lock operations - delegate to Lock state machine
    pub fn ensure_lock(&mut self, config: LockConfig) -> &Lock;
    pub fn get_lock(&self, name: &str) -> Option<&Lock>;
    pub fn acquire_lock(&mut self, name: &str, holder: HolderId, metadata: Option<String>, clock: &impl Clock) -> (bool, Vec<Effect>);
    pub fn release_lock(&mut self, name: &str, holder: HolderId, clock: &impl Clock) -> Vec<Effect>;
    pub fn heartbeat_lock(&mut self, name: &str, holder: HolderId, clock: &impl Clock);

    // Semaphore operations - delegate to Semaphore state machine
    pub fn ensure_semaphore(&mut self, config: SemaphoreConfig) -> &Semaphore;
    pub fn get_semaphore(&self, name: &str) -> Option<&Semaphore>;
    pub fn acquire_semaphore(&mut self, name: &str, holder_id: String, weight: u32, metadata: Option<String>, clock: &impl Clock) -> (bool, Vec<Effect>);
    pub fn release_semaphore(&mut self, name: &str, holder_id: String, clock: &impl Clock) -> Vec<Effect>;
    pub fn heartbeat_semaphore(&mut self, name: &str, holder_id: String, clock: &impl Clock);

    // Guard operations
    pub fn register_guard(&mut self, guard: RegisteredGuard);
    pub fn get_guard(&self, id: &str) -> Option<&RegisteredGuard>;
    pub fn unregister_guard(&mut self, id: &str);
    pub fn build_coordination_inputs(&self) -> GuardInputs; // Populate locks/semaphores state

    // Maintenance
    pub fn tick(&mut self, clock: &impl Clock) -> Vec<Effect>; // Check for stale holders
    pub fn reclaim_stale(&mut self, clock: &impl Clock) -> Vec<Effect>; // Force-release stale
    pub fn lock_names(&self) -> Vec<String>;
    pub fn semaphore_names(&self) -> Vec<String>;
}

/// Executes guards by gathering inputs from adapters
pub struct GuardExecutor<'a, S: SessionAdapter, R: RepoAdapter, I: IssueAdapter> {
    coordination: &'a CoordinationManager,
    sessions: &'a S,
    repos: &'a R,
    issues: &'a I,
}

impl<'a, S, R, I> GuardExecutor<'a, S, R, I> {
    pub fn new(coordination: &'a CoordinationManager, sessions: &'a S, repos: &'a R, issues: &'a I) -> Self;

    pub async fn evaluate(&self, condition: &GuardCondition) -> GuardResult {
        // 1. Build inputs from coordination state
        // 2. Gather additional inputs via adapters based on required_inputs()
        // 3. Call condition.evaluate(&inputs)
    }

    async fn gather_input(&self, inputs: &mut GuardInputs, input_type: &GuardInputType) {
        // Match input_type → call appropriate adapter method → insert into inputs
        // BranchExists → repos.branch_exists()
        // IssueStatus → issues.get() → map status string to IssueStatus enum
        // SessionAlive → sessions.is_alive()
        // CustomCheck → run shell command, check exit status
    }
}
```

**Verification**:
- `cargo test coordination::manager` passes all unit tests
- Lock/semaphore operations work through manager
- GuardExecutor gathers inputs correctly
- Stale reclaim releases resources
- Integration tests with FakeAdapters pass

---

### Phase 5: Pipeline Integration & Phase Gating

**Goal**: Integrate guards into pipeline phase transitions.

**Deliverables**:
1. `PhaseGuard` struct for pre/post guards on phases
2. Modify `Pipeline` to check guards before phase transitions
3. Event-driven guard wake (subscribe to relevant events)
4. Guard failure blocks pipeline with reason
5. Guard pass resumes pipeline
6. Integration tests for gated pipelines

**Key Code**:

```rust
// pipeline.rs (modifications)

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhaseGuards {
    pub pre: Option<GuardCondition>,
    pub post: Option<GuardCondition>,
    pub wake_on: Vec<String>,
}

impl PhaseGuards {
    pub fn new() -> Self;
    pub fn with_pre(self, condition: GuardCondition) -> Self;
    pub fn with_post(self, condition: GuardCondition) -> Self;
    pub fn with_wake_on(self, patterns: Vec<String>) -> Self;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuardedPhase { pub name: String, pub guards: PhaseGuards }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardType { Pre, Post }

// Extend Phase enum with guard-aware Blocked state
pub enum Phase {
    // ... existing phases ...
    Blocked { waiting_on: String, guard_id: Option<String>, guard_condition: Option<GuardCondition>, wake_on: Vec<String> },
}

impl Pipeline {
    pub fn can_proceed_with_guard(&self, result: &GuardResult) -> bool;
    pub fn apply_guard_result(&self, guard_type: GuardType, result: GuardResult, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        // Passed: emit PipelineResumed if Blocked, allow phase transition
        // Failed: set phase to Blocked, emit PipelineBlocked
        // NeedsInput: should not happen after execution
    }
}
```

```rust
// effect.rs (guard-related events)
pub enum Event {
    // ... existing events ...
    GuardEvaluating { guard_id: String, pipeline_id: String },
    GuardPassed { guard_id: String, pipeline_id: String },
    GuardFailed { guard_id: String, pipeline_id: String, reason: String },
}
// Event::name() returns "guard:evaluating", "guard:passed", "guard:failed"
```

```rust
// engine/engine.rs (guard integration)

impl<A: Adapters, C: Clock> Engine<A, C> {
    pub async fn evaluate_phase_guard(&self, pipeline_id: &str, guard: &GuardCondition) -> GuardResult {
        // Create GuardExecutor with coordination + adapters, call evaluate()
    }

    pub fn setup_guard_wake(&self, pipeline_id: &str, wake_patterns: &[String]) -> EventReceiver {
        // Create Subscription with patterns, subscribe to event_bus
    }
}
```

**Verification**:
- Pipeline blocks when pre-guard fails
- Pipeline resumes when guard passes
- Guard wake subscriptions work
- Events emitted for guard state changes
- Integration tests for gated phase transitions

---

### Phase 6: Periodic Maintenance & Storage

**Goal**: Implement background maintenance task and persistence for coordination state.

**Deliverables**:
1. Maintenance task that runs periodically
2. JSON storage for locks and semaphores
3. Recovery of coordination state on startup
4. Stale reclaim during maintenance
5. Heartbeat refresh from active holders

**Key Code**:

```rust
// engine/maintenance.rs

#[derive(Clone, Debug)]
pub struct MaintenanceConfig {
    pub interval: Duration,        // Default 30s
    pub reclaim_stale: bool,       // Default true
}

pub struct MaintenanceTask<C: Clock> {
    config: MaintenanceConfig,
    clock: C,
    effect_tx: mpsc::Sender<Vec<Effect>>,
}

impl<C: Clock> MaintenanceTask<C> {
    pub fn new(config: MaintenanceConfig, clock: C, effect_tx: mpsc::Sender<Vec<Effect>>) -> Self;

    pub async fn tick(&self, coordination: &mut CoordinationManager) {
        // coordination.tick() → check for stale holders
        // coordination.reclaim_stale() → force-release stale resources
        // Send collected effects via effect_tx
    }
}
```

```rust
// storage/json.rs (additions)

impl JsonStore {
    pub fn load_locks(&self) -> io::Result<HashMap<String, Lock>>;     // Read locks.json
    pub fn save_locks(&self, locks: &HashMap<String, Lock>) -> io::Result<()>;
    pub fn load_semaphores(&self) -> io::Result<HashMap<String, Semaphore>>; // Read semaphores.json
    pub fn save_semaphores(&self, semaphores: &HashMap<String, Semaphore>) -> io::Result<()>;
    pub fn load_coordination(&self) -> io::Result<CoordinationManager>; // Rebuild manager from files
    pub fn save_coordination(&self, manager: &CoordinationManager) -> io::Result<()>;
}
```

**Verification**:
- Maintenance task runs at configured interval
- Stale resources are reclaimed
- Coordination state persists across restarts
- No state corruption under concurrent access
- Integration tests for persistence

---

## Key Implementation Details

### State Machine Pattern

All coordination primitives follow the pure functional pattern:

```rust
impl Primitive {
    pub fn transition(&self, event: Event, clock: &impl Clock) -> (Self, Vec<Effect>) {
        // Pure logic, no side effects
        // Returns new state + effects to execute
    }
}
```

### Heartbeat-Based Stale Detection

```
Holder acquires lock
    ↓ periodically sends
Heartbeat (refresh last_heartbeat timestamp)
    ↓ maintenance task checks
If now - last_heartbeat > stale_threshold
    ↓
Mark as stale, allow reclaim by new holder
```

### Event-Driven Guard Wake

Instead of polling, guards subscribe to relevant events:

```
Guard waiting for lock:free
    ↓ subscribes to
"lock:released" event pattern
    ↓ event published when lock released
Guard re-evaluates, passes, pipeline resumes
```

### Guard Composition

```rust
// All must pass (AND)
GuardCondition::all(vec![
    GuardCondition::lock_free("main-branch"),
    GuardCondition::semaphore_available("agents", 1),
])

// Any must pass (OR)
GuardCondition::any(vec![
    GuardCondition::branch_exists("feature/x"),
    GuardCondition::branch_exists("feature/y"),
])

// Negation (NOT)
GuardCondition::not(GuardCondition::file_exists(".lock"))
```

### Coordination Flow

```
Pipeline phase transition
    ↓ check pre-guard
GuardExecutor gathers inputs
    ↓
Pure guard evaluation
    ↓ if passed
Proceed with phase
    ↓ if failed
Block pipeline, subscribe to wake events
    ↓ wake event received
Re-evaluate guard
    ↓ repeat until passed
Resume pipeline
```

### Thread Safety

- `CoordinationManager` uses interior mutability for concurrent access
- Lock/semaphore state machines are `Clone + Send + Sync`
- Event bus handles concurrent publish/subscribe
- Storage operations are atomic (write temp file, rename)

## Verification Plan

### Unit Tests

Run with: `cargo test --lib`

| Module | Key Tests |
|--------|-----------|
| `coordination::lock` | Acquire, release, heartbeat, stale detection, reclaim |
| `coordination::semaphore` | Acquire with weight, release, capacity limits, stale holders |
| `coordination::guard` | Each condition type, composite guards (All/Any/Not) |
| `coordination::manager` | Unified operations, guard execution, maintenance |

### Integration Tests

Run with: `cargo test --test`

| Test | Description |
|------|-------------|
| `lock_lifecycle` | Full lock acquire/heartbeat/release cycle |
| `semaphore_capacity` | Multiple holders up to capacity |
| `guard_evaluation` | Guards with adapter-gathered inputs |
| `pipeline_gating` | Pipeline blocked/resumed on guard state |
| `stale_reclaim` | Stale holders reclaimed during maintenance |
| `event_driven_wake` | Guards wake on relevant events |

### Property-Based Tests

```rust
proptest! {
    // lock_transitions_preserve_invariants: Lock always Free or Held after any event sequence
    // semaphore_never_exceeds_capacity: used_slots() <= max_slots after any event sequence
    // guard_evaluation_deterministic: same inputs → same result
}
```

### Manual Verification Checklist

- [ ] Lock acquire/release works correctly
- [ ] Lock heartbeat extends validity
- [ ] Stale locks can be reclaimed
- [ ] Semaphore respects capacity limits
- [ ] Semaphore weighted acquisition works
- [ ] Guard conditions evaluate correctly
- [ ] Composite guards (All/Any/Not) work
- [ ] Pipeline blocks on failed pre-guard
- [ ] Pipeline resumes when guard passes
- [ ] Event-driven wake triggers re-evaluation
- [ ] Maintenance task reclaims stale resources
- [ ] Coordination state persists to disk
- [ ] Recovery works after restart
- [ ] No regressions in Epic 4 functionality

### Test Commands

```bash
# All tests
cargo test

# Coordination module only
cargo test coordination

# Integration tests
cargo test --test coordination_integration

# With logging
RUST_LOG=debug cargo test -- --nocapture

# Property-based tests (more iterations)
PROPTEST_CASES=1000 cargo test proptest
```

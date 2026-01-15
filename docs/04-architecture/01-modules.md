# Module Structure

This document defines module boundaries, dependencies, and interfaces.

## Dependency Rules

Strict layering ensures testability and prevents circular dependencies:

```
                    ┌─────────────────────┐
                    │        cli          │  Layer 4: Entry points
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │       engine        │  Layer 3: Orchestration
                    └──────────┬──────────┘
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
┌─────────▼─────────┐ ┌────────▼────────┐ ┌─────────▼───────┐
│     adapters      │ │     storage     │ │     runbook     │  Layer 2
└───────────────────┘ └─────────────────┘ └─────────────────┘
          │                    │                    │
          └────────────────────┼────────────────────┘
                               │
                    ┌──────────▼──────────┐
                    │        core         │  Layer 1: Pure logic
                    └─────────────────────┘
```

**Rules:**
- Higher layers may depend on lower layers
- Same layer modules may NOT depend on each other (prevents cycles)
- `core` has ZERO external dependencies (only std)
- `adapters` may use external crates (process, tokio, etc.)

## Module: `core`

Pure business logic with no I/O. Every function is deterministic.

```rust
// core/lib.rs - Public API
pub mod pipeline;   // Pipeline state machine
pub mod queue;      // Queue data structures
pub mod task;       // Task state machine
pub mod lock;       // Lock semantics
pub mod semaphore;  // Semaphore semantics
pub mod guard;      // Guard evaluation
pub mod strategy;   // Strategy evaluation
pub mod event;      // Event types
pub mod effect;     // Effect types
pub mod clock;      // Time abstraction (injectable)
pub mod id;         // ID generation (injectable)
```

### Key Design: Injectable Dependencies

Even `core` needs time and IDs, but these must be injectable for testing:

```rust
pub trait Clock: Clone {
    fn now(&self) -> Instant;
}

pub trait IdGen: Clone {
    fn next(&self) -> Id;
}
```

Production uses `SystemClock` and `UuidGen`; tests use `FakeClock` (with `advance()`) and `SequentialGen` for deterministic behavior.

### Core Module Interfaces

Each module exposes a consistent pattern:

```rust
// pipeline.rs
pub struct Pipeline { /* immutable state */ }
pub enum PipelineEvent { PhaseComplete, PhaseFailed(Error), Timeout, ... }

impl Pipeline {
    pub fn new(config: PipelineConfig, clock: impl Clock, ids: impl IdGen) -> Self;

    /// Pure state transition
    pub fn transition(&self, event: PipelineEvent) -> (Pipeline, Vec<Effect>);

    /// Query current state
    pub fn phase(&self) -> Phase;
    pub fn is_blocked(&self) -> bool;
    pub fn elapsed(&self) -> Duration;
}
```

## Module: `engine`

Orchestrates state machines, executes effects, manages recovery.

```rust
// engine/lib.rs
pub mod executor;    // Effect execution loop
pub mod scheduler;   // Task scheduling
pub mod recovery;    // Recovery handling
pub mod context;     // Execution context
```

### Executor Design

The executor is the imperative shell that drives the functional core:

```rust
// executor.rs
pub struct Executor<A: Adapters> {
    adapters: A,
    store: Store,
    event_bus: EventBus,
}

impl<A: Adapters> Executor<A> {
    /// Main execution loop
    pub async fn run(&mut self, command: Command) -> Result<()> {
        // 1. Load current state
        let state = self.store.load()?;

        // 2. Compute new state and effects (pure)
        let (new_state, effects) = state.handle(command);

        // 3. Execute effects (impure)
        for effect in effects {
            self.execute_effect(effect).await?;
        }

        // 4. Persist new state
        self.store.save(&new_state)?;

        Ok(())
    }

    /// Effect failures feed back as events to the state machine
    async fn execute_effect(&mut self, state: &mut State, effect: Effect) {
        match effect {
            Effect::Emit(event) => self.event_bus.emit(event),
            Effect::Spawn { task_id, cmd, .. } => {
                let event = match self.adapters.sessions().spawn(&cmd).await {
                    Ok(session) => TaskEvent::SessionSpawned { session },
                    Err(e) => TaskEvent::SpawnFailed { reason: e.to_string() },
                };
                let (new_task, effects) = state.task(&task_id).transition(event);
                // Apply new state, recurse on new effects...
            }
            // ...
        }
    }
}
```

### Scheduler Design

The scheduler runs an event loop handling:
- **Cron ticks** - Run due cron jobs on schedule
- **Events** - Wake workers subscribed to specific events
- **Health checks** - Periodic worker health monitoring

## Module: `adapters`

Trait definitions and implementations for external systems.

```rust
// adapters/lib.rs
pub mod traits;     // Trait definitions
pub mod tmux;       // Tmux implementation
pub mod git;        // Git implementation
pub mod claude;     // Claude Code implementation
pub mod wk;         // Issue tracker implementation
pub mod fs;         // Filesystem implementation
pub mod notify;     // Notification implementation

// Aggregate trait for convenience
pub trait Adapters: Clone {
    type Sessions: SessionAdapter;
    type Repos: RepoAdapter;
    type Agent: AgentAdapter;
    type Issues: IssueAdapter;
    type Files: FileAdapter;
    type Notify: NotifyAdapter;

    fn sessions(&self) -> &Self::Sessions;
    fn repos(&self) -> &Self::Repos;
    fn agent(&self) -> &Self::Agent;
    fn issues(&self) -> &Self::Issues;
    fn files(&self) -> &Self::Files;
    fn notify(&self) -> &Self::Notify;
}
```

### Trait Design Pattern

Each adapter trait follows the same pattern:

```rust
// traits.rs

/// Session management (tmux, etc.)
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync {
    async fn spawn(&self, workspace: &Workspace, cmd: &str) -> Result<SessionId>;
    async fn send(&self, id: SessionId, input: &str) -> Result<()>;
    async fn kill(&self, id: SessionId) -> Result<()>;
    async fn is_alive(&self, id: SessionId) -> Result<bool>;
    async fn output(&self, id: SessionId, since: Option<Instant>) -> Result<String>;
}

/// Git operations
#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<()>;
    async fn worktree_remove(&self, path: &Path) -> Result<()>;
    async fn checkout(&self, branch: &str) -> Result<()>;
    async fn merge(&self, branch: &str, strategy: MergeStrategy) -> Result<MergeResult>;
    async fn is_clean(&self, path: &Path) -> Result<bool>;
    async fn branch_exists(&self, branch: &str) -> Result<bool>;
}

/// Agent invocation (Claude Code)
#[async_trait]
pub trait AgentAdapter: Clone + Send + Sync {
    async fn invoke(&self, session: SessionId, prompt: &str) -> Result<()>;
    async fn heartbeat(&self, session: SessionId) -> Result<HeartbeatStatus>;
    async fn nudge(&self, session: SessionId) -> Result<()>;
}

/// Issue tracking (wk)
#[async_trait]
pub trait IssueAdapter: Clone + Send + Sync {
    async fn list(&self, filter: IssueFilter) -> Result<Vec<Issue>>;
    async fn get(&self, id: IssueId) -> Result<Issue>;
    async fn start(&self, id: IssueId) -> Result<()>;
    async fn done(&self, id: IssueId) -> Result<()>;
    async fn note(&self, id: IssueId, content: &str) -> Result<()>;
}
```

## Module: `storage`

Persistence and synchronization.

```rust
// storage/lib.rs
pub mod wal;        // Write-ahead log
pub mod state;      // State serialization
pub mod sync;       // Multi-machine sync
pub mod snapshot;   // State snapshots
```

### WAL Design

The WAL is append-only for reliability. Each entry contains a sequence number, timestamp, machine ID, and operation. State is rebuilt by replaying the log. Key operations:

- `append(op)` - Durably write operation, return sequence number
- `replay()` - Rebuild state from log
- `since(seq)` - Get entries since sequence (for sync)

## Module: `runbook`

Runbook parsing and validation.

```rust
// runbook/lib.rs
pub mod parser;     // TOML parsing
pub mod validator;  // Semantic validation
pub mod loader;     // Dynamic loading
pub mod template;   // Prompt templates
```

### Parser Design

Separates parsing from validation:

```rust
// parser.rs
/// Raw parsed structure (may be invalid)
pub struct RawRunbook {
    pub commands: HashMap<String, RawCommand>,
    pub workers: HashMap<String, RawWorker>,
    pub pipelines: HashMap<String, RawPipeline>,
    // ...
}

pub fn parse(content: &str) -> Result<RawRunbook, ParseError>;

// validator.rs
/// Validated runbook (guaranteed valid)
pub struct Runbook { /* ... */ }

pub fn validate(raw: RawRunbook) -> Result<Runbook, Vec<ValidationError>>;
```

## Module: `cli`

Command-line interface.

```rust
// cli/lib.rs
pub mod commands;   // Command handlers
pub mod output;     // Output formatting
pub mod args;       // Argument parsing
```

### Command Handler Pattern

Each command is a pure function from args to request:

```rust
// commands/pipeline.rs
pub fn handle_pipeline_cmd(args: PipelineArgs) -> Result<Request> {
    match args.subcmd {
        PipelineSubcmd::List { filter } => {
            Ok(Request::PipelineList { filter })
        }
        PipelineSubcmd::Show { id } => {
            Ok(Request::PipelineShow { id })
        }
        PipelineSubcmd::Transition { id, phase } => {
            Ok(Request::PipelineTransition { id, phase })
        }
    }
}
```

The engine then handles the request, keeping CLI thin.

## Inter-Module Communication

Modules communicate through well-defined interfaces:

```
┌─────────────────────────────────────────────────────────┐
│                         CLI                              │
│                                                          │
│  Args ──parse──▶ Request ──▶ [passes to engine]         │
└──────────────────────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│                       Engine                              │
│                                                           │
│  Request + State ──▶ Core ──▶ (NewState, Effects)        │
│                                       │                   │
│  for effect in effects:               │                   │
│      adapters.execute(effect) ◀───────┘                  │
│      storage.persist(new_state)                          │
└──────────────────────────────────────────────────────────┘
                         │
           ┌─────────────┼─────────────┐
           ▼             ▼             ▼
      ┌─────────┐  ┌─────────┐  ┌─────────┐
      │Adapters │  │ Storage │  │ Runbook │
      └─────────┘  └─────────┘  └─────────┘
           │             │             │
           └─────────────┼─────────────┘
                         ▼
                    ┌─────────┐
                    │  Core   │
                    └─────────┘
```

## Crate Structure

Two crates: a core library and a CLI binary.

```
crates/
├── cli/                    # CLI binary
│   ├── Cargo.toml          # depends on: core
│   ├── CLAUDE.md
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/       # CLI command handlers
│   │   └── output.rs       # Output formatting
│   └── tests/              # Integration tests (real adapters)
│
└── core/                   # Library
    ├── Cargo.toml
    ├── CLAUDE.md
    └── src/
        ├── lib.rs
        ├── pipeline.rs     # + pipeline_tests.rs
        ├── queue.rs        # + queue_tests.rs
        ├── task.rs         # + task_tests.rs
        ├── lock.rs         # + lock_tests.rs
        ├── semaphore.rs    # + semaphore_tests.rs
        ├── guard.rs        # + guard_tests.rs
        ├── strategy.rs     # + strategy_tests.rs
        ├── event.rs
        ├── effect.rs
        ├── engine/         # Orchestration (executor, scheduler, recovery)
        ├── adapters/       # Trait definitions + implementations
        ├── storage/        # WAL, state, sync
        └── runbook/        # Parser, validator, loader

checks/
└── specs/
    └── cli/                # Compiled binary E2E tests
```

The layering (core → engine/adapters/storage/runbook → cli) is enforced by module visibility within the library, not by separate crates.

## See Also

- [Runbook Core](02-runbook-core.md) - Detailed core module design
- [Adapters](06-adapters.md) - Adapter implementation details
- [Testing Strategy](07-testing.md) - Testing each layer

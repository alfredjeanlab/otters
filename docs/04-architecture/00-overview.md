# Architecture Overview

## Design Goals

1. **High testability** - Target 90%+ test coverage through architectural choices
2. **Composability** - Small, focused modules that compose into larger behaviors
3. **Offline-first** - Full functionality without network; sync when available
4. **Observability** - Events and metrics at every boundary
5. **Recoverability** - Checkpoint and resume from any failure

## Core Pattern: Functional Core, Imperative Shell

```
┌─────────────────────────────────────────────────────────┐
│                    Imperative Shell                     │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐     │
│  │  CLI    │  │  tmux   │  │  git    │  │ claude  │     │
│  │ Adapter │  │ Adapter │  │ Adapter │  │ Adapter │     │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘     │
│       │            │            │            │          │
│  ┌────┴────────────┴────────────┴────────────┴─────┐    │
│  │              Effect Execution Layer             │    │
│  │   (Executes effects, calls adapters, I/O)       │    │
│  └─────────────────────┬───────────────────────────┘    │
└────────────────────────┼────────────────────────────────┘
                         │
┌────────────────────────┼────────────────────────────────┐
│                        │      Functional Core           │
│  ┌─────────────────────┴───────────────────────────┐    │
│  │              State Machine Engine               │    │
│  │   (Pure state transitions, effect generation)   │    │
│  └─────────────────────┬───────────────────────────┘    │
│                        │                                │
│  ┌─────────┬───────────┼───────────┬─────────┐          │
│  │         │           │           │         │          │
│  ▼         ▼           ▼           ▼         ▼          │
│ Pipeline  Queue      Task       Lock    Semaphore       │
│ (pure)    (pure)     (pure)     (pure)   (pure)         │
│                                                         │
│  Each module: State + Event → (NewState, Effects)       │
└─────────────────────────────────────────────────────────┘
```

**Benefits:**
- Functional core is 100% unit testable with no mocks
- Integration tests focus only on the thin shell
- State transitions are deterministic and reproducible
- Effects are explicit and inspectable

## Key Architectural Decisions

### 1. Effects as Data

All side effects are represented as data structures, not function calls:

```rust
enum Effect {
    WriteFile { path: PathBuf, content: String },
    Spawn { session: SessionId, command: String },
    Emit { event: Event },
    AcquireLock { name: String, holder: String },
    // ...
}
```

The shell interprets and executes these effects via adapters. This allows:
- Testing core logic without any I/O
- Logging/auditing all effects before execution
- Replaying effect sequences for debugging
- Dry-run mode for validation

### 2. Trait-Based Adapters

All external integrations go through trait abstractions:

```rust
trait SessionRunner {
    fn spawn(&self, workspace: &Workspace, cmd: &str) -> Result<SessionId>;
    fn send(&self, session: SessionId, input: &str) -> Result<()>;
    fn kill(&self, session: SessionId) -> Result<()>;
    fn is_alive(&self, session: SessionId) -> Result<bool>;
}
```

Each trait has:
- **Production implementation** - Real tmux, git, etc.
- **Fake implementation** - Records calls, returns configured responses
- **Contract tests** - Verify both implementations behave correctly

### 3. Event-Driven Architecture

```
Pipeline ──emit──▶ EventBus ──notify──▶ Subscribers
                      │
                      ├──▶ Worker (wakes on event)
                      ├──▶ Logger (records event)
                      └──▶ Notifier (sends alert)
```

Components communicate via events rather than direct calls, enabling loose coupling and natural audit trails.

### 4. Explicit State Machines

Each runbook primitive is an explicit state machine with a pure transition function:

```
transition(state, event) → (new_state, effects)
```

This makes state machines fully testable, easy to visualize, and impossible to have hidden state.

### 5. Layered Testing Strategy

- **E2E Tests (5%)**
  - Full system with real tmux, git, filesystem
  - Smoke tests for critical paths
- **Integration Tests (25%)**
  - Engine + fake adapters with in-memory state
  - Real adapter contract tests (`crates/core/tests/`)
- **Unit Tests (70%)**
  - Pure state transitions
  - Effect generation
  - Business logic
  - Property-based tests for state machines

## Module Overview

```
armor/
├── core/              # Functional core (pure, no I/O)
│   ├── pipeline.rs    # Pipeline state machine
│   ├── queue.rs       # Queue operations
│   ├── task.rs        # Task state machine
│   ├── lock.rs        # Lock semantics
│   ├── semaphore.rs   # Semaphore semantics
│   ├── guard.rs       # Guard evaluation
│   ├── strategy.rs    # Strategy chain evaluation
│   ├── event.rs       # Event types and routing
│   └── effect.rs      # Effect types
│
├── engine/            # State machine orchestration
│   ├── executor.rs    # Effect execution loop
│   ├── scheduler.rs   # Task scheduling
│   └── recovery.rs    # Recovery action handling
│
├── adapters/          # External integrations (I/O)
│   ├── traits.rs      # Adapter trait definitions
│   ├── tmux.rs        # tmux session management
│   ├── git.rs         # Git operations
│   ├── claude.rs      # Claude Code integration
│   ├── wk.rs          # Issue tracker integration
│   └── fs.rs          # Filesystem operations
│
├── storage/           # Persistence layer
│   ├── wal.rs         # Write-ahead log
│   ├── state.rs       # State serialization
│   └── sync.rs        # Multi-machine sync
│
├── runbook/           # Runbook loading and parsing
│   ├── parser.rs      # TOML parsing
│   ├── validator.rs   # Runbook validation
│   └── loader.rs      # Dynamic loading
│
└── cli/               # Command-line interface
    ├── commands/      # CLI command handlers
    └── output.rs      # Output formatting
```

## Data Flow

```
User Command
     │
     ▼
┌─────────┐    ┌──────────┐    ┌────────────┐
│   CLI   │───▶│  Engine  │───▶│ Core Logic │
└─────────┘    └──────────┘    └────────────┘
                    │                 │
                    │                 ▼
                    │          (State, Effects)
                    │                 │
                    ▼                 │
              ┌──────────┐            │
              │ Adapters │◀───────────┘
              └──────────┘
                    │
                    ▼
            External Systems
```

1. **CLI** parses command, creates request
2. **Engine** loads current state, invokes core logic
3. **Core** computes new state and effects (pure)
4. **Engine** executes effects via adapters
5. **Adapters** perform actual I/O
6. **Engine** persists new state to WAL

## See Also

- [Module Structure](01-modules.md) - Detailed module boundaries
- [Runbook Core](02-runbook-core.md) - Pipeline, Queue, Task design
- [Execution Layer](03-execution.md) - Workspace, Session design
- [Coordination](04-coordination.md) - Lock, Semaphore, Guard design
- [Storage](05-storage.md) - WAL and state persistence
- [Adapters](06-adapters.md) - Integration adapter design
- [Testing Strategy](07-testing.md) - Achieving high test coverage

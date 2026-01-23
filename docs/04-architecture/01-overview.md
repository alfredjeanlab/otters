# Architecture Overview

## Design Goals

1. **High testability** - 90%+ coverage through architectural choices
2. **Composability** - Small, focused modules that compose
3. **Offline-first** - Full functionality without network
4. **Observability** - Events at every boundary
5. **Recoverability** - Checkpoint and resume from any failure

## Core Pattern: Functional Core, Imperative Shell

```
┌─────────────────────────────────────────────────────────┐
│                    Imperative Shell                     │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐     │
│  │  CLI    │  │  tmux   │  │  git    │  │   wk    │     │
│  │         │  │ Adapter │  │ Adapter │  │ Adapter │     │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘     │
│       │            │            │            │          │
│  ┌────┴────────────┴────────────┴────────────┴─────┐    │
│  │              Effect Execution Layer             │    │
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
│ Pipeline  Queue      Agent      Lock    Semaphore       │
│ (pure)    (pure)     (pure)     (pure)   (pure)         │
│                                                         │
│  Each module: State + Event → (NewState, Effects)       │
└─────────────────────────────────────────────────────────┘
```

## Module Layers

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

**Dependency Rules:**
1. Higher layers may depend on lower layers
2. Same-layer modules may NOT depend on each other (prevents cycles)
3. `core` has ZERO external dependencies (only std)
4. `adapters` may use external crates (tokio, process, etc.)

| Layer | Responsibility | I/O |
|-------|---------------|-----|
| **cli** | Parse args, format output | stdin/stdout |
| **engine** | Execute effects, schedule work | Calls adapters |
| **adapters** | Wrap external tools (tmux, git, wk) | Subprocess I/O |
| **storage** | WAL, snapshots, state persistence | File I/O |
| **runbook** | Parse TOML, validate, load templates | File read |
| **core** | Pure state machines, effect generation | None |

## Key Decisions

### 1. Effects as Data

All side effects are data structures, not function calls:

```rust
enum Effect {
    Spawn { session: SessionId, command: String },
    Emit { event: Event },
    AcquireLock { name: String, holder: String },
}
```

This allows testing without I/O, logging before execution, and dry-run mode.

### 2. Trait-Based Adapters

External integrations go through trait abstractions with production and fake implementations.

### 3. Event-Driven Architecture

Components communicate via events rather than direct calls, enabling loose coupling.

### 4. Explicit State Machines

Each primitive has a pure transition function: `(state, event) → (new_state, effects)`

### 5. Injectable Dependencies

Even `core` needs time and IDs, but these must be injectable:

```rust
pub trait Clock: Clone {
    fn now(&self) -> Instant;
}
```

Build/integration tests use `SystemClock`; unit tests use `FakeClock` for determinism.

## Data Flow

```
CLI ──parse──▶ Request
                  │
                  ▼
Engine ──▶ Core.transition(event) ──▶ (NewState, Effects)
                                            │
              ┌─────────────────────────────┘
              ▼
    for effect in effects:
        adapters.execute(effect)
        storage.persist(new_state)
```

## See Also

- [Effects](02-effects.md) - Effect types and execution
- [Coordination](03-coordination.md) - Lock, Semaphore, Guard
- [Storage](04-storage.md) - WAL and state persistence
- [Adapters](05-adapters.md) - Integration adapters

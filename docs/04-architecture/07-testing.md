# Testing Strategy

This document details the testing approach for achieving very high test coverage (90%+).

## Testing Pyramid

- **Unit tests (70%)** - Pure functions, state machines
- **Integration tests (25%)** - Engine + fake adapters, real adapter contracts
- **E2E / spec tests (5%)** - Compiled binary with real systems

## Unit Testing: The Functional Core

The functional core is 100% testable without mocks. Every pure function:
- Takes explicit inputs (including time via `Clock`)
- Returns explicit outputs (new state + effects)
- Has no hidden dependencies

### Unit Test Convention

Use sibling `_tests.rs` files instead of inline `#[cfg(test)]` modules:

```rust
// src/pipeline.rs
#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
```

```rust
// src/pipeline_tests.rs
use super::*;

#[test]
fn starts_in_init_phase() { ... }
```

**Why separate files?**
- Shorter source files fit better in LLM context windows
- LOC metrics reflect implementation conciseness, not test volume
- Integration tests remain in `tests/` as usual

### State Machine Testing Pattern

Each state machine test:
1. Creates a `FakeClock` and `SequentialIdGen` for deterministic behavior
2. Constructs the state machine with test config
3. Applies transitions via events
4. Asserts on resulting state and effects

Example tests:
- **Initial state** - `Pipeline::new` starts in `Phase::Init`
- **State transition** - `Start` event moves to `Running(first_phase)`, emits `PipelinePhase` event
- **Blocked handling** - Unmet dependencies cause `Blocked` state
- **Timeout** - Use `clock.advance()` to test timeout detection

### Parametrized Tests with yare

Use `yare` for targeted edge case parametrized tests:

```rust
#[yare::parameterized(
    empty_phases = { vec![], Phase::Done },
    single_phase = { vec!["plan".into()], Phase::Running { name: "plan".into() } },
    multiple_phases = { vec!["a".into(), "b".into()], Phase::Running { name: "a".into() } },
)]
fn test_start_transitions_to_first_phase(phases: Vec<String>, expected: Phase) {
    let config = PipelineConfig { phases, ..Default::default() };
    let pipeline = Pipeline::new(config, &FakeClock::new(), &test_ids());
    let (pipeline, _) = pipeline.transition(PipelineEvent::Start, &clock);
    assert_eq!(pipeline.phase, expected);
}
```

### Property-Based Testing with proptest

Use `proptest` to verify general state machine invariants:

Key properties to verify:
- **Transitions never panic** - Apply random event sequences to random configs
- **Terminal states are stable** - `Done` and `Failed(non-recoverable)` ignore all events
- **Phase sequence is monotonic** - Phases never repeat or go backwards
- **Effects match state changes** - Events emitted correspond to actual transitions

Generator pattern:
```rust
prop_compose! {
    fn arb_pipeline_config()(num_phases in 1..5usize) -> PipelineConfig { ... }
}
```

### Effect Testing

Test that correct effects are generated for state transitions:
- Task becoming stuck emits `TaskStuck` event
- Lock acquisition emits `LockAcquired` event
- Recovery actions emit appropriate effects (Nudge, Kill, Spawn)

Use helper assertions: `assert_effect_emitted(effects, &EventKind::TaskStuck)`

## Integration Testing: The Shell

Integration tests verify the imperative shell correctly:
1. Executes effects via adapters
2. Persists state correctly
3. Handles errors appropriately

### Engine Integration Tests

Pattern: Create `Engine<FakeAdapters>` with `Store::in_memory()`, then:
- **State persistence** - Commands create WAL entries and update state
- **Effect execution** - Effects call adapters (verify via `adapters.sessions.calls()`)
- **Error handling** - Configure `FakeSessionConfig { spawn_fails: true }`, verify graceful failure without state corruption
- **Feedback loop** - Effect failure → state machine receives failure event → correct recovery effects generated

### Effect Execution Tests

Test the `EffectExecutor` handles all `Effect` variants without panicking and calls appropriate adapters.

## Adapter Testing

### Contract Tests

Every adapter implementation must pass contract tests. Contract tests are generic over the trait:

```rust
pub async fn run_all<A: SessionAdapter>(adapter: &A) {
    // spawn creates session, is_alive returns true
    // kill removes session, is_alive returns false
    // spawn duplicate name returns AlreadyExists
    // send to nonexistent returns NotFound
}
```

- **Fake adapters** - Run contract tests in unit test suite (`cargo test --lib`)
- **Real adapters** - Run contract tests in `crates/core/tests/`

### Fake Adapter Tests

Test fakes themselves:
- **Call recording** - Verify `calls()` captures all invocations
- **Configurable failures** - `FakeSessionConfig { spawn_fails: true }` causes spawn to error

## Integration Testing: Real Adapters

Integration tests with real adapters (tmux, git, etc.) live in each crate's `tests/` directory:

```
crates/core/tests/
├── tmux_contract.rs    # Real tmux contract tests
├── git_contract.rs     # Real git contract tests
└── ...
```

These test library code with real external systems but don't require the full compiled binary.

## E2E / Spec Testing

Spec tests in `checks/specs/` test the **compiled binaries** end-to-end:

```
checks/specs/
└── cli/           # CLI binary behavior tests
```

### Pattern

1. Build the binary (`cargo build`)
2. Setup temporary git repo / test fixtures
3. Invoke CLI commands via `std::process::Command`
4. Assert on stdout, stderr, exit codes, and file artifacts

Run separately: `cargo test --manifest-path checks/specs/Cargo.toml`

## Test Utilities

### FakeClock

Controllable time for deterministic tests:
- `FakeClock::new()` - Start at current time
- `FakeClock::at(instant)` - Start at specific time
- `clock.advance(duration)` - Move time forward
- Implements `Clock` trait for injection

### Test Fixtures

`fixtures` module provides factory functions:
- `pipeline_config()` - Default 3-phase config
- `queue_config()` - Config with DLQ
- `issue(status)` - Issue with given status
- `guard_inputs()` - Empty GuardInputs

### Test Assertions

`assertions` module provides helpers:
- `assert_effect_emitted(effects, &EventKind::X)` - Check event in effects
- `assert_no_effects(effects)` - Verify empty
- `assert_state_unchanged(before, after)` - Verify equality

## Coverage Targets

| Module | Unit Test Coverage | Integration Coverage |
|--------|-------------------|---------------------|
| `core/pipeline` | 95% | - |
| `core/queue` | 95% | - |
| `core/task` | 95% | - |
| `core/lock` | 95% | - |
| `core/semaphore` | 95% | - |
| `core/guard` | 90% | - |
| `core/strategy` | 90% | - |
| `engine/executor` | 80% | 90% |
| `engine/scheduler` | 70% | 90% |
| `adapters/*` | 50% (fakes) | 80% (contracts) |
| `storage/wal` | 90% | - |
| `storage/state` | 85% | - |
| `storage/sync` | 80% | 70% |
| `cli/*` | 60% | 80% |
| **Overall** | **85%** | **80%** |

## Running Tests

```bash
# Unit tests only (fast)
cargo test --lib

# Unit + integration tests (includes crates/*/tests/)
cargo test

# Spec tests (compiled binary E2E)
cargo test --manifest-path checks/specs/Cargo.toml

# Specific spec component
cargo test --manifest-path checks/specs/Cargo.toml -p specs-cli

# Coverage report (requires LLVM)
./coverage.sh

# Coverage as HTML
./coverage.sh --html

# Property tests with more iterations
PROPTEST_CASES=1000 cargo test

# Specific module
cargo test --lib core::pipeline
```

Coverage uses native rustc instrumentation via `RUSTFLAGS="-C instrument-coverage"` and `llvm-cov`.

## See Also

- [Overview](00-overview.md) - Testing architecture decisions
- [Adapters](06-adapters.md) - Adapter testing details
- [Module Structure](01-modules.md) - Test organization

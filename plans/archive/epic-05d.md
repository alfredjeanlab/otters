# Epic 5d: CLAUDE.md & Invariants

**Root Feature:** `otters-174f`

## Overview

This epic establishes comprehensive AI-assistant guidelines through per-module CLAUDE.md files and documents critical system invariants. The goal is to ensure that AI assistants (and human developers) have clear guidance on module responsibilities, testing expectations, and behavioral contracts.

Key deliverables:
- Enhanced per-crate CLAUDE.md files with landing checklists
- Per-folder CLAUDE.md files for engine/, coordination/, adapters/, events/, pipelines/, storage/
- Documented invariants for state machines, effect ordering, and adapter contracts
- Centralized lint enforcement script at `checks/lint.sh`

## Project Structure

```
otters/
├── CLAUDE.md                          # Root (enhance with policies)
├── checks/
│   └── lint.sh                        # NEW: Lint enforcement script
├── crates/
│   ├── cli/
│   │   ├── CLAUDE.md                  # Enhance existing
│   │   └── src/CLAUDE.md              # NEW: CLI module guidance
│   └── core/
│       ├── CLAUDE.md                  # Enhance existing
│       └── src/
│           ├── adapters/CLAUDE.md     # NEW: Adapter contracts
│           ├── coordination/CLAUDE.md # NEW: Coordination invariants
│           ├── engine/CLAUDE.md       # NEW: Effect ordering rules
│           ├── events/CLAUDE.md       # NEW: Event system contracts
│           ├── pipelines/CLAUDE.md    # NEW: Pipeline state machines
│           └── storage/CLAUDE.md      # NEW: Storage patterns
```

## Dependencies

No new external dependencies required. This epic is purely documentation and scripting.

## Implementation Phases

### Phase 1: Root & Crate-Level CLAUDE.md Enhancement

**Milestone**: All crate-level CLAUDE.md files have complete landing checklists and policies.

#### 1.1 Enhance Root CLAUDE.md

Add explicit policies to `/otters/CLAUDE.md`:

```markdown
## Development Policies

### Dead Code Policy
- All unused code must be removed, not commented out
- No `#[allow(dead_code)]` without documented justification
- Unused dependencies must be removed from Cargo.toml

### Escape Hatch Policy
- `unsafe` blocks require safety comment explaining invariants
- `unwrap()`/`expect()` only in:
  - Tests
  - Infallible cases with comment explaining why
  - CLI parsing where panic is acceptable
- `#[allow(...)]` requires justification comment above

### Test Conventions
- Unit tests in `*_tests.rs` files, imported via `#[cfg(test)]`
- Integration tests in `tests/` directory
- Use `FakeClock`, `FakeAdapters` for deterministic tests
- Property tests for state machine transitions
```

#### 1.2 Enhance `crates/core/CLAUDE.md`

Add landing checklist and module map:

```markdown
## Landing Checklist

Before committing changes to core:
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -p otters-core`
- [ ] No new `#[allow(dead_code)]` without justification
- [ ] State machine changes have corresponding test coverage

## Module Responsibilities

| Module | Responsibility | Key Invariants |
|--------|---------------|----------------|
| adapters/ | External system integration | Fake implementations for all traits |
| coordination/ | Distributed resource management | Heartbeat-based staleness |
| engine/ | Effect execution orchestration | Causal effect ordering |
| events/ | Event routing and audit | Pattern-based subscriptions |
| pipelines/ | Workflow state machines | Deterministic transitions |
| storage/ | State persistence | Atomic writes |
```

#### 1.3 Enhance `crates/cli/CLAUDE.md`

Add CLI-specific guidance:

```markdown
## Landing Checklist

Before committing changes to cli:
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -p otters-cli`
- [ ] CLI help text is accurate
- [ ] Error messages are user-friendly

## CLI Conventions
- Use `anyhow` for error handling with context
- `expect()` allowed for argument parsing (panic acceptable)
- Commands should be idempotent where possible
```

### Phase 2: Adapter & Coordination CLAUDE.md Files

**Milestone**: adapters/ and coordination/ have complete CLAUDE.md with invariants.

#### 2.1 Create `crates/core/src/adapters/CLAUDE.md`

```markdown
# Adapters Module

External system integration layer. All I/O happens through adapters.

## Adapter Contracts

### Required Implementations
Every adapter trait MUST have:
1. A real implementation (production use)
2. A fake implementation (testing use)

### Trait Definitions

| Trait | Purpose | Key Methods |
|-------|---------|-------------|
| `SessionAdapter` | tmux session management | `spawn`, `send`, `kill`, `is_alive`, `capture_pane` |
| `RepoAdapter` | Git worktree operations | `worktree_add`, `worktree_remove`, `is_clean`, `merge` |
| `NotifyAdapter` | User notifications | `notify` |
| `ClaudeAdapter` | Claude Code integration | `spawn_session` |
| `IssueAdapter` | Issue tracker integration | `fetch_issue`, `update_status` |

### Invariants

1. **Async-only I/O**: All adapter methods are `async`. No blocking I/O.
2. **Error propagation**: Return `Result<T, E>` with descriptive errors.
3. **Idempotency**: Operations should be idempotent where possible.
4. **Fake parity**: Fake implementations must exercise the same code paths.

### Testing Pattern

```rust
// Tests use FakeAdapters for determinism
let adapters = FakeAdapters::new();
adapters.session.set_alive("session-1", true);

// Real adapters only in integration tests
#[cfg(feature = "integration")]
async fn test_real_tmux() { ... }
```

## Landing Checklist

- [ ] New adapter trait has fake implementation
- [ ] Fake implementation in `fake.rs` or `*_tests.rs`
- [ ] Error types implement `std::error::Error`
- [ ] No blocking I/O (use async)
```

#### 2.2 Create `crates/core/src/coordination/CLAUDE.md`

```markdown
# Coordination Module

Distributed resource management primitives for multi-worker systems.

## State Machine Invariants

### Lock Invariants

```
INVARIANT: A lock has at most one holder at any time
INVARIANT: Stale locks (no heartbeat for `stale_after`) can be reclaimed
INVARIANT: Lock state transitions are atomic
```

State transitions:
```
Unlocked --acquire(holder)--> Locked(holder)
Locked(holder) --release(holder)--> Unlocked
Locked(holder) --reclaim(new_holder) [if stale]--> Locked(new_holder)
```

### Semaphore Invariants

```
INVARIANT: Sum of holder weights <= capacity
INVARIANT: Stale holders are removed on tick()
INVARIANT: Weight must be > 0
```

State transitions:
```
Available(remaining) --acquire(holder, weight)--> Available(remaining - weight)
Available(0) --acquire(...)--> Blocked
Blocked --release(holder, weight)--> Available(weight)
```

### Guard Invariants

```
INVARIANT: Guards are evaluated atomically
INVARIANT: NeedsInput returns specific data requirements
INVARIANT: Composition (and/or) preserves invariants
```

Evaluation:
```
Guard::evaluate(data) -> GuardResult
  GuardResult::Passed        // Condition met
  GuardResult::Failed(reason) // Condition not met
  GuardResult::NeedsInput(req) // More data needed
```

## Effect Ordering

Coordination effects must execute in causal order:
1. `CheckGuard` before `AcquireLock`
2. `AcquireLock` before `StartWork`
3. `ReleaseResource` after work completion

## Landing Checklist

- [ ] State transitions are deterministic (same input → same output)
- [ ] Invariants documented in code comments
- [ ] Property tests for state machine transitions
- [ ] Staleness thresholds are configurable
```

### Phase 3: Engine & Events CLAUDE.md Files

**Milestone**: engine/ and events/ have complete CLAUDE.md with effect ordering rules.

#### 3.1 Create `crates/core/src/engine/CLAUDE.md`

```markdown
# Engine Module

Core execution orchestration: scheduler, workers, and effect execution.

## Effect System Invariants

### Effect Ordering Rules

```
INVARIANT: Effects execute in causal order
INVARIANT: Effects are data (not functions) - serializable and inspectable
INVARIANT: Effect execution never blocks the event loop
INVARIANT: Failed effects produce error events (not panics)
```

### Execution Flow

```
Event → Scheduler → Worker → State Machine → Effects
                                    ↑              ↓
                                    └── Events ←───┘
```

### Executor Invariants

```
INVARIANT: One effect executes at a time per worker
INVARIANT: Effects with dependencies wait for prerequisites
INVARIANT: Checkpoint effects block until persistence confirmed
```

### Effect Categories

| Category | Blocking | Example |
|----------|----------|---------|
| State mutation | No | `UpdateTaskState` |
| External I/O | Yes (async) | `CreateWorktree`, `SendCommand` |
| Persistence | Yes | `SaveCheckpoint` |
| Notification | No | `EmitEvent` |

### Recovery Invariants

```
INVARIANT: Recovery replays from last checkpoint
INVARIANT: Idempotent effects can safely re-execute
INVARIANT: Non-idempotent effects check preconditions
```

## Landing Checklist

- [ ] New effects are data structures (not closures)
- [ ] Effect handlers are async
- [ ] Error events emitted on failure
- [ ] Recovery path tested
```

#### 3.2 Create `crates/core/src/events/CLAUDE.md`

```markdown
# Events Module

Event routing, subscriptions, and audit logging.

## Event System Invariants

### EventBus Invariants

```
INVARIANT: Events are delivered at-least-once
INVARIANT: Subscribers receive events matching their pattern
INVARIANT: Event delivery does not block publisher
```

### Pattern Matching

```
"workspace:*"     - All workspace events
"task:stuck"      - Specific event
"pipeline:build:*" - All build pipeline events
```

### EventLog Invariants

```
INVARIANT: All events are logged (audit trail)
INVARIANT: Log entries are immutable
INVARIANT: Timestamps use monotonic clock
```

### Subscription Lifecycle

```
Subscribe(pattern) → SubscriptionId
  ↓
Receive(events matching pattern)
  ↓
Unsubscribe(SubscriptionId) → No more events
```

## Landing Checklist

- [ ] Events are serializable (for logging)
- [ ] Patterns use consistent naming (noun:verb or noun:adjective)
- [ ] Subscriptions cleaned up on worker shutdown
```

### Phase 4: Pipelines & Storage CLAUDE.md Files

**Milestone**: pipelines/ and storage/ have complete CLAUDE.md.

#### 4.1 Create `crates/core/src/pipelines/CLAUDE.md`

```markdown
# Pipelines Module

Workflow state machines for build and bugfix pipelines.

## Pipeline State Machine Pattern

All pipelines follow the pure state machine pattern:

```rust
fn transition(state: &State, event: Event) -> (State, Vec<Effect>)
```

### Invariants

```
INVARIANT: Transitions are pure functions (no I/O)
INVARIANT: Same (state, event) always produces same (state', effects)
INVARIANT: Terminal states produce no effects
INVARIANT: Effects capture all side effects
```

### Pipeline States

```
         ┌─────────────────────────────────────────┐
         ↓                                         │
     Pending → Running → Verifying → Completed     │
         │         │         │                     │
         └────────→├─────────┘                     │
                   ↓                               │
                Failed ←───────────────────────────┘
                   │
                   ↓
               Cancelled
```

### Build Pipeline Specifics

Phases: Setup → Build → Test → Verify → Complete
- Each phase has guard conditions
- Failure in any phase → Failed state
- Retry logic handled by effects, not state machine

### Bugfix Pipeline Specifics

Phases: Analyze → Fix → Verify → Complete
- Analyze may require multiple iterations
- Fix produces code changes
- Verify runs tests

## Landing Checklist

- [ ] State transitions are exhaustive (all cases handled)
- [ ] Property tests verify transition determinism
- [ ] Terminal states are explicit
- [ ] Effects are minimal and necessary
```

#### 4.2 Create `crates/core/src/storage/CLAUDE.md`

```markdown
# Storage Module

State persistence and JSON serialization.

## Storage Invariants

```
INVARIANT: Writes are atomic (no partial states)
INVARIANT: Reads return consistent state or error
INVARIANT: Storage format is backward-compatible within major version
```

### Serialization Pattern

```rust
// All state types derive Serialize/Deserialize
#[derive(Serialize, Deserialize)]
struct PipelineState { ... }

// Storage uses JSON for human readability
let json = serde_json::to_string_pretty(&state)?;
```

### File Layout

```
.oj/
├── state.json        # Current system state
├── events.jsonl      # Event audit log (append-only)
└── checkpoints/      # Recovery checkpoints
    └── {timestamp}.json
```

### Backward Compatibility

- Add fields with `#[serde(default)]`
- Never remove fields in minor versions
- Use `#[serde(rename = "old_name")]` for renames

## Landing Checklist

- [ ] New fields have `#[serde(default)]`
- [ ] State roundtrips through JSON correctly
- [ ] Large states tested for performance
```

### Phase 5: Lint Enforcement Script

**Milestone**: `checks/lint.sh` provides unified lint enforcement.

#### 5.1 Create `checks/lint.sh`

```bash
#!/usr/bin/env bash
# checks/lint.sh - Unified lint enforcement for otters
#
# Usage: ./checks/lint.sh [--fix]
#
# Runs all lint checks. With --fix, attempts to auto-fix issues.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

failed=0

echo "🔍 Running lint checks..."
echo ""

# 1. Format check
echo "📝 Checking formatting..."
if $FIX_MODE; then
    cargo fmt --all
    echo -e "${GREEN}✓ Formatted${NC}"
else
    if ! cargo fmt --all -- --check; then
        echo -e "${RED}✗ Format check failed. Run './checks/lint.sh --fix'${NC}"
        failed=1
    else
        echo -e "${GREEN}✓ Format OK${NC}"
    fi
fi

# 2. Clippy
echo ""
echo "📎 Running clippy..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo -e "${RED}✗ Clippy found issues${NC}"
    failed=1
else
    echo -e "${GREEN}✓ Clippy OK${NC}"
fi

# 3. Dead code check (custom)
echo ""
echo "💀 Checking for unauthorized dead code..."
DEAD_CODE=$(grep -r '#\[allow(dead_code)\]' crates/ --include="*.rs" | grep -v "// JUSTIFIED:" || true)
if [[ -n "$DEAD_CODE" ]]; then
    echo -e "${YELLOW}⚠ Found #[allow(dead_code)] without justification:${NC}"
    echo "$DEAD_CODE"
    echo -e "${YELLOW}  Add '// JUSTIFIED: <reason>' comment to suppress${NC}"
    failed=1
else
    echo -e "${GREEN}✓ No unauthorized dead code${NC}"
fi

# 4. Unsafe check
echo ""
echo "🔒 Checking unsafe blocks..."
UNSAFE=$(grep -rn 'unsafe {' crates/ --include="*.rs" | grep -v "// SAFETY:" || true)
if [[ -n "$UNSAFE" ]]; then
    echo -e "${YELLOW}⚠ Found unsafe blocks without SAFETY comment:${NC}"
    echo "$UNSAFE"
    failed=1
else
    echo -e "${GREEN}✓ All unsafe blocks documented${NC}"
fi

# 5. Unwrap check (outside tests)
echo ""
echo "🎁 Checking unwrap() usage..."
# Find unwrap/expect outside test files and test modules
UNWRAP=$(grep -rn '\.unwrap()' crates/ --include="*.rs" | \
    grep -v '_tests\.rs' | \
    grep -v '#\[cfg(test)\]' | \
    grep -v '// OK:' | \
    grep -v 'test' || true)
if [[ -n "$UNWRAP" ]]; then
    echo -e "${YELLOW}⚠ Found unwrap() in non-test code without '// OK:' comment:${NC}"
    echo "$UNWRAP" | head -10
    if [[ $(echo "$UNWRAP" | wc -l) -gt 10 ]]; then
        echo "  ... and more"
    fi
    # This is a warning, not a failure (for now)
fi
echo -e "${GREEN}✓ Unwrap check complete${NC}"

# 6. Test file check
echo ""
echo "🧪 Verifying test file conventions..."
# Check that all *_tests.rs files are imported via #[cfg(test)]
for test_file in $(find crates/ -name "*_tests.rs"); do
    module_name=$(basename "$test_file" .rs)
    parent_dir=$(dirname "$test_file")
    mod_file="$parent_dir/mod.rs"

    if [[ -f "$mod_file" ]]; then
        if ! grep -q "#\[cfg(test)\]" "$mod_file" || ! grep -q "mod $module_name" "$mod_file"; then
            echo -e "${YELLOW}⚠ $test_file may not be imported correctly${NC}"
        fi
    fi
done
echo -e "${GREEN}✓ Test conventions OK${NC}"

# Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [[ $failed -eq 0 ]]; then
    echo -e "${GREEN}✅ All lint checks passed${NC}"
    exit 0
else
    echo -e "${RED}❌ Some lint checks failed${NC}"
    exit 1
fi
```

#### 5.2 Integrate with Makefile

Add to Makefile:

```makefile
lint:
	./checks/lint.sh

lint-fix:
	./checks/lint.sh --fix
```

### Phase 6: Final Integration & Verification

**Milestone**: All CLAUDE.md files complete and lint script functional.

#### 6.1 Create checks/ Directory Structure

```
checks/
├── lint.sh          # Main lint script (Phase 5)
└── README.md        # Document available checks
```

#### 6.2 Update Root CLAUDE.md

Reference new lint script:

```markdown
## Landing the Plane

Before committing changes:

- [ ] Run `./checks/lint.sh` (or `make lint`)
- [ ] Run `make check` for full verification
```

#### 6.3 Verification Checklist

- [ ] All 8 new CLAUDE.md files created
- [ ] `checks/lint.sh` executable and working
- [ ] `make lint` target added
- [ ] Existing CLAUDE.md files enhanced

## Key Implementation Details

### CLAUDE.md Template Structure

Each module CLAUDE.md should follow this structure:

```markdown
# Module Name

Brief description of module purpose.

## Invariants

List of formal invariants with code examples.

## Key Patterns

Important patterns used in this module.

## Landing Checklist

Module-specific pre-commit checks.
```

### Invariant Documentation Format

Use triple-backtick blocks with `INVARIANT:` prefix:

```
INVARIANT: Description of the property that must always hold
```

### State Machine Documentation

Use ASCII diagrams for state transitions:

```
State1 --event--> State2
   │                 │
   └──error───> ErrorState
```

## Verification Plan

### Phase-by-Phase Verification

| Phase | Verification Method |
|-------|-------------------|
| 1 | Review enhanced CLAUDE.md files for completeness |
| 2 | Verify adapter and coordination invariants match code |
| 3 | Verify effect ordering rules match executor implementation |
| 4 | Verify pipeline state machines match documentation |
| 5 | Run `./checks/lint.sh` on clean and dirty codebases |
| 6 | Full `make check` passes |

### Invariant Verification

For each documented invariant:
1. Verify property tests exist that would catch violations
2. Verify code comments reference the invariant
3. Verify recovery paths maintain invariants

### Final Acceptance Criteria

- [ ] `./checks/lint.sh` exits 0 on current codebase
- [ ] All CLAUDE.md files render correctly in GitHub
- [ ] Invariants are machine-verifiable where possible
- [ ] `make check` passes
- [ ] No new `#[allow(dead_code)]` without justification

## File Summary

| File | Action | Phase |
|------|--------|-------|
| `CLAUDE.md` | Enhance | 1 |
| `crates/core/CLAUDE.md` | Enhance | 1 |
| `crates/cli/CLAUDE.md` | Enhance | 1 |
| `crates/core/src/adapters/CLAUDE.md` | Create | 2 |
| `crates/core/src/coordination/CLAUDE.md` | Create | 2 |
| `crates/core/src/engine/CLAUDE.md` | Create | 3 |
| `crates/core/src/events/CLAUDE.md` | Create | 3 |
| `crates/core/src/pipelines/CLAUDE.md` | Create | 4 |
| `crates/core/src/storage/CLAUDE.md` | Create | 4 |
| `checks/lint.sh` | Create | 5 |
| `Makefile` | Enhance | 5 |

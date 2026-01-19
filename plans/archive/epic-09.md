# Epic 9: Polish & Production Readiness

**Epic ID:** 9
**Status:** Planning
**Depends on:** 5c (quality baseline), 9g (production adapter wiring)
**Root Feature:** `otters-5837`

## Overview

Final polish, performance optimization, and production readiness for day-to-day operation. This epic addresses rough edges, fills test coverage gaps, improves error messages, and documents operational procedures. No new features—stabilize existing functionality.

**Current State (from baseline.json):**
- 483 tests across 2 crates
- 37 `unwrap()` calls in source (need audit)
- 11 `#[allow(dead_code)]` annotations
- 1 file over 600 LOC limit
- Core: 11,730 source LOC, 10,076 test LOC
- CLI: 1,404 source LOC, 3,208 test LOC

**Key Deliverables:**
1. 90%+ test coverage with claude simulator edge case testing
2. Actionable error messages with context and recovery suggestions
3. CLI polish: help text, shell completions, progress indicators
4. Performance profiling and hot path optimization
5. Operational documentation: runbook, troubleshooting guide
6. Migration script from bash scripts
7. Graceful shutdown with resource cleanup verification
8. Memory and file handle resource limits

## Project Structure

```
otters/
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── error.rs              # UPDATE: Add context and suggestions
│   │       ├── engine/
│   │       │   ├── runtime.rs        # UPDATE: Resource limits, graceful shutdown
│   │       │   └── signals.rs        # UPDATE: Clean termination verification
│   │       └── limits.rs             # NEW: Resource limit types and enforcement
│   │
│   └── cli/
│       ├── src/
│       │   ├── main.rs               # UPDATE: Progress indicators
│       │   ├── error.rs              # UPDATE: User-friendly error display
│       │   └── completions.rs        # NEW: Shell completion generation
│       └── Cargo.toml                # UPDATE: Add clap_complete
│
├── docs/
│   ├── 05-operations/
│   │   ├── 01-runbook.md             # NEW: Operational runbook
│   │   └── 02-troubleshooting.md     # NEW: Troubleshooting guide
│   └── 06-migration/
│       └── 01-from-bash.md           # NEW: Migration guide
│
├── scripts/
│   └── migrate-from-bash.sh          # NEW: Migration script
│
├── checks/
│   └── coverage/
│       ├── measure.sh                # NEW: Coverage measurement
│       └── report.sh                 # NEW: Coverage reporting
│
└── tests/
    └── integration/
        └── edge_cases/               # NEW: Claude simulator edge case tests
```

## Dependencies

### New Dependencies (workspace)

```toml
[workspace.dependencies]
clap_complete = "4.5"     # Shell completion generation

[dev-dependencies]
cargo-llvm-cov = "0.6"    # Coverage measurement (install via cargo)
```

### External Tools

```bash
cargo install cargo-llvm-cov   # Coverage reporting
cargo install flamegraph        # Performance profiling (optional)
```

## Implementation Phases

### Phase 1: Test Coverage Infrastructure & Gap Analysis

**Goal**: Establish coverage measurement and identify gaps to reach 90%+ coverage.

**Deliverables**:
1. Coverage measurement scripts
2. Module-by-module coverage report
3. Prioritized list of uncovered paths
4. Edge case test plan using claude simulator

**Coverage Scripts**:

```bash
#!/bin/bash
# checks/coverage/measure.sh
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    --json --output-path target/coverage.json

cargo llvm-cov report --json \
    --ignore-filename-regex='_tests\.rs$' \
    | jq '.data[0].totals.lines.percent' > target/coverage-percent.txt

echo "Coverage: $(cat target/coverage-percent.txt)%"
```

```bash
#!/bin/bash
# checks/coverage/report.sh
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Generate HTML report
cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    --html --output-dir target/coverage-html

# Generate per-file report
cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    2>&1 | grep -E '^\s+[0-9]+\.[0-9]+%' | sort -t'%' -k1 -n

echo ""
echo "HTML report: target/coverage-html/index.html"
```

**Gap Analysis Process**:

1. Run coverage measurement
2. Identify files below 80% coverage
3. Categorize uncovered code:
   - Error paths (need failure injection tests)
   - Edge cases (need property tests or parametrized tests)
   - Integration paths (need claude simulator tests)
   - Dead code (delete or justify)

**Priority Modules for Coverage**:

| Module | Priority | Reason |
|--------|----------|--------|
| `scheduling/` | High | Recently added, complex state machines |
| `coordination/` | High | Critical for correctness |
| `engine/runtime.rs` | High | Integration point for all systems |
| `storage/wal/` | Medium | Durability guarantees |
| `runbook/` | Medium | User-facing parsing |

**Verification**:
- [ ] `./checks/coverage/measure.sh` produces coverage percentage
- [ ] `./checks/coverage/report.sh` generates HTML and per-file reports
- [ ] Coverage gaps documented with test plan

---

### Phase 2: Edge Case Testing with Claude Simulator

**Goal**: Use claudeless simulator to test complex integration scenarios and error paths.

**Deliverables**:
1. Edge case test scenarios in TOML
2. Integration tests exercising failure modes
3. Recovery path verification

**Test Scenarios**:

```toml
# tests/integration/edge_cases/network_failures.toml
[scenario]
name = "network_timeout_recovery"
description = "Verify task recovers from network timeout"

[[responses]]
pattern = ".*"
delay_ms = 30000  # Simulate timeout
error = "network"

[[responses]]
pattern = ".*"
response = "Task completed after retry"
```

```toml
# tests/integration/edge_cases/malformed_output.toml
[scenario]
name = "malformed_json_handling"
description = "Verify graceful handling of malformed JSON"

[[responses]]
pattern = ".*get.*status.*"
response = "{not valid json"

[[responses]]
pattern = ".*retry.*"
response = '{"status": "ok"}'
```

**Integration Test Structure**:

```rust
// tests/integration/edge_cases/mod.rs

#[test]
fn test_network_timeout_triggers_retry() {
    let scenario = load_scenario("network_failures.toml");
    let (adapters, _recorder) = FakeAdapters::with_claude_scenario(scenario);

    let mut engine = Engine::new(adapters, FakeClock::new());
    let pipeline_id = engine.create_pipeline("test-pipeline");

    // First tick should timeout and schedule retry
    let effects = engine.tick();
    assert!(effects.iter().any(|e| matches!(e, Effect::ScheduleRetry { .. })));

    // Advance clock past retry delay
    engine.clock.advance(Duration::from_secs(60));

    // Second tick should succeed
    let effects = engine.tick();
    assert!(effects.iter().any(|e| matches!(e, Effect::Complete { .. })));
}

#[test]
fn test_graceful_degradation_on_auth_failure() {
    let scenario = load_scenario("auth_failure.toml");
    let (adapters, _recorder) = FakeAdapters::with_claude_scenario(scenario);

    let mut engine = Engine::new(adapters, FakeClock::new());

    // Should emit escalation event, not crash
    let effects = engine.tick();
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Emit { event, .. } if event.contains("auth_failure")
    )));
}
```

**Coverage Targets by Module**:

| Module | Current | Target | Strategy |
|--------|---------|--------|----------|
| `scheduling/cron.rs` | ~70% | 95% | State machine edge cases |
| `scheduling/watcher.rs` | ~65% | 95% | Condition evaluation paths |
| `coordination/lock.rs` | ~85% | 95% | Timeout and stale detection |
| `engine/executor.rs` | ~75% | 90% | Effect failure injection |
| `storage/wal/recovery.rs` | ~60% | 95% | Corruption scenarios |

**Verification**:
- [ ] All edge case scenarios have passing tests
- [ ] Failure injection tests cover network, auth, rate-limit, timeout
- [ ] Recovery paths verified for each failure mode
- [ ] Coverage increased by at least 10% from baseline

---

### Phase 3: Error Messages & CLI Polish

**Goal**: Make errors actionable and improve CLI user experience.

**Deliverables**:
1. Error context and suggestions framework
2. Shell completion generation
3. Progress indicators for long operations
4. Improved help text

**Error Framework**:

```rust
// crates/core/src/error.rs

use thiserror::Error;

/// Error with context and recovery suggestions
#[derive(Error, Debug)]
pub struct OjError {
    /// What went wrong
    pub message: String,
    /// Why it might have happened
    pub context: Vec<String>,
    /// How to fix it
    pub suggestions: Vec<String>,
    /// Original error if any
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl OjError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            context: Vec::new(),
            suggestions: Vec::new(),
            source: None,
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }
}

// Example usage
fn lock_acquire_error(lock_id: &str, holder: &str) -> OjError {
    OjError::new(format!("Failed to acquire lock '{}'", lock_id))
        .with_context(format!("Lock is currently held by '{}'", holder))
        .with_context("Lock has been held for 45 minutes")
        .with_suggestion("Wait for the current holder to release")
        .with_suggestion(format!("Force release with: oj lock release {} --force", lock_id))
        .with_suggestion("Check if the holder process is stuck: oj session status")
}
```

**CLI Error Display**:

```rust
// crates/cli/src/error.rs

impl std::fmt::Display for OjError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "error: {}", self.message)?;

        if !self.context.is_empty() {
            writeln!(f)?;
            for ctx in &self.context {
                writeln!(f, "  → {}", ctx)?;
            }
        }

        if !self.suggestions.is_empty() {
            writeln!(f)?;
            writeln!(f, "suggestions:")?;
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, suggestion)?;
            }
        }

        Ok(())
    }
}
```

**Shell Completions**:

```rust
// crates/cli/src/completions.rs

use clap::CommandFactory;
use clap_complete::{generate, Shell};

pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "oj", &mut std::io::stdout());
}

// Add to CLI
#[derive(Subcommand)]
enum Commands {
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    // ... existing commands
}
```

**Progress Indicators**:

```rust
// crates/cli/src/progress.rs

use indicatif::{ProgressBar, ProgressStyle};

pub struct ProgressIndicator {
    bar: ProgressBar,
}

impl ProgressIndicator {
    pub fn spinner(message: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("valid template")
        );
        bar.set_message(message.to_string());
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        Self { bar }
    }

    pub fn update(&self, message: &str) {
        self.bar.set_message(message.to_string());
    }

    pub fn finish(&self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }
}
```

**Help Text Improvements**:

```rust
// Improved command documentation
#[derive(Parser)]
#[command(
    name = "oj",
    about = "Agentic development orchestration",
    long_about = "oj orchestrates AI-assisted development workflows, managing \
                  workspaces, sessions, and pipelines for automated coding tasks.",
    after_help = "EXAMPLES:\n    \
                  oj run build          Run the build pipeline\n    \
                  oj run bugfix         Run the bugfix pipeline\n    \
                  oj pipeline status    Show pipeline status\n    \
                  oj workspace list     List active workspaces"
)]
struct Cli { ... }
```

**Verification**:
- [ ] All errors include context and suggestions
- [ ] `oj completions bash/zsh/fish` generates valid completions
- [ ] Long-running commands show progress spinners
- [ ] Help text is comprehensive with examples

---

### Phase 4: Performance Profiling & Optimization

**Goal**: Profile hot paths and optimize any regressions from baseline.

**Deliverables**:
1. Profiling setup and documentation
2. Hot path identification
3. Targeted optimizations
4. Benchmark comparison vs baseline

**Profiling Setup**:

```bash
#!/bin/bash
# scripts/profile.sh
set -euo pipefail

case "${1:-cpu}" in
    cpu)
        cargo flamegraph --bin oj -- run build --dry-run
        ;;
    memory)
        cargo build --release
        heaptrack ./target/release/oj run build --dry-run
        ;;
    time)
        hyperfine --warmup 3 \
            './target/release/oj pipeline list' \
            './target/release/oj workspace list'
        ;;
esac
```

**Hot Path Analysis**:

Based on architecture, likely hot paths:
1. **WAL writes** - fsync on every operation
2. **State materialization** - replaying operations on startup
3. **Effect execution** - adapter calls in tight loops
4. **Lock heartbeat** - frequent refresh operations

**Optimization Targets**:

```rust
// WAL batching for multiple operations
impl WalStore {
    /// Batch multiple operations into a single fsync
    pub fn execute_batch(&mut self, ops: Vec<Operation>) -> Result<(), WalError> {
        for op in &ops {
            self.write_entry(op)?;
        }
        self.sync()?;  // Single fsync for batch
        for op in ops {
            self.apply(op);
        }
        Ok(())
    }
}

// Lazy state materialization
impl Store {
    /// Materialize only requested state, not full state
    pub fn materialize_lazy(&self) -> LazyState {
        LazyState::new(&self.wal)
    }
}

// Connection pooling for adapters
impl RealAdapters {
    /// Reuse tmux connections instead of spawning new processes
    pub fn with_connection_pool(pool_size: usize) -> Self { ... }
}
```

**Benchmark Targets**:

| Operation | Baseline | Target | Notes |
|-----------|----------|--------|-------|
| Cold start | ~500ms | <300ms | Lazy materialization |
| WAL write | ~10ms | <5ms | Batching |
| Pipeline list | ~50ms | <20ms | Cached state |
| Lock acquire | ~15ms | <10ms | Connection reuse |

**Verification**:
- [ ] Profiling produces flamegraph/heaptrack output
- [ ] Hot paths identified and documented
- [ ] No performance regressions from baseline
- [ ] Key operations meet target benchmarks

---

### Phase 5: Graceful Shutdown & Resource Limits

**Goal**: Ensure clean termination and bounded resource usage.

**Deliverables**:
1. Graceful shutdown verification
2. Resource limit enforcement
3. Cleanup verification tests

**Graceful Shutdown**:

```rust
// crates/core/src/engine/signals.rs

pub struct ShutdownCoordinator {
    workers: Vec<WorkerHandle>,
    sessions: Vec<SessionId>,
    timeout: Duration,
}

impl ShutdownCoordinator {
    pub async fn shutdown(&mut self) -> ShutdownResult {
        let mut result = ShutdownResult::default();

        // Phase 1: Signal workers to stop accepting new work
        for worker in &self.workers {
            worker.stop_accepting();
        }

        // Phase 2: Wait for in-progress work (with timeout)
        let deadline = Instant::now() + self.timeout;
        for worker in &mut self.workers {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match worker.wait_idle(remaining).await {
                Ok(()) => result.workers_clean += 1,
                Err(_) => {
                    worker.force_stop();
                    result.workers_forced += 1;
                }
            }
        }

        // Phase 3: Checkpoint sessions
        for session_id in &self.sessions {
            if let Err(e) = self.checkpoint_session(session_id).await {
                tracing::warn!(?session_id, ?e, "failed to checkpoint session");
                result.sessions_lost.push(session_id.clone());
            }
        }

        // Phase 4: Release all locks
        self.release_all_locks().await;

        result
    }
}

#[derive(Default)]
pub struct ShutdownResult {
    pub workers_clean: usize,
    pub workers_forced: usize,
    pub sessions_lost: Vec<SessionId>,
}
```

**Resource Limits**:

```rust
// crates/core/src/limits.rs

/// Resource limits for the engine
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum concurrent sessions
    pub max_sessions: usize,
    /// Maximum file handles (soft limit)
    pub max_file_handles: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Maximum WAL size before compaction
    pub max_wal_size_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_sessions: 10,
            max_file_handles: 256,
            max_memory_bytes: 512 * 1024 * 1024,  // 512MB
            max_wal_size_bytes: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Monitor resource usage
pub struct ResourceMonitor {
    limits: ResourceLimits,
}

impl ResourceMonitor {
    pub fn check(&self) -> ResourceStatus {
        let current = self.measure_current();
        ResourceStatus {
            sessions: UsageLevel::from_ratio(
                current.sessions as f64 / self.limits.max_sessions as f64
            ),
            file_handles: UsageLevel::from_ratio(
                current.file_handles as f64 / self.limits.max_file_handles as f64
            ),
            memory: UsageLevel::from_ratio(
                current.memory_bytes as f64 / self.limits.max_memory_bytes as f64
            ),
            wal_size: UsageLevel::from_ratio(
                current.wal_size_bytes as f64 / self.limits.max_wal_size_bytes as f64
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsageLevel {
    Normal,     // < 70%
    Warning,    // 70-90%
    Critical,   // > 90%
}
```

**Cleanup Verification Tests**:

```rust
#[test]
fn test_graceful_shutdown_completes_in_progress_work() {
    let (adapters, recorder) = FakeAdapters::new();
    let mut engine = Engine::new(adapters, FakeClock::new());

    // Start a pipeline
    engine.create_and_start_pipeline("test");

    // Initiate shutdown
    let result = engine.shutdown(Duration::from_secs(30));

    // Verify checkpoint was called
    assert!(recorder.calls().iter().any(|c| c.contains("checkpoint")));
    assert_eq!(result.workers_forced, 0);
}

#[test]
fn test_resource_limits_prevent_overload() {
    let limits = ResourceLimits {
        max_sessions: 2,
        ..Default::default()
    };
    let mut engine = Engine::with_limits(FakeAdapters::new().0, limits);

    // Create sessions up to limit
    engine.create_session("session-1").unwrap();
    engine.create_session("session-2").unwrap();

    // Third session should be rejected
    let result = engine.create_session("session-3");
    assert!(matches!(result, Err(OjError { .. })));
}
```

**Verification**:
- [ ] SIGTERM triggers graceful shutdown
- [ ] In-progress work completes before exit
- [ ] Sessions checkpointed on shutdown
- [ ] Resource limits enforced
- [ ] No leaked file handles after shutdown

---

### Phase 6: Documentation & Migration

**Goal**: Complete operational documentation and migration tooling.

**Deliverables**:
1. Operational runbook
2. Troubleshooting guide
3. Migration script from bash scripts

**Operational Runbook** (`docs/05-operations/01-runbook.md`):

```markdown
# Operational Runbook

## Starting the Daemon

\`\`\`bash
oj daemon start
\`\`\`

## Health Checks

\`\`\`bash
# Quick health check
oj status

# Detailed diagnostics
oj status --verbose
\`\`\`

## Common Operations

### View Active Work
\`\`\`bash
oj pipeline list --active
oj session list
oj workspace list
\`\`\`

### Handle Stuck Pipeline
\`\`\`bash
# Check status
oj pipeline status <pipeline-id>

# View session output
oj session capture <session-id>

# Nudge the agent
oj pipeline nudge <pipeline-id>

# If still stuck, restart
oj pipeline restart <pipeline-id>
\`\`\`

### Release Stale Lock
\`\`\`bash
# Check lock status
oj lock status merge-lock

# Force release (use with caution)
oj lock release merge-lock --force
\`\`\`

## Monitoring

### Resource Usage
\`\`\`bash
oj status --resources
\`\`\`

### WAL Size
\`\`\`bash
ls -lh .build/wal/
\`\`\`

### Trigger Compaction
\`\`\`bash
oj maintenance compact
\`\`\`
```

**Troubleshooting Guide** (`docs/05-operations/02-troubleshooting.md`):

```markdown
# Troubleshooting Guide

## Pipeline Won't Start

**Symptoms**: `oj run build` hangs or returns immediately without starting

**Possible Causes**:
1. Lock held by another pipeline
2. Semaphore capacity exhausted
3. Guard condition not met

**Resolution**:
\`\`\`bash
# Check locks
oj lock list

# Check semaphores
oj semaphore status agent-slots

# Check guards
oj pipeline guards <pipeline-id>
\`\`\`

## Session Not Responding

**Symptoms**: Session shows as active but no output

**Possible Causes**:
1. Claude CLI hung
2. Network timeout
3. Rate limit exceeded

**Resolution**:
\`\`\`bash
# Check session status
oj session status <session-id>

# View last output
oj session capture <session-id> --tail 50

# Send interrupt
oj session send <session-id> --interrupt

# If unresponsive, kill and restart
oj session kill <session-id>
\`\`\`

## WAL Corruption

**Symptoms**: Engine fails to start with "WAL checksum mismatch"

**Resolution**:
\`\`\`bash
# Try recovery mode
oj daemon start --recover

# If recovery fails, reset from snapshot
oj maintenance restore-snapshot --latest
\`\`\`
```

**Migration Script**:

```bash
#!/bin/bash
# scripts/migrate-from-bash.sh
#
# Migrate from bash scripts (feature, bugfix, mergeq) to oj

set -euo pipefail

echo "=== Migrating from Bash Scripts to oj ==="
echo ""

# Check for running bash-based processes
if pgrep -f "mergeq|feature-daemon" > /dev/null; then
    echo "WARNING: Found running bash processes. Stop them first:"
    echo "  pkill -f mergeq"
    echo "  pkill -f feature-daemon"
    exit 1
fi

# Check for existing state
BASH_STATE_DIR="${HOME}/.feature-state"
OJ_STATE_DIR="${PWD}/.build"

if [ -d "$BASH_STATE_DIR" ]; then
    echo "Found bash script state in $BASH_STATE_DIR"

    # Migrate queue items
    if [ -f "$BASH_STATE_DIR/merge-queue.json" ]; then
        echo "  Migrating merge queue..."
        oj queue import "$BASH_STATE_DIR/merge-queue.json" --format legacy
    fi

    # Note: Active worktrees managed by bash scripts are compatible
    echo "  Worktrees are compatible, no migration needed"

    # Backup and archive old state
    BACKUP_DIR="${BASH_STATE_DIR}.backup.$(date +%Y%m%d)"
    echo "  Backing up old state to $BACKUP_DIR"
    mv "$BASH_STATE_DIR" "$BACKUP_DIR"
fi

# Initialize oj state
echo ""
echo "Initializing oj state directory..."
mkdir -p "$OJ_STATE_DIR"
oj init

# Verify installation
echo ""
echo "Verifying installation..."
oj status

echo ""
echo "=== Migration Complete ==="
echo ""
echo "Next steps:"
echo "  1. Start the daemon: oj daemon start"
echo "  2. Run a test pipeline: oj run build --dry-run"
echo "  3. If all looks good, add shell aliases:"
echo "     alias feature='oj run feature'"
echo "     alias bugfix='oj run bugfix'"
echo "     alias mergeq='oj queue'"
```

**Verification**:
- [ ] Operational runbook covers all common operations
- [ ] Troubleshooting guide addresses likely issues
- [ ] Migration script handles existing bash state
- [ ] Documentation is accurate and complete

## Key Implementation Details

### Error Context Pattern

All errors should follow this pattern:

```rust
fn operation() -> Result<T, OjError> {
    inner_operation()
        .map_err(|e| OjError::new("Operation failed")
            .with_context(format!("Attempted to do X with {}", id))
            .with_context(format!("Current state: {}", state))
            .with_suggestion("Try Y instead")
            .with_source(e))
}
```

### Resource Limit Enforcement Points

| Resource | Enforcement Point | Action on Exceed |
|----------|-------------------|------------------|
| Sessions | `create_session()` | Return error |
| File handles | `open_file()` | Close LRU handles |
| Memory | Periodic check | Trigger compaction |
| WAL size | After write | Trigger compaction |

### Graceful Shutdown Sequence

1. Stop accepting new work (signal workers)
2. Wait for in-progress work (with timeout)
3. Checkpoint all sessions
4. Release all locks
5. Sync WAL
6. Exit

### Shell Completion Installation

```bash
# Bash
oj completions bash > ~/.local/share/bash-completion/completions/oj

# Zsh
oj completions zsh > ~/.zfunc/_oj

# Fish
oj completions fish > ~/.config/fish/completions/oj.fish
```

## Verification Plan

### Phase 1 Verification
- [ ] `./checks/coverage/measure.sh` produces coverage percentage
- [ ] Coverage report identifies gaps by module
- [ ] Test plan prioritizes based on coverage gaps

### Phase 2 Verification
- [ ] Edge case scenarios cover all failure modes
- [ ] Claude simulator tests pass
- [ ] Coverage reaches 90%+ for priority modules
- [ ] Overall coverage increased by 10%+ from baseline

### Phase 3 Verification
- [ ] All errors include context and suggestions
- [ ] Shell completions work for bash, zsh, fish
- [ ] Progress indicators display for long operations
- [ ] Help text is comprehensive with examples

### Phase 4 Verification
- [ ] Profiling setup documented and working
- [ ] No performance regressions from baseline
- [ ] Hot paths optimized where beneficial

### Phase 5 Verification
- [ ] SIGTERM triggers graceful shutdown
- [ ] Sessions checkpointed before exit
- [ ] Resource limits enforced
- [ ] No resource leaks after shutdown

### Phase 6 Verification
- [ ] Operational runbook is complete
- [ ] Troubleshooting guide addresses common issues
- [ ] Migration script handles all bash state
- [ ] All documentation reviewed for accuracy

### Final Verification
- [ ] `make check` passes
- [ ] Coverage >= 90% overall
- [ ] All 37 `unwrap()` calls audited
- [ ] No new `#[allow(dead_code)]` without justification
- [ ] `./checks/lint.sh` passes
- [ ] All documentation complete and accurate

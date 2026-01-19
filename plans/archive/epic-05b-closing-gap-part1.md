# Epic 5b: Closing the Gap

**Root Feature:** `oj-9376`

## Overview

Wire up the CLI to actually use the Engine, creating a working imperative shell that exercises the functional core built in Epics 1-5. This "proves the pudding" by running real pipelines end-to-end before adding runbook complexity in Epic 6.

The architecture is complete but disconnected:
- **Functional core** (Epics 1-2): State machines for Pipeline, Task, Queue, Lock, Semaphore, Guard
- **Engine & adapters** (Epic 3): Effect execution, recovery chains, feedback loop
- **Events & notifications** (Epic 4): Event bus, notification routing
- **Coordination** (Epic 5): Lock/semaphore primitives with heartbeats
- **Claude simulator** (Epics 5a-5d): Test infrastructure for integration testing

What's missing is the **wiring** - CLI commands directly manipulate JSON state instead of going through the Engine. This epic connects those dots, enabling end-to-end pipeline execution with real tmux sessions, git worktrees, and Claude (simulated or real).

**Key Deliverables:**
1. `RealAdapters` bundle combining TmuxAdapter, GitAdapter, WkAdapter, OsascriptNotifier
2. `oj run build/bugfix` actually spawns tmux sessions and starts tasks
3. `oj daemon` command for background polling and tick loops
4. `oj done/checkpoint` routes through Engine signals
5. Smoke test script for CI and manual validation

## Project Structure

```
crates/
├── cli/
│   ├── Cargo.toml                      # UPDATE: Add tokio full features
│   └── src/
│       ├── main.rs                     # UPDATE: Add daemon subcommand
│       ├── commands/
│       │   ├── mod.rs                  # UPDATE: Add daemon module
│       │   ├── run.rs                  # REWRITE: Use Engine instead of direct JSON
│       │   ├── signal.rs               # REWRITE: Route through Engine
│       │   └── daemon.rs               # NEW: Background polling loop
│       └── adapters.rs                 # NEW: RealAdapters implementation
│
├── core/
│   └── src/
│       ├── lib.rs                      # UPDATE: Export RealAdapters
│       ├── adapters/
│       │   ├── mod.rs                  # UPDATE: Export real_adapters module
│       │   └── real.rs                 # NEW: RealAdapters bundle
│       └── engine/
│           └── runtime.rs              # UPDATE: Add start_pipeline helper
│
scripts/
└── smoke-test.sh                       # NEW: E2E validation script

tests/
├── integration/                        # NEW: Integration test directory
│   ├── pipeline_lifecycle.rs           # NEW: Full pipeline tests with claude-sim
│   ├── daemon_polling.rs               # NEW: Daemon tick loop tests
│   └── signal_handling.rs              # NEW: Done/checkpoint signal tests
└── e2e/                                # NEW: Bats E2E tests
    ├── cli_commands.bats               # NEW: CLI behavior tests
    └── daemon_lifecycle.bats           # NEW: Daemon start/stop tests
```

## Dependencies

### CLI Crate Updates

```toml
[dependencies]
oj-core = { path = "../core" }
tokio = { version = "1", features = ["full", "signal"] }  # Add signal feature
ctrlc = "3"                                               # Graceful shutdown
```

### Dev Dependencies (workspace level)

```toml
[workspace.dev-dependencies]
assert_cmd = "2"    # CLI testing
predicates = "3"    # Assertion matchers
```

### External Tools

- `bats-core` for E2E shell tests (install via homebrew: `brew install bats-core`)
- `claude-sim` from Epic 5a-5d (in-workspace)

## Implementation Phases

### Phase 1: RealAdapters Bundle

**Goal**: Create a production `Adapters` implementation that bundles real adapters together.

**Deliverables**:
1. `RealAdapters` struct implementing `Adapters` trait
2. Unit tests verifying trait implementation
3. Documentation for adapter configuration

**Key Code**:

```rust
// crates/core/src/adapters/real.rs

use crate::adapters::{...};
use crate::engine::executor::Adapters;
use std::path::PathBuf;

/// Production adapters bundle for real I/O operations
#[derive(Clone)]
pub struct RealAdapters {
    sessions: TmuxAdapter,
    repos: GitAdapter,
    issues: WkAdapter,
    notify: OsascriptNotifier,
}

impl RealAdapters {
    /// Create adapters with default configuration
    pub fn new() -> Self { /* init all adapters with defaults */ }

    /// Create adapters for a specific repository root
    pub fn with_repo_root(root: PathBuf) -> Self { /* init with custom repo root */ }
}

impl Default for RealAdapters { fn default() -> Self { Self::new() } }

impl Adapters for RealAdapters {
    type Sessions = TmuxAdapter;
    type Repos = GitAdapter;
    type Issues = WkAdapter;
    type Notify = OsascriptNotifier;

    fn sessions(&self) -> Self::Sessions { self.sessions.clone() }
    fn repos(&self) -> Self::Repos { self.repos.clone() }
    fn issues(&self) -> Self::Issues { self.issues.clone() }
    fn notify(&self) -> Self::Notify { self.notify.clone() }
}

// Tests: verify trait implementation compiles and all adapters accessible
```

**Verification**:
- `RealAdapters::new()` compiles and creates valid adapter instances
- Trait bounds satisfied for `Engine<RealAdapters, SystemClock>`
- `cargo test -p oj-core adapters::real` passes

---

### Phase 2: Wire Up `oj run`

**Goal**: Replace direct JSON manipulation with Engine-driven pipeline creation and session spawning.

**Deliverables**:
1. Rewrite `run_build` to use Engine
2. Rewrite `run_bugfix` to use Engine
3. Engine creates workspace, spawns tmux session, starts task
4. Environment variables set in spawned session

**Key Code**:

```rust
// crates/cli/src/commands/run.rs (rewritten)

use crate::adapters::make_engine;
use anyhow::Result;
use clap::Subcommand;
use oj_core::pipeline::{Pipeline, PipelineEvent};
use oj_core::workspace::Workspace;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum RunCommand {
    /// Start a build pipeline
    Build {
        /// Name for this build
        name: String,
        /// Prompt describing what to build
        prompt: String,
    },
    /// Start a bugfix pipeline
    Bugfix {
        /// Issue ID to fix
        id: String,
    },
}

pub async fn handle(command: RunCommand) -> Result<()> {
    match command {
        RunCommand::Build { name, prompt } => run_build(name, prompt).await,
        RunCommand::Bugfix { id } => run_bugfix(id).await,
    }
}

async fn run_build(name: String, prompt: String) -> Result<()> {
    // 1. make_engine() and load()
    // 2. Create workspace at .worktrees/build-{name} with branch build-{name}
    // 3. Create build pipeline with workspace attached
    // 4. Generate CLAUDE.md in workspace
    // 5. Create git worktree via adapters.repos().worktree_add()
    // 6. Start phase task (spawns tmux session)
    // 7. Print status: pipeline name, workspace path, branch, task id, attach command
}

async fn run_bugfix(id: String) -> Result<()> {
    // Same pattern as run_build but:
    // - workspace at .worktrees/bugfix-{id}
    // - branch bugfix-{id}
    // - Pipeline::new_bugfix() instead of new_build()
    // - generate_bugfix_claude_md() for CLAUDE.md
}

fn generate_claude_md(name: &str, prompt: &str, pipeline_id: &str) -> String {
    // Template with: task name, prompt, signaling instructions (oj done),
    // environment vars (OTTER_PIPELINE, OTTER_TASK, OTTER_PHASE), guidelines
}

fn generate_bugfix_claude_md(issue_id: &str, pipeline_id: &str) -> String {
    // Template with: issue reference, signaling instructions,
    // environment vars, bugfix-specific guidelines
}
```

```rust
// crates/cli/src/adapters.rs (new file)

use anyhow::Result;
use oj_core::{adapters::RealAdapters, clock::SystemClock, engine::Engine, storage::JsonStore};

/// Create a production engine with real adapters
pub fn make_engine() -> Result<Engine<RealAdapters, SystemClock>> {
    // JsonStore::open(".build/operations"), RealAdapters::new(), SystemClock
    // -> Engine::new(adapters, store, clock)
}
```

**Verification**:
- `oj run build test "hello"` creates workspace, pipeline, and tmux session
- `tmux list-sessions` shows `oj-test-init` session
- `.build/operations/pipelines/build-test/state.json` exists
- `.worktrees/build-test/CLAUDE.md` generated with correct content

---

### Phase 3: Wire Up `oj done/checkpoint`

**Goal**: Route signal commands through Engine instead of direct store manipulation.

**Deliverables**:
1. Rewrite `handle_done` to use `engine.signal_done()`
2. Rewrite `handle_checkpoint` to use `engine.signal_checkpoint()`
3. Proper error handling and feedback

**Key Code**:

```rust
// crates/cli/src/commands/signal.rs (rewritten)

use crate::adapters::make_engine;
use anyhow::{bail, Result};
use oj_core::workspace::WorkspaceId;

pub async fn handle_done(error: Option<String>) -> Result<()> {
    // 1. make_engine() and load()
    // 2. detect_workspace_id()
    // 3. engine.signal_done(&workspace_id, error)
    // 4. find_pipeline_by_workspace() for status message
    // 5. Print: phase failed (with reason) OR phase complete (with next phase info)
}

pub async fn handle_checkpoint() -> Result<()> {
    // 1. make_engine() and load()
    // 2. detect_workspace_id()
    // 3. engine.signal_checkpoint(&workspace_id)
    // 4. Print: checkpoint saved for pipeline at phase
}

/// Detect workspace ID from environment or current directory
fn detect_workspace_id() -> Result<WorkspaceId> {
    // Priority order:
    // 1. OTTER_PIPELINE env var
    // 2. OTTER_TASK env var
    // 3. If cwd parent is .worktrees, use dir name
    // 4. Scan up to 5 parent dirs for .build/operations/pipelines, extract workspace from path
    // 5. bail!() if none found
}
```

**Verification**:
- `oj done` from workspace directory advances pipeline phase
- `oj done --error "oops"` fails current task and pipeline phase
- `oj checkpoint` saves checkpoint without advancing phase
- Environment variables are correctly detected

---

### Phase 4: Add `oj daemon` Command

**Goal**: Implement background daemon for polling sessions, ticking tasks, and processing queue.

**Deliverables**:
1. `oj daemon` command with configurable intervals
2. Graceful shutdown on SIGINT/SIGTERM
3. Logging of daemon activity
4. Session heartbeat detection
5. Stuck task handling

**Key Code**:

```rust
// crates/cli/src/commands/daemon.rs (new file)

use crate::adapters::make_engine;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

#[derive(clap::Args)]
pub struct DaemonArgs {
    /// Interval for polling sessions (seconds)
    #[arg(long, default_value = "5")]
    poll_interval: u64,

    /// Interval for ticking tasks (seconds)
    #[arg(long, default_value = "30")]
    tick_interval: u64,

    /// Interval for ticking queue (seconds)
    #[arg(long, default_value = "10")]
    queue_interval: u64,

    /// Run once and exit (for testing)
    #[arg(long)]
    once: bool,
}

pub async fn handle(args: DaemonArgs) -> Result<()> {
    // 1. Setup ctrlc handler with AtomicBool for graceful shutdown
    // 2. Print startup message with intervals
    // 3. make_engine() and load()
    // 4. Create tokio interval timers for poll/tick/queue
    // 5. If args.once: run_once() and return
    // 6. Main loop with tokio::select! on three timers:
    //    - poll_timer -> engine.poll_sessions()
    //    - tick_timer -> engine.tick_all_tasks()
    //    - queue_timer -> engine.tick_queue("merge")
    // 7. Break loop when running flag is false
}

async fn run_once(engine: &mut Engine<RealAdapters, SystemClock>) -> Result<()> {
    // Single iteration: poll_sessions, tick_all_tasks, tick_queue
}
```

```rust
// crates/cli/src/main.rs (update)

mod adapters;
mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "oj", version, about = "Otter Jobs - Agentic development orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true, default_value = "text")]
    format: String,
}

#[derive(Subcommand)]
enum Commands {
    Run { #[command(subcommand)] command: commands::run::RunCommand },
    Pipeline { #[command(subcommand)] command: commands::pipeline::PipelineCommand },
    Workspace { #[command(subcommand)] command: commands::workspace::WorkspaceCommand },
    Session { #[command(subcommand)] command: commands::session::SessionCommand },
    Queue { #[command(subcommand)] command: commands::queue::QueueCommand },
    Done { #[arg(long)] error: Option<String> },
    Checkpoint,
    Daemon(commands::daemon::DaemonArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI and dispatch to appropriate command handler
}
```

**Verification**:
- `oj daemon` starts and logs activity
- `oj daemon --once` runs single iteration and exits
- Ctrl+C gracefully stops daemon
- Daemon detects stuck tasks and attempts recovery
- Daemon detects dead sessions

---

### Phase 5: Integration Tests with Claude Simulator

**Goal**: Create comprehensive integration tests using claude-sim for deterministic testing.

**Deliverables**:
1. Integration test harness with claude-sim in PATH
2. Full pipeline lifecycle tests
3. Signal handling tests
4. Error injection tests
5. Concurrent pipeline tests

**Key Code**:

```rust
// tests/integration/pipeline_lifecycle.rs

use assert_cmd::Command;
use predicates::prelude::*;
use std::{env, fs};
use tempfile::TempDir;

/// Setup test environment with claude-sim in PATH
fn setup_test_env() -> TempDir {
    // Create temp dir, add target/debug to PATH, create .build/operations
}

#[tokio::test]
async fn test_build_pipeline_creates_workspace_and_session() {
    // oj run build test-feature "..." -> success
    // Assert: .worktrees/build-test-feature exists
    // Assert: CLAUDE.md contains prompt
    // Assert: .build/operations/pipelines/.../state.json exists
}

#[tokio::test]
async fn test_done_signal_advances_pipeline() {
    // Create pipeline, then oj done from workspace with OTTER_PIPELINE set
    // Assert: stdout contains "phase complete"
}

#[tokio::test]
async fn test_done_with_error_fails_pipeline() {
    // Create pipeline, then oj done --error "..." from workspace
    // Assert: stdout contains "phase failed"
}

#[tokio::test]
async fn test_daemon_single_iteration() {
    // Create pipeline, then oj daemon --once
    // Assert: stdout contains "Running single daemon iteration"
}
```

```bash
# tests/e2e/cli_commands.bats
#!/usr/bin/env bats

setup()    { /* mktemp, cd, mkdir .build/operations, git init, add target/debug to PATH */ }
teardown() { rm -rf "$TEMP_DIR" }

@test "oj --help shows usage"                    { /* status 0, output contains "Otter Jobs" */ }
@test "oj run build creates pipeline"            { /* status 0, state.json exists */ }
@test "oj pipeline list shows pipelines"         { /* run build first, then list shows it */ }
@test "oj done without workspace fails gracefully" { /* status != 0, "Could not detect workspace" */ }
@test "oj daemon --once completes successfully"  { /* run build first, daemon --once succeeds */ }
```

**Verification**:
- `cargo test --test pipeline_lifecycle` passes
- `cargo test --test signal_handling` passes
- `bats tests/e2e/cli_commands.bats` passes

---

### Phase 6: Smoke Test Script

**Goal**: Create a comprehensive smoke test script for CI and manual validation.

**Deliverables**:
1. `scripts/smoke-test.sh` accepting `--model simulated` or `--model haiku`
2. Tests full pipeline lifecycle
3. Clear pass/fail output
4. Cleanup on exit

**Key Code**:

```bash
#!/usr/bin/env bash
# scripts/smoke-test.sh - E2E smoke test for oj
# Usage: ./scripts/smoke-test.sh --model [simulated|haiku]

set -euo pipefail

# Parse args: --model simulated (CI) or --model haiku (manual)
# Setup: TEMP_DIR, cleanup trap, git init, .build/operations
# If simulated: add target/debug to PATH, set CLAUDE_SIM_RESPONSE
# Build oj if needed

# Test 1: oj run build smoke-test "Create a hello world program"
# Test 2: Verify .build/operations/pipelines/build-smoke-test/state.json exists
# Test 3: Verify .worktrees/build-smoke-test exists
# Test 4: Verify CLAUDE.md exists and contains "hello world"
# Test 5: cd to workspace, oj done (with OTTER_PIPELINE set)
# Test 6: oj daemon --once
# Test 7: oj pipeline list contains "smoke-test"
# Test 8 (haiku only): tmux capture-pane to verify Claude responded

# Print ALL TESTS PASSED or exit 1 on failure
```

**Verification**:
- `./scripts/smoke-test.sh --model simulated` passes in CI
- `./scripts/smoke-test.sh --model haiku` passes with real Claude (manual)
- Script cleans up temp files and tmux sessions

## Key Implementation Details

### Engine-CLI Integration

The CLI creates and uses an Engine for all stateful operations:

```rust
// Pattern: make_engine()? -> engine.load()? -> engine operations -> auto-persist
```

### Adapter Bundle Pattern

`RealAdapters` implements `Adapters` trait with associated types for each adapter.

### Daemon Polling Architecture

The daemon uses tokio's interval timers with select:

```rust
loop {
    tokio::select! {
        _ = poll_timer.tick() => engine.poll_sessions().await?,
        _ = tick_timer.tick() => engine.tick_all_tasks().await?,
        _ = queue_timer.tick() => engine.tick_queue("merge")?,
    }
}
```

### Testing Pyramid

| Layer | Tool | Scope | Speed |
|-------|------|-------|-------|
| Unit | cargo test | State machines, effects | Fast |
| Integration | claude-sim | Full CLI with fake Claude | Medium |
| E2E | bats | Shell-level behavior | Medium |
| Manual | haiku | Real Claude validation | Slow |

## Verification Plan

### Unit Tests

Run with: `cargo test -p oj-core -p oj-cli`

| Module | Key Tests |
|--------|-----------|
| `adapters::real` | Trait implementation, construction |
| `cli::adapters` | Engine factory function |

### Integration Tests

Run with: `cargo test --test '*'` (requires `cargo build` first for claude-sim)

| Test File | Description |
|-----------|-------------|
| `pipeline_lifecycle` | Create → run → signal → complete |
| `daemon_polling` | Daemon tick loops |
| `signal_handling` | Done/checkpoint routing |

### E2E Tests

Run with: `bats tests/e2e/`

| Test File | Description |
|-----------|-------------|
| `cli_commands.bats` | Basic CLI behavior |
| `daemon_lifecycle.bats` | Daemon start/stop |

### Smoke Test

```bash
# CI (simulated)
./scripts/smoke-test.sh --model simulated

# Manual validation (real Claude)
./scripts/smoke-test.sh --model haiku
```

### Manual Verification Checklist

- [ ] `cargo build --all` succeeds
- [ ] `make check` passes
- [ ] `oj run build test "hello"` creates workspace and tmux session
- [ ] `tmux attach -t oj-test-init` shows Claude session
- [ ] `oj done` advances pipeline (from workspace)
- [ ] `oj daemon --once` completes without error
- [ ] `./scripts/smoke-test.sh --model simulated` passes
- [ ] Ctrl+C gracefully stops daemon
- [ ] No regressions in existing CLI commands

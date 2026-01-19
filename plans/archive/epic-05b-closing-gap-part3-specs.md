# Epic 5b Part 3: Bats E2E Specs

**Root Feature:** `oj-881e`

## Overview

Add shell-level E2E tests using bats (Bash Automated Testing System) with local installation (no homebrew dependency). Also includes manual validation runbook for testing against real Claude.

## Project Structure

```
tests/
└── specs/
    ├── bats/
    │   ├── install.sh          # Download pinned bats versions
    │   ├── .gitkeep
    │   ├── bats-core/          # Downloaded: v1.13.0
    │   ├── bats-support/       # Downloaded: v0.3.0
    │   └── bats-assert/        # Downloaded: v2.1.0
    ├── helpers/
    │   └── common.bash         # Shared test utilities
    ├── unit/
    │   ├── cli_help.bats       # Help/version behavior
    │   └── cli_args.bats       # Argument validation
    ├── integration/
    │   ├── pipeline_create.bats  # Pipeline creation
    │   ├── pipeline_signal.bats  # Done/checkpoint signals
    │   └── daemon_lifecycle.bats # Daemon start/stop
    └── edge_cases/
        └── error_handling.bats # Error conditions

scripts/
├── spec                        # Bats runner wrapper
└── init-worktree               # Copies bats libs to worktrees

```

## Implementation Phases

### Phase 5.1: Bats Installation Script

**New file: `tests/specs/bats/install.sh`**

Downloads pinned versions for reproducible builds:
- bats-core v1.13.0
- bats-support v0.3.0
- bats-assert v2.1.0

```bash
#!/usr/bin/env bash
# Download and extract each library from github releases
# Skip if already installed
# Use curl or wget (whichever available)
```

**New file: `tests/specs/bats/.gitkeep`** (empty)

**Update `.gitignore`:**

```gitignore
# Downloaded BATS testing libraries (run scripts/spec to auto-install)
tests/specs/bats/bats-core/
tests/specs/bats/bats-support/
tests/specs/bats/bats-assert/
```

---

### Phase 5.1b: Worktree Init Script

Copies cached bats libraries to worktrees to avoid re-downloading.

**New file: `scripts/init-worktree`**

```bash
#!/usr/bin/env bash
# Used with V0_WORKTREE_INIT hook
#
# Environment:
#   V0_CHECKOUT_DIR - Path to main checkout
#   V0_WORKTREE_DIR - Path to new worktree
#
# If bats not in main checkout, run install.sh
# Copy bats-core, bats-support, bats-assert to worktree
```

**Update `.v0.rc`** (if exists):

```bash
V0_WORKTREE_INIT='scripts/init-worktree'
```

---

### Phase 5.2: Common Test Helpers

**New file: `tests/specs/helpers/common.bash`**

```bash
#!/usr/bin/env bash
# Shared test utilities for oj specs

# Compute paths relative to this file
HELPERS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$(dirname "$HELPERS_DIR")"
PROJECT_ROOT="$(cd "$SPECS_DIR/../.." && pwd)"

# Load bats libraries
load "$SPECS_DIR/bats/bats-support/load"
load "$SPECS_DIR/bats/bats-assert/load"

# Build binaries (don't auto-detect - just build them)
cargo build -p oj-cli -p claudeless --quiet 2>/dev/null || true
export OJ_BIN="$PROJECT_ROOT/target/debug/oj"
export CLAUDELESS_BIN="$PROJECT_ROOT/target/debug/claudeless"

# File-level setup: create temp dir, init git, add binaries to PATH
file_setup() { ... }

# File-level teardown: kill oj-* tmux sessions, remove temp dir
file_teardown() { ... }

# Per-test setup: reset to temp dir
test_setup() { ... }

# Initialize git repo (idempotent)
init_git_repo() { ... }

# Initialize oj project structure (git + .build/operations)
init_oj_project() { ... }

# Create claudeless scenario file
# Usage: path=$(create_scenario "name" "content")
create_scenario() { ... }

# Create auto-done scenario
create_auto_done_scenario() { ... }

# Assert file exists
assert_file_exists() { ... }

# Assert directory exists
assert_dir_exists() { ... }

# Assert JSON field equals value (requires jq)
assert_json_field() { ... }

# Default setup/teardown hooks
setup_file() { file_setup; init_oj_project; }
teardown_file() { file_teardown; }
setup() { test_setup; }
```

---

### Phase 5.3: Spec Runner Script

**New file: `scripts/spec`**

Simple runner - runs all specs with automatic parallelism (like wok).

```bash
#!/usr/bin/env bash
# Run BATS specs
#
# Usage: scripts/spec [options] [file...]
#
# Options:
#   --filter <regex>   Filter tests by name
#   --timeout <secs>   Test timeout (default: 5)
#
# Examples:
#   scripts/spec                           # Run all specs
#   scripts/spec tests/specs/unit/*.bats   # Run specific files

# Build binaries first (don't auto-detect)
cargo build -p oj-cli -p claudeless --quiet

# Install bats if needed
# Set BATS_LIB_PATH
# Auto-parallel: calculate jobs as 80% of CPUs
# Run bats with --jobs N --recursive on tests/specs/
```

---

### Phase 5.4: Unit Specs

Test CLI interface which is durable infrastructure.

**New file: `tests/specs/unit/cli_help.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

@test "oj --help exits 0" { ... }
@test "oj --help shows Otter Jobs" { ... }
@test "oj --version exits 0" { ... }
@test "oj run --help shows subcommands" { ... }
@test "oj daemon --help shows interval options" { ... }
@test "oj pipeline --help shows subcommands" { ... }
@test "oj workspace --help shows subcommands" { ... }
@test "oj session --help shows subcommands" { ... }
@test "oj queue --help shows subcommands" { ... }
```

**New file: `tests/specs/unit/cli_args.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

# Signal commands
@test "oj done without context fails gracefully" { ... }
@test "oj done shows helpful error message" { ... }
@test "oj checkpoint without context fails gracefully" { ... }

# Daemon commands
@test "oj daemon accepts --once flag" { ... }
@test "oj daemon accepts --poll-interval" { ... }
@test "oj daemon accepts --tick-interval" { ... }

# List commands (durable)
@test "oj pipeline list works without pipelines" { ... }
@test "oj workspace list works without workspaces" { ... }
@test "oj session list works without sessions" { ... }
```

---

### Phase 5.5: Integration Specs

Test durable CLI/infrastructure behavior. Session, workspace, and daemon are critical.

**New file: `tests/specs/integration/pipeline_create.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

# Core infrastructure (durable)
@test "oj run build creates workspace directory" { ... }
@test "oj run build spawns tmux session" { ... }
@test "oj run build creates pipeline state file" { ... }
@test "oj pipeline list shows created pipelines" { ... }
@test "oj pipeline show displays pipeline details" { ... }
@test "multiple pipelines can coexist" { ... }

# Spot check hardcoded pipelines (minimal, Epic 6 replaces)
@test "oj run build works end-to-end" { ... }
@test "oj run bugfix works end-to-end" { ... }
```

**New file: `tests/specs/integration/pipeline_signal.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

@test "oj done with OTTER_PIPELINE succeeds" { ... }
@test "oj done autodetects pipeline from workspace" { ... }
@test "oj done --error marks pipeline failed" { ... }
@test "oj done --error records error message" { ... }
@test "oj checkpoint saves state" { ... }
@test "oj checkpoint updates heartbeat" { ... }
```

**New file: `tests/specs/integration/session_management.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

# Session listing
@test "oj session list shows active sessions" { ... }
@test "oj session list empty when no sessions" { ... }
@test "oj session list shows multiple sessions" { ... }

# Session inspection
@test "oj session show displays session details" { ... }
@test "oj session show nonexistent fails gracefully" { ... }

# Session control
@test "oj session kill terminates session" { ... }
@test "oj session nudge sends input to session" { ... }

# Session naming
@test "session naming follows oj-{type}-{name}-{phase} pattern" { ... }
```

**New file: `tests/specs/integration/workspace_management.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

@test "oj workspace list shows workspaces" { ... }
@test "oj workspace list empty when no workspaces" { ... }
@test "oj workspace show displays workspace details" { ... }
@test "workspace is valid git worktree" { ... }
@test "workspace contains CLAUDE.md" { ... }
```

**New file: `tests/specs/integration/daemon_lifecycle.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

setup_file() { export BATS_TEST_TIMEOUT=10; file_setup; init_oj_project; }

# Basic operation
@test "daemon --once completes successfully" { ... }
@test "daemon --once with no pipelines exits cleanly" { ... }
@test "daemon --once with pipeline processes it" { ... }

# Logging
@test "daemon logs startup message" { ... }
@test "daemon logs interval configuration" { ... }

# Signal handling
@test "daemon responds to SIGINT" { ... }
@test "daemon responds to SIGTERM" { ... }

# Custom intervals
@test "daemon accepts --poll-interval" { ... }
@test "daemon accepts --tick-interval" { ... }
```

---

### Phase 5.6: Edge Case Specs

Edge cases and error handling are critical for production reliability.

**New file: `tests/specs/edge_cases/error_handling.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

# Input validation
@test "oj run build with invalid characters in name fails gracefully" { ... }
@test "oj run build with empty name fails" { ... }
@test "oj run build with spaces in name handles correctly" { ... }

# Context errors
@test "oj done from wrong directory fails gracefully" { ... }
@test "oj done with nonexistent pipeline fails gracefully" { ... }
@test "oj checkpoint from project root fails gracefully" { ... }

# State errors
@test "oj handles missing .build directory" { ... }
@test "oj handles corrupted state file" { ... }
@test "oj handles missing workspace directory" { ... }

# Git errors
@test "oj works in non-git directory fails gracefully" { ... }
@test "oj works in repo without commits fails gracefully" { ... }

# Concurrency edge cases
@test "duplicate pipeline names are rejected" { ... }
@test "rapid sequential pipeline creates work" { ... }
```

**New file: `tests/specs/edge_cases/session_failures.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

@test "oj session kill nonexistent session fails gracefully" { ... }
@test "oj session nudge nonexistent session fails gracefully" { ... }
@test "daemon detects manually killed session" { ... }
@test "daemon handles session that exits immediately" { ... }
```

**New file: `tests/specs/edge_cases/claudeless_integration.bats`**

```bash
#!/usr/bin/env bats
load '../helpers/common'

setup_file() {
    file_setup
    init_oj_project
    export CLAUDELESS_SCENARIO=$(create_auto_done_scenario)
}

@test "pipeline with claudeless scenario completes" { ... }
@test "claudeless receives prompt from CLAUDE.md" { ... }
@test "claudeless can signal oj done" { ... }
@test "claudeless failure scenario triggers error state" { ... }
```

---

### Phase 5.7: ShellCheck Linting

**Add to `Makefile`:**

```makefile
.PHONY: spec lint-specs

spec:
	@./scripts/spec

lint-specs:
	@shellcheck -x -S warning tests/specs/helpers/*.bash
	@shellcheck -x -S warning scripts/spec
```

---

### Phase 5.8: GitHub CI Integration

**Update `.github/workflows/ci.yml`:**

```yaml
jobs:
  bats:
    name: Bats Specs
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-action@stable
      - run: cargo build --release
      - run: ./tests/specs/bats/install.sh
      - run: ./scripts/spec --parallel

  shellcheck:
    name: ShellCheck
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get install -y shellcheck
      - run: |
          shellcheck -x -S warning scripts/spec
          shellcheck -x -S warning tests/specs/helpers/*.bash
```

---

## Verification Plan

```bash
# Install bats
./tests/specs/bats/install.sh

# Build binaries (required)
cargo build -p oj-cli -p claudeless

# Run all specs
./scripts/spec

# Run with parallel
./scripts/spec --parallel

# Run specific suite
./scripts/spec unit
./scripts/spec integration

# Lint
make lint-specs

# Manual validation
# Follow docs/TEST_HAIKU_RUNBOOK.md
```

## Dependencies

### External Tools

- `tmux`: Already required
- `jq`: For manual testing and JSON assertions (`brew install jq`)
- `shellcheck`: For linting (`brew install shellcheck` or apt-get)

## Success Criteria

- [ ] `./tests/specs/bats/install.sh` downloads pinned versions
- [ ] `./scripts/spec` runs all specs with automatic parallelism
- [ ] Session management specs cover `oj session` commands (8 tests)
- [ ] Workspace management specs cover `oj workspace` commands (5 tests)
- [ ] Daemon lifecycle specs cover `oj daemon` behavior (9 tests)
- [ ] Edge case specs cover error handling (17 tests)
- [ ] Spot checks for `oj done/checkpoint` (6 tests) and hardcoded pipelines (2 tests)
- [ ] Total: 55+ bats specs
- [ ] `make lint-specs` passes shellcheck
- [ ] GitHub CI runs bats and shellcheck
- [ ] `make check` passes

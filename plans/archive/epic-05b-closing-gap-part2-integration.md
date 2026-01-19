# Epic 5b Part 2: Rust Integration Tests with Claudeless

**Root Feature:** `oj-9376` (continuation)

## Overview

Wire up the CLI integration tests to use `claudeless` (the Claude simulator) so they verify actual behavior through the Engine/adapter layer, not just CLI parsing.

## Current State

**What exists:**
- 74 CLI integration tests in `crates/cli/tests/`
- `claudeless` simulator installed globally (assumed in PATH)
- Scenario TOML files in `crates/cli/tests/scenarios/`

**Completed:**
- [ ] Test infrastructure with claudeless helpers
- [ ] Tmux session management helpers
- [ ] Failure injection tests
- [ ] Concurrent pipeline tests
- [ ] Session management tests
- [ ] Workspace management tests

## Project Structure

```
crates/cli/
├── tests/
│   ├── common/
│   │   ├── mod.rs              # Shared test utilities
│   │   ├── claudeless.rs       # Simulator setup helpers
│   │   └── tmux.rs             # Tmux assertion helpers
│   ├── scenarios/              # Claudeless scenario TOML files
│   │   ├── simple.toml
│   │   ├── auto-done.toml
│   │   ├── network-failure.toml
│   │   ├── auth-failure.toml
│   │   ├── rate-limit.toml
│   │   ├── timeout.toml
│   │   ├── malformed.toml
│   │   └── transient-failure.toml
│   ├── pipeline_lifecycle.rs
│   ├── signal_handling.rs
│   ├── daemon_polling.rs
│   ├── failure_injection.rs
│   ├── concurrent_pipelines.rs
│   ├── session_management.rs
│   └── workspace_management.rs
```

## Implementation Details

### Claudeless Integration

Claudeless is assumed to be globally installed. The test helpers:

1. Find claudeless via `which claudeless`
2. Create a temp directory with `claude` -> `claudeless` symlink
3. Prepend that directory to PATH for tests

```rust
/// Get path to the globally installed claudeless binary.
/// Uses `which claudeless` to find it in PATH.
pub fn claudeless_bin() -> PathBuf { ... }

/// Setup environment PATH with claude symlink directory prepended.
/// Returns the modified PATH string.
pub fn setup_claudeless_path() -> String { ... }
```

### Scenario Files

Scenarios are external TOML files loaded at test time:

```rust
/// Copy a scenario file to the test directory.
fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let src = scenarios_dir().join(format!("{}.toml", name));
    let dst = dir.join(format!("{}.toml", name));
    fs::copy(&src, &dst).unwrap();
    dst
}

pub fn simple_scenario(dir: &Path) -> PathBuf { copy_scenario(dir, "simple") }
pub fn network_failure_scenario(dir: &Path) -> PathBuf { copy_scenario(dir, "network-failure") }
// ... etc
```

### Test Pattern

```rust
mod common;
use common::{claudeless, tmux, setup_test_env, unique_id};

#[test]
fn test_example() {
    // Skip if claudeless not installed
    if !claudeless::is_claudeless_available() {
        eprintln!("Skipping: claudeless not found in PATH.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("test-{}", id);

    // Cleanup guard kills sessions on test exit (pass or fail)
    let _guard = tmux::SessionGuard::new(&format!("oj-build-{}", name));

    // Setup claudeless
    let scenario = claudeless::auto_done_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args(["run", "build", &name, "Test prompt"])
        .assert()
        .success();

    // Assert tmux session was created
    assert!(tmux::session_matches(&format!("oj-build-{}", name)));
}
```

## Verification Plan

```bash
# Ensure claudeless is installed globally
which claudeless

# Build oj
cargo build -p oj

# Run integration tests
cargo test -p oj --test pipeline_lifecycle
cargo test -p oj --test signal_handling
cargo test -p oj --test daemon_polling
cargo test -p oj --test failure_injection
cargo test -p oj --test concurrent_pipelines
cargo test -p oj --test session_management
cargo test -p oj --test workspace_management

# Full check
make check
```

## Success Criteria

- [ ] All existing tests still pass
- [ ] `cargo test -p oj` runs 74 tests
- [ ] Session management tests cover `oj session` commands (15 tests)
- [ ] Workspace management tests cover `oj workspace` commands (6 tests)
- [ ] Daemon tests cover `oj daemon` behavior (15 tests)
- [ ] Failure injection tests cover error handling (8 tests)
- [ ] Concurrent pipeline tests verify isolation (7 tests)
- [ ] Tests use claudeless with external scenario files
- [ ] Tests use SessionGuard for cleanup
- [ ] Tests skip gracefully if claudeless not installed
- [ ] `make check` passes

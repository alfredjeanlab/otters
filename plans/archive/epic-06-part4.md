# Plan: Fix CLI Integration Tests for Runbook-Only Pipelines

**Root Feature:** `otters-42cb`

## Overview

Address the gap from epic-06-part3: CLI integration tests fail because they run in temp directories without runbooks.

## Problem

After commit 0065dd5, the `oj run` command requires runbooks to exist. CLI integration tests create temp directories with git repos but don't include runbooks, causing 7 test failures:

```
Error: No runbooks loaded. Ensure runbooks directory exists and contains valid runbook files.
```

Failing tests in `crates/cli/tests/`:
- `concurrent_pipelines.rs` (7 tests)
- `daemon_polling.rs`
- `failure_injection.rs`
- `pipeline_lifecycle.rs`
- `session_management.rs`
- `signal_handling.rs`
- `workspace_management.rs`

## Root Cause

`setup_test_env()` in `crates/cli/tests/common/mod.rs` creates a temp git repo but doesn't copy the `runbooks/` directory.

## Changes

### Phase 1: Update Test Setup to Include Runbooks

**File: `crates/cli/tests/common/mod.rs`**

Update `setup_test_env()` to copy runbooks to the temp directory:

```rust
/// Setup test environment with initialized git repo, .build/operations directory,
/// and runbooks for CLI testing.
pub fn setup_test_env() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp directory");

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to init git");

    // ... existing git config ...

    // Create .build/operations directory
    fs::create_dir_all(temp.path().join(".build/operations"))
        .expect("Failed to create operations dir");

    // Copy runbooks directory for CLI tests
    copy_runbooks(temp.path());

    temp
}

/// Copy runbooks from the project root to the test directory.
fn copy_runbooks(dest: &Path) {
    let runbooks_dir = dest.join("runbooks");
    fs::create_dir_all(&runbooks_dir).expect("Failed to create runbooks dir");

    // Get the project root (where the actual runbooks live)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = Path::new(manifest_dir).parent().unwrap().parent().unwrap();
    let source_runbooks = project_root.join("runbooks");

    // Copy each runbook file
    if source_runbooks.exists() {
        for entry in fs::read_dir(&source_runbooks).expect("Failed to read runbooks dir") {
            let entry = entry.expect("Failed to read entry");
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                let dest_file = runbooks_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest_file).expect("Failed to copy runbook");
            }
        }
    }
}
```

### Phase 2: Verify All CLI Tests Pass

Run the full test suite to verify the fix:

```bash
cargo test -p oj --test concurrent_pipelines
cargo test -p oj --test daemon_polling
cargo test -p oj --test failure_injection
cargo test -p oj --test pipeline_lifecycle
cargo test -p oj --test session_management
cargo test -p oj --test signal_handling
cargo test -p oj --test workspace_management
```

## File Summary

| File | Action |
|------|--------|
| `crates/cli/tests/common/mod.rs` | Add `copy_runbooks()` helper, update `setup_test_env()` |

## Verification

```bash
./checks/lint.sh
make check
```

All checks must pass:
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all` (including all CLI integration tests)
- [ ] `cargo build --all`

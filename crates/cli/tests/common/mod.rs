// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Shared test utilities for CLI integration tests.

#![allow(dead_code)]

pub mod claudeless;
pub mod tmux;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Generate a unique test identifier using timestamp + atomic counter.
/// This ensures uniqueness even with parallel test execution.
pub fn unique_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", timestamp % 1_000_000, counter)
}

/// Setup test environment with initialized git repo, .build/operations directory,
/// and runbooks for CLI testing.
/// Returns a TempDir that will be cleaned up when dropped.
pub fn setup_test_env() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp directory");

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to init git");

    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to configure git email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to configure git name");

    // Create initial commit
    fs::write(temp.path().join("README.md"), "# Test").expect("Failed to write README");
    std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to add README");

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit", "--quiet"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to commit");

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

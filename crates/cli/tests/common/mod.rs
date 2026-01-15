// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Shared test utilities for CLI integration tests.

#![allow(dead_code)]

pub mod claudeless;
pub mod tmux;

use std::fs;
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

/// Setup test environment with initialized git repo and .build/operations directory.
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

    temp
}

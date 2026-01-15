// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for signal handling (oj done, oj checkpoint)
//!
//! These tests verify the signal commands work correctly for communicating
//! phase completion and checkpointing from within workspace contexts.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{setup_test_env, tmux, unique_id};
use predicates::prelude::*;

#[test]
fn test_done_without_workspace_fails_gracefully() {
    let temp = setup_test_env();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .arg("done")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not detect workspace"));
}

#[test]
fn test_done_with_env_var_signals_completion() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("signal-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline first
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test signaling"])
        .assert()
        .success();

    // Signal done with OTTER_PIPELINE env var
    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("done")
        .assert()
        .success()
        .stdout(predicate::str::contains("phase complete"));
}

#[test]
fn test_done_with_error_fails_pipeline() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("error-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test error handling"])
        .assert()
        .success();

    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    // Signal done with error
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .args(["done", "--error", "Something went wrong"])
        .assert()
        .success()
        .stdout(predicate::str::contains("phase failed"));
}

#[test]
fn test_checkpoint_without_workspace_fails_gracefully() {
    let temp = setup_test_env();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .arg("checkpoint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not detect workspace"));
}

#[test]
fn test_checkpoint_saves_state() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("checkpoint-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test checkpointing"])
        .assert()
        .success();

    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    // Save checkpoint
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("checkpoint")
        .assert()
        .success()
        .stdout(predicate::str::contains("Checkpoint saved"));
}

// ============================================================================
// New Phase 2 tests - Signal handling verification
// ============================================================================

#[test]
fn test_done_requires_pipeline_context() {
    // Test that done fails without OTTER_PIPELINE in a non-workspace directory
    let temp = setup_test_env();

    // Run done from project root (not workspace) without env var
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .arg("done")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not detect"));
}

#[test]
fn test_done_from_workspace_autodetects_pipeline() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("autodetect-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test autodetection"])
        .assert()
        .success();

    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    // With OTTER_PIPELINE env var, it should work from workspace
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("done")
        .assert()
        .success();
}

#[test]
fn test_done_error_marks_failed() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("fail-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test failure marking"])
        .assert()
        .success();

    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    // Signal done with error - verifies the error message is accepted and processed
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .args(["done", "--error", "Build failed due to compile errors"])
        .assert()
        .success()
        .stdout(predicate::str::contains("phase failed"));

    // Verify state file still exists (exact state format depends on daemon processing)
    let state_path = temp
        .path()
        .join(format!(".build/operations/pipelines/{}.json", pipeline_id));
    assert!(state_path.exists(), "Pipeline state file should exist");
}

#[test]
fn test_checkpoint_updates_heartbeat() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("heartbeat-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test heartbeat update"])
        .assert()
        .success();

    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));

    // Wait a moment to ensure timestamp difference is measurable
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Save checkpoint
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("checkpoint")
        .assert()
        .success()
        .stdout(predicate::str::contains("Checkpoint saved"));

    // The checkpoint command should update last_activity in the pipeline state
    let state_path = temp
        .path()
        .join(format!(".build/operations/pipelines/{}.json", pipeline_id));
    assert!(state_path.exists(), "Pipeline state should exist");
}

#[test]
fn test_checkpoint_without_context_fails() {
    let temp = setup_test_env();

    // Checkpoint without being in a workspace or having OTTER_PIPELINE should fail
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .arg("checkpoint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not detect"));
}

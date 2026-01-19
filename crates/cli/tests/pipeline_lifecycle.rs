// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for pipeline lifecycle
//!
//! These tests verify end-to-end pipeline operations through the CLI binary,
//! including workspace creation, tmux session spawning, and state management.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{claudeless, setup_test_env, tmux, unique_id};
use predicates::prelude::*;
use std::fs;

#[test]
fn test_oj_help() {
    let mut cmd = Command::cargo_bin("oj").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("oj orchestrates"));
}

#[test]
fn test_oj_version() {
    let mut cmd = Command::cargo_bin("oj").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("oj"));
}

#[test]
fn test_run_build_creates_pipeline_state() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("feature-{}", id);

    let mut cmd = Command::cargo_bin("oj").unwrap();
    cmd.current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Build a test feature",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Started build pipeline"));

    // Verify pipeline was created by listing it
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("build-{}", name)));
}

#[test]
fn test_run_build_creates_workspace() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("ws-{}", id);

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test workspace creation",
        ])
        .assert()
        .success();

    // Verify workspace directory exists
    assert!(temp
        .path()
        .join(format!(".worktrees/build-{}", name))
        .exists());
}

#[test]
fn test_run_build_generates_claude_md() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("claude-{}", id);

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test CLAUDE.md generation",
        ])
        .assert()
        .success();

    // Verify CLAUDE.md exists
    let claude_md_path = temp
        .path()
        .join(format!(".worktrees/build-{}/CLAUDE.md", name));
    assert!(claude_md_path.exists());

    // Verify CLAUDE.md contains the prompt
    let content = fs::read_to_string(&claude_md_path).unwrap();
    assert!(content.contains("Test CLAUDE.md generation"));
    assert!(content.contains("oj done"));
}

#[test]
fn test_run_bugfix_creates_pipeline() {
    let temp = setup_test_env();
    let id = unique_id();
    let issue_id = format!("bug-{}", id);

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "bugfix", "--input", &format!("bug={}", issue_id)])
        .assert()
        .success()
        .stdout(predicate::str::contains("Started bugfix pipeline"));

    // Verify pipeline was created by listing it
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("bugfix-{}", issue_id)));

    // Verify workspace
    assert!(temp
        .path()
        .join(format!(".worktrees/bugfix-{}", issue_id))
        .exists());
}

#[test]
fn test_pipeline_list_shows_created_pipelines() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("list-{}", id);

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test listing",
        ])
        .assert()
        .success();

    // List pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&name));
}

#[test]
fn test_daemon_once_completes() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("daemon-{}", id);

    // Create a pipeline first so daemon has something to process
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test daemon",
        ])
        .assert()
        .success();

    // Run daemon once
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Running single daemon iteration"));
}

// ============================================================================
// New Phase 2 tests - Tmux session verification
// ============================================================================

#[test]
fn test_run_build_spawns_tmux_session() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("tmux-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Setup guard to cleanup session on test exit (pass or fail)
    // Session name format is: oj-{pipeline_id}-{phase}
    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test tmux session spawning",
        ])
        .assert()
        .success();

    // Wait for tmux session to be created (up to 2 seconds)
    // Session name will be oj-build-{name}-init
    let session_found = tmux::wait_for_session_matching(&format!("oj-{}", pipeline_id), 2000);
    assert!(
        session_found,
        "Expected tmux session containing 'oj-{}' to exist",
        pipeline_id
    );
}

#[test]
fn test_run_build_claudeless_receives_prompt() {
    // Skip if claudeless not available
    if !claudeless::is_claudeless_available() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("capture-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Setup guard (session name format: oj-{pipeline_id}-{phase})
    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create scenario and capture path
    let scenario = claudeless::simple_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();
    let capture_path = temp.path().join("capture.jsonl");

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .env("CLAUDELESS_CAPTURE", capture_path.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test prompt capture",
        ])
        .assert()
        .success();

    // Verify pipeline was created by listing it
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_full_pipeline_lifecycle() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("lifecycle-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Setup guard (session name format: oj-{pipeline_id}-{phase})
    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // 1. Create pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Full lifecycle test",
        ])
        .assert()
        .success();

    // 2. Verify initial state via pipeline show
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "show", &pipeline_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Phase:"));

    // 3. Signal done from workspace
    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("done")
        .assert()
        .success();

    // 4. Run daemon once to process state transition
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();
}

// ============================================================================
// Spot checks for hardcoded pipelines (minimal - will be replaced by Epic 6)
// ============================================================================

#[test]
fn test_build_pipeline_exists() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("build-check-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Verify `oj run build` works end-to-end
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Build pipeline existence check",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Started build pipeline"));
}

#[test]
fn test_bugfix_pipeline_exists() {
    let temp = setup_test_env();
    let id = unique_id();
    let issue = format!("issue-{}", id);
    let pipeline_id = format!("bugfix-{}", issue);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Verify `oj run bugfix` works end-to-end
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "bugfix", "--input", &format!("bug={}", issue)])
        .assert()
        .success()
        .stdout(predicate::str::contains("Started bugfix pipeline"));
}

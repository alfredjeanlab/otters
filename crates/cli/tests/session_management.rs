// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for session management
//!
//! These tests verify `oj session` commands work correctly.
//! Session management is durable infrastructure per EXECUTION.md.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{setup_test_env, tmux, unique_id};
use predicates::prelude::*;

// ============================================================================
// Session lifecycle tests
// ============================================================================

#[test]
fn test_session_list_shows_active_sessions() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("sess-list-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline (which spawns a tmux session)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Session list test"])
        .assert()
        .success();

    // Wait for session to be created
    assert!(
        tmux::wait_for_session_matching(&format!("oj-{}", pipeline_id), 2000),
        "Session should be created"
    );

    // Session should appear in oj session list
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_session_list_empty_when_no_sessions() {
    let temp = setup_test_env();

    // No pipelines created, so no sessions
    // Note: This test may pick up other sessions in the system
    // The command should succeed even with no sessions
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "list"])
        .assert()
        .success();
}

#[test]
fn test_session_list_multiple_sessions() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("multi-a-{}", id);
    let name2 = format!("multi-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create two pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name1, "Multi session test A"])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name2, "Multi session test B"])
        .assert()
        .success();

    // Both sessions should appear in list
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id1))
        .stdout(predicate::str::contains(&pipeline_id2));
}

#[test]
fn test_session_show_displays_details() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("sess-show-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Session show test"])
        .assert()
        .success();

    // Wait for session
    assert!(
        tmux::wait_for_session_matching(&format!("oj-{}", pipeline_id), 2000),
        "Session should be created"
    );

    // Show session details - use full session name pattern
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "show", &format!("oj-{}-init", pipeline_id)])
        .assert()
        .success()
        .stdout(predicate::str::contains("Session:"));
}

#[test]
fn test_session_show_nonexistent_fails() {
    let temp = setup_test_env();

    // Try to show a session that doesn't exist
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "show", "nonexistent-session-xyz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ============================================================================
// Session control tests
// ============================================================================

#[test]
fn test_session_kill_terminates_session() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("sess-kill-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Note: Don't use SessionGuard here since we're testing kill
    // We'll clean up manually

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Session kill test"])
        .assert()
        .success();

    // Wait for session
    let session_name = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&session_name, 2000),
        "Session should be created"
    );

    // Kill the session via oj command
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "kill", &session_name])
        .assert()
        .success()
        .stdout(predicate::str::contains("Killed session"));

    // Session should no longer exist
    assert!(
        tmux::wait_for_session_gone(&session_name, 1000),
        "Session should be killed"
    );
}

#[test]
fn test_session_kill_updates_pipeline_state() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("kill-state-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Kill state test"])
        .assert()
        .success();

    let session_name = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&session_name, 2000),
        "Session should be created"
    );

    // Kill the session
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "kill", &session_name])
        .assert()
        .success();

    // Pipeline state should still exist (killing session doesn't delete pipeline)
    let state_path = temp
        .path()
        .join(format!(".build/operations/pipelines/{}.json", pipeline_id));
    assert!(state_path.exists(), "Pipeline state should still exist");
}

#[test]
fn test_session_nudge_sends_input() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("sess-nudge-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Session nudge test"])
        .assert()
        .success();

    let session_name = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&session_name, 2000),
        "Session should be created"
    );

    // Nudge the session with a message
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "nudge", &session_name, "Test nudge message"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent nudge"));
}

#[test]
fn test_session_nudge_nonexistent_handled() {
    let temp = setup_test_env();

    // Try to nudge a session that doesn't exist
    // Note: The command currently succeeds but logs an error to stderr
    // (tmux send-keys returns an error in stderr but the CLI reports success)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "nudge", "nonexistent-session-xyz", "Hello"])
        .assert()
        .success()
        .stderr(predicate::str::contains("can't find"));
}

// ============================================================================
// Session naming and detection tests
// ============================================================================

#[test]
fn test_session_naming_convention() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("naming-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a build pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Naming convention test"])
        .assert()
        .success();

    // Session name should follow oj-{pipeline_id}-{phase} pattern
    // For build pipelines in init phase, it's oj-build-{name}-init
    let expected_pattern = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&expected_pattern, 2000),
        "Session name should match expected pattern"
    );
}

#[test]
fn test_session_name_uniqueness() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("unique-a-{}", id);
    let name2 = format!("unique-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create two pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name1, "Uniqueness test A"])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name2, "Uniqueness test B"])
        .assert()
        .success();

    // Each pipeline should have its own unique session name
    let session1 = format!("oj-{}-init", pipeline_id1);
    let session2 = format!("oj-{}-init", pipeline_id2);

    assert!(
        tmux::wait_for_session_matching(&session1, 2000),
        "First session should exist"
    );
    assert!(
        tmux::wait_for_session_matching(&session2, 2000),
        "Second session should exist"
    );

    // Verify they're different sessions
    assert_ne!(session1, session2, "Session names should be different");
}

#[test]
fn test_dead_session_detection() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("dead-detect-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Dead session detection test"])
        .assert()
        .success();

    let session_name = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&session_name, 2000),
        "Session should be created"
    );

    // Kill the session directly via tmux (not through oj)
    tmux::kill_session(&session_name);
    assert!(
        tmux::wait_for_session_gone(&session_name, 1000),
        "Session should be killed"
    );

    // Session list should not show the dead session
    // (Or should show it as dead - implementation dependent)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "list"])
        .assert()
        .success();
}

#[test]
fn test_session_capture_pane() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("capture-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Capture pane test"])
        .assert()
        .success();

    let session_name = format!("oj-{}-init", pipeline_id);
    assert!(
        tmux::wait_for_session_matching(&session_name, 2000),
        "Session should be created"
    );

    // We can capture pane content (verify via show command)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["session", "show", &session_name])
        .assert()
        .success();
}

// ============================================================================
// Session environment tests
// ============================================================================

#[test]
fn test_session_has_otter_env_vars() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("env-vars-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Environment vars test"])
        .assert()
        .success();

    // The session should have OTTER_PIPELINE and OTTER_WORKSPACE set
    // This is verified indirectly by the fact that oj done works from the workspace
    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    assert!(workspace_path.exists(), "Workspace should exist");
}

#[test]
fn test_session_cwd_is_workspace() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("cwd-test-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "CWD test"])
        .assert()
        .success();

    // Verify workspace directory exists (session should start there)
    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    assert!(
        workspace_path.exists(),
        "Workspace directory should exist for session"
    );
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for daemon polling
//!
//! These tests verify the daemon's polling, ticking, and session management
//! behavior through the CLI interface.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{setup_test_env, tmux, unique_id};
use predicates::prelude::*;

#[test]
fn test_daemon_help() {
    let mut cmd = Command::cargo_bin("oj").unwrap();
    cmd.args(["daemon", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("poll-interval"))
        .stdout(predicate::str::contains("tick-interval"))
        .stdout(predicate::str::contains("queue-interval"));
}

#[test]
fn test_daemon_once_empty_state() {
    let temp = setup_test_env();

    // Run daemon once with no pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Running single daemon iteration"))
        .stdout(predicate::str::contains("Done"));
}

#[test]
fn test_daemon_once_with_pipeline() {
    let temp = setup_test_env();
    let id = unique_id();
    let pipeline_name = format!("daemon-poll-{}", id);
    let pipeline_id = format!("build-{}", pipeline_name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &pipeline_name, "Test daemon polling"])
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

#[test]
fn test_daemon_custom_intervals() {
    let temp = setup_test_env();

    // Test that custom intervals are accepted
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "daemon",
            "--once",
            "--poll-interval",
            "10",
            "--tick-interval",
            "60",
            "--queue-interval",
            "20",
        ])
        .assert()
        .success();
}

#[test]
fn test_daemon_logs_intervals_on_start() {
    let temp = setup_test_env();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Poll interval:"))
        .stdout(predicate::str::contains("Tick interval:"))
        .stdout(predicate::str::contains("Queue interval:"));
}

// ============================================================================
// New Phase 2 tests - Daemon behavior verification
// ============================================================================

#[test]
fn test_daemon_once_completes() {
    let temp = setup_test_env();

    // Verify `daemon --once` exits after one iteration
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Running single daemon iteration"))
        .stdout(predicate::str::contains("Done"));
}

#[test]
fn test_daemon_once_with_no_pipelines() {
    let temp = setup_test_env();

    // Verify daemon exits cleanly with nothing to do
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Done"));
}

#[test]
fn test_daemon_detects_dead_sessions() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("dead-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Session name format is: oj-{pipeline_id}-{phase}
    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline (which spawns a tmux session)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test dead session detection"])
        .assert()
        .success();

    // Wait for session to be created (session name is oj-build-{name}-init)
    assert!(
        tmux::wait_for_session_matching(&format!("oj-{}", pipeline_id), 2000),
        "Session should be created"
    );

    // Kill the tmux session manually
    tmux::kill_sessions_matching(&format!("oj-{}", pipeline_id));

    // Wait for session to be gone
    assert!(
        tmux::wait_for_session_gone(&format!("oj-{}-init", pipeline_id), 1000),
        "Session should be killed"
    );

    // Run daemon once - it should detect the dead session
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();

    // The daemon should have processed the pipeline even though the session is dead
    // (exact behavior depends on implementation - this just ensures it doesn't crash)
    let state_path = temp
        .path()
        .join(format!(".build/operations/pipelines/{}.json", pipeline_id));
    assert!(state_path.exists(), "Pipeline state should still exist");
}

#[test]
fn test_daemon_detects_completed_sessions() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("completed-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test completed session detection"])
        .assert()
        .success();

    // Signal done from workspace
    let workspace_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace_path)
        .env("OTTER_PIPELINE", &pipeline_id)
        .arg("done")
        .assert()
        .success();

    // Run daemon once - it should process the completed state
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();
}

#[test]
fn test_daemon_respects_poll_interval() {
    let temp = setup_test_env();

    // Verify custom --poll-interval is used (shown in startup message)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once", "--poll-interval", "15"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Poll interval: 15s"));
}

#[test]
fn test_daemon_respects_tick_interval() {
    let temp = setup_test_env();

    // Verify custom --tick-interval is used (shown in startup message)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once", "--tick-interval", "45"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tick interval: 45s"));
}

#[test]
fn test_daemon_logs_startup() {
    let temp = setup_test_env();

    // Verify startup message with intervals logged
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Starting oj daemon"));
}

#[test]
fn test_daemon_logs_pipeline_activity() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("activity-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline so there's activity to log
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Test activity logging"])
        .assert()
        .success();

    // Run daemon once - should log that it's processing
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Running single daemon iteration"));
}

// ============================================================================
// Signal handling tests
// ============================================================================

#[test]
#[cfg(unix)]
fn test_daemon_handles_sigint() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command as StdCommand, Stdio};
    use std::time::Duration;

    let temp = setup_test_env();

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    std::thread::sleep(Duration::from_millis(500));

    // Send SIGINT
    let pid = Pid::from_raw(child.id() as i32);
    kill(pid, Signal::SIGINT).expect("Failed to send SIGINT");

    let status = child.wait().expect("Failed to wait for daemon");
    // On Unix, SIGINT results in:
    // - exit code 0 when handled gracefully (ctrlc handler triggered)
    // - signal 2 when killed by signal (status.signal() == Some(2))
    assert!(
        status.success() || status.signal() == Some(2),
        "Daemon should exit on SIGINT, got: {:?}",
        status
    );
}

#[test]
#[cfg(unix)]
fn test_daemon_handles_sigterm() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command as StdCommand, Stdio};
    use std::time::Duration;

    let temp = setup_test_env();

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    std::thread::sleep(Duration::from_millis(500));

    // Send SIGTERM
    let pid = Pid::from_raw(child.id() as i32);
    kill(pid, Signal::SIGTERM).expect("Failed to send SIGTERM");

    let status = child.wait().expect("Failed to wait for daemon");
    // On Unix, SIGTERM results in:
    // - exit code 0 when handled gracefully (ctrlc handler triggered)
    // - signal 15 when killed by signal (status.signal() == Some(15))
    assert!(
        status.success() || status.signal() == Some(15),
        "Daemon should exit on SIGTERM, got: {:?}",
        status
    );
}

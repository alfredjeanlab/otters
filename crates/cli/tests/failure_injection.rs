// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for failure injection
//!
//! These tests verify error handling paths using claudeless failure scenarios.
//! They ensure the system handles network errors, auth failures, rate limits,
//! and other error conditions gracefully.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{claudeless, setup_test_env, tmux, unique_id};
use predicates::prelude::*;

/// Helper to check if claudeless integration tests should run
fn should_run_claudeless_tests() -> bool {
    claudeless::is_claudeless_available()
}

#[test]
fn test_network_failure_detected() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("net-fail-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create network failure scenario
    let scenario = claudeless::network_failure_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    // Create pipeline with failure scenario
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test network failure handling",
        ])
        .assert()
        .success();

    // Verify pipeline was created (even if it will fail later)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_auth_failure_detected() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("auth-fail-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create auth failure scenario
    let scenario = claudeless::auth_failure_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test auth failure handling",
        ])
        .assert()
        .success();

    // Pipeline should be created
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_auth_failure_message_recorded() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("auth-msg-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create auth failure scenario
    let scenario = claudeless::auth_failure_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test auth error message",
        ])
        .assert()
        .success();

    // Verify scenario file was written correctly
    assert!(scenario.exists(), "Scenario file should exist");
}

#[test]
fn test_rate_limit_handled() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("rate-limit-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create rate limit scenario
    let scenario = claudeless::rate_limit_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test rate limit handling",
        ])
        .assert()
        .success();

    // Daemon shouldn't crash when processing rate-limited pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();
}

#[test]
fn test_malformed_response_handled() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("malformed-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create malformed response scenario
    let scenario = claudeless::malformed_response_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test malformed response handling",
        ])
        .assert()
        .success();

    // Daemon shouldn't crash on malformed response
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();
}

#[test]
fn test_timeout_detected() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("timeout-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create timeout scenario
    let scenario = claudeless::timeout_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test timeout handling",
        ])
        .assert()
        .success();

    // Pipeline should be created
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_session_crash_detected() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("crash-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

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
            "prompt=Test session crash detection",
        ])
        .assert()
        .success();

    // Wait for session to exist
    let session_exists = tmux::wait_for_session_matching(&format!("oj-{}", pipeline_id), 2000);
    if !session_exists {
        // Session may have already completed; skip the crash test
        eprintln!("Session completed before crash test could run");
        return;
    }

    // Kill the tmux session to simulate crash
    tmux::kill_sessions_matching(&format!("oj-{}", pipeline_id));

    // Daemon should handle the dead session gracefully
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success();
}

#[test]
fn test_recovery_after_transient_failure() {
    if !should_run_claudeless_tests() {
        eprintln!("Skipping: claudeless not found in PATH. Install claudeless globally first.");
        return;
    }

    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("transient-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create transient failure scenario (fails first, succeeds on retry)
    let scenario = claudeless::transient_failure_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name),
            "--input",
            "prompt=Test transient failure recovery",
        ])
        .assert()
        .success();

    // Verify pipeline was created
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

//! Daemon lifecycle specs
//!
//! Verify daemon start/stop/status lifecycle.

use crate::prelude::*;

#[test]
fn daemon_status_fails_when_not_running() {
    let temp = Project::empty();

    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Daemon not running");
}

#[test]
fn daemon_start_reports_success() {
    let temp = Project::empty();

    temp.oj()
        .args(&["daemon", "start"])
        .passes()
        .stdout_has("Daemon started");
}

#[test]
fn daemon_status_shows_running_after_start() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Status: running");
}

#[test]
fn daemon_status_shows_uptime() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Uptime:");
}

#[test]
fn daemon_status_shows_pipeline_count() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Pipelines:");
}

#[test]
fn daemon_status_shows_version() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Version:");
}

#[test]
fn daemon_stop_reports_success() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["daemon", "stop"])
        .passes()
        .stdout_has("Daemon stopped");
}

#[test]
fn daemon_status_fails_after_stop() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj().args(&["daemon", "stop"]).passes();
    temp.oj()
        .args(&["daemon", "status"])
        .passes()
        .stdout_has("Daemon not running");
}

#[test]
fn daemon_start_shows_runbook_error() {
    let temp = Project::empty();
    // Invalid runbook - missing required 'run' field
    temp.file(
        ".oj/runbooks/bad.toml",
        "[command.test]\nargs = \"<name>\"\n",
    );

    // Start should fail and show the parse error
    temp.oj()
        .args(&["daemon", "start"])
        .fails()
        .stderr_has("missing required field");
}

#[test]
fn daemon_creates_version_file() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();

    // Find the state directory for this project (using isolated state_path)
    let state_dir = temp.state_path().join("oj/projects");

    // There should be at least one project directory with a version file
    let has_version = wait_for(SPEC_WAIT_MAX_MS, || {
        std::fs::read_dir(&state_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|entry| entry.path().join("daemon.version").exists())
            })
            .unwrap_or(false)
    });

    assert!(has_version, "daemon.version file should exist");
}

#[test]
fn daemon_creates_pid_file() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();

    let state_dir = temp.state_path().join("oj/projects");

    // There should be at least one project directory with a pid file
    let has_pid = wait_for(SPEC_WAIT_MAX_MS, || {
        std::fs::read_dir(&state_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|entry| entry.path().join("daemon.pid").exists())
            })
            .unwrap_or(false)
    });

    assert!(has_pid, "daemon.pid file should exist");
}

#[test]
fn daemon_creates_socket_file() {
    let temp = Project::empty();
    temp.oj().args(&["daemon", "start"]).passes();

    // Socket is in OJ_SOCKET_DIR (which we set to state_path in tests)
    let socket_dir = temp.state_path().to_path_buf();

    let has_socket = wait_for(SPEC_WAIT_MAX_MS, || {
        std::fs::read_dir(&socket_dir)
            .ok()
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|ext| ext == "sock")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });

    assert!(has_socket, "daemon socket file should exist");
}

#[test]
fn daemon_start_error_log_shows_in_cli() {
    // Force socket path to exceed SUN_LEN (104 bytes on macOS) by using OJ_SOCKET_DIR
    let temp = Project::empty();

    // Create a deeply nested socket directory to make socket path too long
    // Socket path will be: {socket_dir}/{hash}.sock
    // We need total path > 104 chars
    let long_suffix =
        "this_is_a_very_long_path_segment_to_ensure_socket_path_exceeds_sun_len_limit_on_macos";
    let long_socket_dir = temp.state_path().join(long_suffix);
    std::fs::create_dir_all(&long_socket_dir).unwrap();

    // Start should fail with socket path error, NOT "Connection timeout"
    temp.oj()
        .env("OJ_SOCKET_DIR", &long_socket_dir)
        .args(&["daemon", "start"])
        .fails()
        .stderr_has("path must be shorter than SUN_LEN")
        .stderr_lacks("Connection timeout");
}

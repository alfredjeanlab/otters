// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Tests for daemon client behavior.

use super::{get_daemon_dir, ClientError, DaemonClient};
use std::fs;
use tempfile::tempdir;

/// Verify that connect() does not delete state files when daemon is not running.
///
/// This is a regression test for a race condition where connect() would call
/// cleanup_stale_files() during startup polling, deleting the pid file before
/// the daemon finished initializing.
#[test]
fn connect_does_not_delete_pid_file() {
    let temp = tempdir().unwrap();
    let project_root = temp.path().to_path_buf();

    // Set up isolated state directory
    let state_dir = tempdir().unwrap();
    std::env::set_var("XDG_STATE_HOME", state_dir.path());
    std::env::set_var("OJ_SOCKET_DIR", state_dir.path());

    // Create a pid file (simulating daemon mid-startup)
    let daemon_dir = get_daemon_dir(&project_root).unwrap();
    fs::create_dir_all(&daemon_dir).unwrap();
    let pid_path = daemon_dir.join("daemon.pid");
    fs::write(&pid_path, "12345\n").unwrap();

    // connect() should fail (no socket) but NOT delete the pid file
    let result = DaemonClient::connect(project_root);
    assert!(matches!(result, Err(ClientError::DaemonNotRunning)));

    // Pid file should still exist
    assert!(pid_path.exists(), "connect() must not delete pid file");
}

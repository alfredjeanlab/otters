// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Claudeless simulator integration helpers for tests.
//!
//! These helpers configure the test environment to use the globally installed
//! claudeless binary as a mock Claude CLI, allowing tests to verify actual
//! behavior through the Engine/adapter layer.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static CLAUDE_SYMLINK_DIR: OnceLock<Option<tempfile::TempDir>> = OnceLock::new();

/// Get path to the globally installed claudeless binary.
/// Uses `which claudeless` to find it in PATH.
pub fn claudeless_bin() -> PathBuf {
    claudeless_bin_opt().expect("claudeless not found in PATH")
}

/// Get path to the globally installed claudeless binary, if available.
fn claudeless_bin_opt() -> Option<PathBuf> {
    Command::new("which")
        .arg("claudeless")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
            None
        })
}

/// Get the directory containing the `claude` symlink for tests.
/// Creates a temp directory with a `claude` -> `claudeless` symlink.
/// This directory is cached and reused across tests.
pub fn claudeless_bin_dir() -> PathBuf {
    let temp_dir = CLAUDE_SYMLINK_DIR.get_or_init(|| {
        let claudeless_path = claudeless_bin_opt()?;
        let temp = tempfile::TempDir::new().ok()?;
        let claude_symlink = temp.path().join("claude");
        std::os::unix::fs::symlink(&claudeless_path, &claude_symlink).ok()?;
        Some(temp)
    });

    temp_dir
        .as_ref()
        .expect("Failed to create claude symlink directory")
        .path()
        .to_path_buf()
}

/// Setup environment PATH with claude symlink directory prepended.
/// Returns the modified PATH string.
pub fn setup_claudeless_path() -> String {
    let bin_dir = claudeless_bin_dir();
    let current_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bin_dir.display(), current_path)
}

/// Get the directory containing scenario TOML files.
fn scenarios_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("tests/scenarios")
}

/// Copy a scenario file to the test directory.
/// Returns path to the copied scenario file.
fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let src = scenarios_dir().join(format!("{}.toml", name));
    let dst = dir.join(format!("{}.toml", name));
    fs::copy(&src, &dst).expect("Failed to copy scenario file");
    dst
}

/// Write a scenario file for claudeless.
/// Returns path to the created scenario file.
pub fn write_scenario(dir: &Path, name: &str, content: &str) -> PathBuf {
    let scenario_path = dir.join(format!("{}.toml", name));
    fs::write(&scenario_path, content).expect("Failed to write scenario file");
    scenario_path
}

/// Create scenario that provides a simple response.
/// This is the basic scenario for tests that just need claudeless to respond.
pub fn simple_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "simple")
}

/// Create scenario that signals done after receiving prompt.
/// Uses tool_execution for simulated tool calls.
pub fn auto_done_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "auto-done")
}

/// Create scenario that fails with network error.
pub fn network_failure_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "network-failure")
}

/// Create scenario that fails with auth error.
pub fn auth_failure_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "auth-failure")
}

/// Create scenario that fails with rate limit.
pub fn rate_limit_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "rate-limit")
}

/// Create scenario that times out (connection timeout).
pub fn timeout_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "timeout")
}

/// Create scenario with malformed response.
pub fn malformed_response_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "malformed")
}

/// Create scenario that fails on first request but succeeds on retry.
/// Uses pattern matching to differentiate first vs subsequent requests.
pub fn transient_failure_scenario(dir: &Path) -> PathBuf {
    copy_scenario(dir, "transient-failure")
}

/// Create scenario that runs for a specified duration (for heartbeat tests).
pub fn long_running_scenario(dir: &Path, delay_ms: u64) -> PathBuf {
    let content = format!(
        r#"name = "long-running"

[[responses]]
pattern = {{ type = "any" }}

[responses.response]
text = "Working on a long task..."
delay_ms = {}
"#,
        delay_ms
    );
    write_scenario(dir, "long-running", &content)
}

/// Read claudeless capture log from .claudeless/capture.jsonl (if it exists).
pub fn read_capture_log(dir: &Path) -> Option<String> {
    let capture_path = dir.join(".claudeless/capture.jsonl");
    fs::read_to_string(capture_path).ok()
}

/// State directory for test isolation (.claudeless-state).
pub fn state_dir(temp_dir: &Path) -> PathBuf {
    let state = temp_dir.join(".claudeless-state");
    fs::create_dir_all(&state).expect("Failed to create state directory");
    state
}

/// Check if claudeless binary is available in PATH.
pub fn is_claudeless_available() -> bool {
    claudeless_bin_opt().is_some()
}

/// Check if claudeless is available; if not, print skip message and return false.
/// Use this in tests to skip when claudeless isn't installed:
/// ```
/// if !claudeless::require_claudeless() { return; }
/// ```
pub fn require_claudeless() -> bool {
    if !is_claudeless_available() {
        eprintln!(
            "Skipping: claudeless binary not found in PATH. Install claudeless globally first."
        );
        return false;
    }
    true
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Tmux session assertion helpers for integration tests.
//!
//! These utilities help verify tmux session state and provide cleanup
//! mechanisms to ensure tests don't leave orphan sessions.

use std::process::Command;
use std::thread;
use std::time::Duration;

/// List all tmux sessions (returns empty vec if no server).
pub fn list_sessions() -> Vec<String> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect(),
        _ => vec![],
    }
}

/// Check if a session exists by exact name.
pub fn session_exists(name: &str) -> bool {
    let output = Command::new("tmux")
        .args(["has-session", "-t", name])
        .output();

    matches!(output, Ok(out) if out.status.success())
}

/// Check if any session name contains pattern.
pub fn session_matches(pattern: &str) -> bool {
    list_sessions().iter().any(|s| s.contains(pattern))
}

/// Find all sessions matching a pattern.
pub fn find_sessions_matching(pattern: &str) -> Vec<String> {
    list_sessions()
        .into_iter()
        .filter(|s| s.contains(pattern))
        .collect()
}

/// Kill a session by name.
pub fn kill_session(name: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .output();
}

/// Kill all sessions matching a pattern.
pub fn kill_sessions_matching(pattern: &str) {
    for session in find_sessions_matching(pattern) {
        kill_session(&session);
    }
}

/// Capture pane content from a session.
pub fn capture_pane(session: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", session, "-p"])
        .output();

    match output {
        Ok(out) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).to_string()),
        _ => None,
    }
}

/// Send keys to a session.
pub fn send_keys(session: &str, keys: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", session, keys, "Enter"])
        .output();
}

/// Send raw keys to a session (without Enter).
pub fn send_keys_raw(session: &str, keys: &str) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", session, keys])
        .output();
}

/// Wait for session to exist (with timeout).
/// Polls every 100ms until timeout_ms.
pub fn wait_for_session(name: &str, timeout_ms: u64) -> bool {
    let poll_interval = Duration::from_millis(100);
    let max_iterations = timeout_ms / 100;

    for _ in 0..max_iterations {
        if session_exists(name) {
            return true;
        }
        thread::sleep(poll_interval);
    }
    false
}

/// Wait for session matching pattern to exist (with timeout).
pub fn wait_for_session_matching(pattern: &str, timeout_ms: u64) -> bool {
    let poll_interval = Duration::from_millis(100);
    let max_iterations = timeout_ms / 100;

    for _ in 0..max_iterations {
        if session_matches(pattern) {
            return true;
        }
        thread::sleep(poll_interval);
    }
    false
}

/// Wait for session pane to contain text (with timeout).
/// Polls every 100ms until timeout_ms.
pub fn wait_for_content(session: &str, content: &str, timeout_ms: u64) -> bool {
    let poll_interval = Duration::from_millis(100);
    let max_iterations = timeout_ms / 100;

    for _ in 0..max_iterations {
        if let Some(pane_content) = capture_pane(session) {
            if pane_content.contains(content) {
                return true;
            }
        }
        thread::sleep(poll_interval);
    }
    false
}

/// Wait for session to NOT exist (with timeout).
/// Useful for verifying cleanup.
pub fn wait_for_session_gone(name: &str, timeout_ms: u64) -> bool {
    let poll_interval = Duration::from_millis(100);
    let max_iterations = timeout_ms / 100;

    for _ in 0..max_iterations {
        if !session_exists(name) {
            return true;
        }
        thread::sleep(poll_interval);
    }
    false
}

/// Guard that kills sessions on drop.
/// Usage: let _guard = SessionGuard::new("oj-build-mytest");
pub struct SessionGuard {
    pattern: String,
}

impl SessionGuard {
    /// Create a new session guard that will kill all sessions matching
    /// the pattern when dropped.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
        }
    }

    /// Create a session guard for a specific session name.
    pub fn for_session(name: &str) -> Self {
        Self::new(name)
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        kill_sessions_matching(&self.pattern);
    }
}

/// Get the number of active tmux sessions.
pub fn session_count() -> usize {
    list_sessions().len()
}

/// Check if tmux server is running.
pub fn is_tmux_running() -> bool {
    let output = Command::new("tmux").args(["list-sessions"]).output();

    // tmux returns success if server is running (even if no sessions)
    // or returns error with specific message if not running
    match output {
        Ok(out) => out.status.success() || !out.stderr.is_empty(),
        Err(_) => false,
    }
}

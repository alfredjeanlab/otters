// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Session log parsing for detecting Claude agent state.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// State detected from Claude's session log
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// Claude is working (processing or running tools)
    Working,
    /// Claude finished and is waiting for input
    WaitingForInput,
    /// Session encountered a failure
    Failed(FailureReason),
    /// Log file not found or unreadable
    Unknown,
}

/// Reason for session failure
#[derive(Debug, Clone, PartialEq)]
pub enum FailureReason {
    /// Invalid API key
    Unauthorized,
    /// Exceeded quota or billing issue
    OutOfCredits,
    /// Network connectivity issue
    NoInternet,
    /// Rate limited by API
    RateLimited,
    /// Other error
    Other(String),
}

/// Watch a Claude session log file
pub struct SessionLogWatcher {
    path: PathBuf,
}

impl SessionLogWatcher {
    /// Create a new watcher for the given session log path
    pub fn new(session_log_path: PathBuf) -> Self {
        Self {
            path: session_log_path,
        }
    }

    /// Check current session state by reading the log file
    pub fn check_state(&self) -> SessionState {
        let Ok(file) = File::open(&self.path) else {
            return SessionState::Unknown;
        };

        let reader = BufReader::new(file);
        let mut last_line = String::new();

        // Read all lines to find the last one
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            last_line = line;
        }

        if last_line.is_empty() {
            return SessionState::Unknown;
        }

        // Parse last line
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&last_line) else {
            return SessionState::Unknown;
        };

        // Check for error indicators first
        if let Some(error) = Self::detect_error(&json) {
            return SessionState::Failed(error);
        }

        let last_type = json.get("type").and_then(|v| v.as_str());

        // Check stop_reason for assistant messages
        if last_type == Some("assistant") {
            let stop_reason = json
                .get("message")
                .and_then(|m| m.get("stop_reason"))
                .and_then(|v| v.as_str());

            return match stop_reason {
                Some("end_turn") => SessionState::WaitingForInput,
                Some("tool_use") => SessionState::Working,
                _ => SessionState::Unknown,
            };
        }

        // User messages mean Claude is working on them
        if last_type == Some("user") {
            return SessionState::Working;
        }

        SessionState::Unknown
    }

    fn detect_error(json: &serde_json::Value) -> Option<FailureReason> {
        let error_msg = json.get("error").and_then(|v| v.as_str()).or_else(|| {
            json.get("message")
                .and_then(|m| m.get("error"))
                .and_then(|v| v.as_str())
        });

        if let Some(err) = error_msg {
            let err_lower = err.to_lowercase();
            if err_lower.contains("unauthorized") || err_lower.contains("invalid api key") {
                return Some(FailureReason::Unauthorized);
            }
            if err_lower.contains("credit")
                || err_lower.contains("quota")
                || err_lower.contains("billing")
            {
                return Some(FailureReason::OutOfCredits);
            }
            if err_lower.contains("network")
                || err_lower.contains("connection")
                || err_lower.contains("offline")
            {
                return Some(FailureReason::NoInternet);
            }
            if err_lower.contains("rate limit") || err_lower.contains("too many requests") {
                return Some(FailureReason::RateLimited);
            }
            return Some(FailureReason::Other(err.to_string()));
        }

        None
    }
}

/// Find the session log path for a project.
///
/// Uses `CLAUDE_LOCAL_STATE_DIR` env var if set, otherwise defaults to `~/.claude`.
/// This allows tests to point at a temp directory.
pub fn find_session_log(project_path: &Path, session_id: &str) -> Option<PathBuf> {
    let claude_base = std::env::var("CLAUDE_LOCAL_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".claude"));
    find_session_log_in(project_path, session_id, &claude_base)
}

/// Find the session log path for a project within a specific Claude state directory.
///
/// This is the testable core of `find_session_log`.
pub fn find_session_log_in(
    project_path: &Path,
    session_id: &str,
    claude_base: &Path,
) -> Option<PathBuf> {
    // Claude stores logs in <base>/projects/<hash>/<session>.jsonl
    let claude_dir = claude_base.join("projects");

    // Hash the project path to find the right directory
    let project_hash = hash_project_path(project_path);
    let project_dir = claude_dir.join(&project_hash);

    if !project_dir.exists() {
        return None;
    }

    // Look for session file
    let session_file = project_dir.join(format!("{session_id}.jsonl"));
    if session_file.exists() {
        return Some(session_file);
    }

    // Fallback: find most recent .jsonl file
    std::fs::read_dir(&project_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path())
}

fn hash_project_path(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
#[path = "session_log_tests.rs"]
mod tests;

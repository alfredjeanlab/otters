// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Tmux session adapter

use super::{SessionAdapter, SessionError};
use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;

/// Tmux-based session adapter
#[derive(Clone, Default)]
pub struct TmuxAdapter;

impl TmuxAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionAdapter for TmuxAdapter {
    async fn spawn(
        &self,
        name: &str,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<String, SessionError> {
        let session_id = format!("oj-{}", name);

        // Check if session already exists and clean it up
        let existing = Command::new("tmux")
            .args(["has-session", "-t", &session_id])
            .output()
            .await;

        if existing.map(|o| o.status.success()).unwrap_or(false) {
            tracing::warn!(session_id, "session already exists, killing first");
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &session_id])
                .output()
                .await;
        }

        // Build tmux command
        let mut tmux_cmd = Command::new("tmux");
        tmux_cmd
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(&session_id)
            .arg("-c")
            .arg(cwd);

        // Add environment variables
        for (key, value) in env {
            tmux_cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        tmux_cmd.arg(cmd);

        let output = tmux_cmd
            .output()
            .await
            .map_err(|e| SessionError::SpawnFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SessionError::SpawnFailed(stderr.to_string()));
        }

        Ok(session_id)
    }

    async fn send(&self, id: &str, input: &str) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .arg("send-keys")
            .arg("-t")
            .arg(id)
            .arg(input)
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(SessionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    async fn kill(&self, id: &str) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .arg("kill-session")
            .arg("-t")
            .arg(id)
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            // Session might already be dead, which is fine
        }

        Ok(())
    }

    async fn is_alive(&self, id: &str) -> Result<bool, SessionError> {
        let output = Command::new("tmux")
            .arg("has-session")
            .arg("-t")
            .arg(id)
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        Ok(output.status.success())
    }

    async fn capture_output(&self, id: &str, lines: u32) -> Result<String, SessionError> {
        let output = Command::new("tmux")
            .arg("capture-pane")
            .arg("-t")
            .arg(id)
            .arg("-p")
            .arg("-S")
            .arg(format!("-{}", lines))
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(SessionError::NotFound(id.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn is_process_running(&self, id: &str, pattern: &str) -> Result<bool, SessionError> {
        // Get the pane PID
        let output = Command::new("tmux")
            .args(["list-panes", "-t", id, "-F", "#{pane_pid}"])
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(SessionError::NotFound(id.to_string()));
        }

        let pane_pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if pane_pid.is_empty() {
            return Ok(false);
        }

        // Check for child processes matching pattern (e.g., "claude")
        let output = Command::new("pgrep")
            .args(["-P", &pane_pid, "-f", pattern])
            .output()
            .await
            .map_err(|e| SessionError::CommandFailed(e.to_string()))?;

        Ok(output.status.success())
    }
}

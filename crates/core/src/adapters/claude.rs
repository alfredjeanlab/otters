//! Claude Code adapter implementation

use super::traits::SessionId;
use tokio::process::Command;

/// Claude Code integration helper
#[derive(Clone, Default)]
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Start Claude Code in a tmux session
    pub async fn start_in_session(
        &self,
        session_id: &SessionId,
        prompt: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let mut cmd = "claude".to_string();

        if let Some(prompt) = prompt {
            cmd.push_str(&format!(" --print \"{}\"", prompt.replace('"', "\\\"")));
        }

        Command::new("tmux")
            .args(["send-keys", "-t", &session_id.0, &cmd, "Enter"])
            .output()
            .await?;

        Ok(())
    }

    /// Send a message to Claude in a session
    pub async fn send_message(
        &self,
        session_id: &SessionId,
        message: &str,
    ) -> Result<(), std::io::Error> {
        Command::new("tmux")
            .args(["send-keys", "-t", &session_id.0, message, "Enter"])
            .output()
            .await?;

        Ok(())
    }

    /// Check if Claude is likely running by looking at pane content
    pub async fn is_running(&self, session_id: &SessionId) -> Result<bool, std::io::Error> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", &session_id.0, "-p", "-S", "-10"])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(false);
        }

        let content = String::from_utf8_lossy(&output.stdout);

        // Look for Claude-specific indicators in the output
        let is_running = content.contains("Claude")
            || content.contains("claude>")
            || content.contains("Thinking")
            || content.contains("─────");

        Ok(is_running)
    }
}

//! Real tmux adapter implementation

use super::traits::{SessionAdapter, SessionError, SessionId, SessionInfo};
use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;

/// Real tmux session adapter
#[derive(Clone)]
pub struct TmuxAdapter {
    /// Prefix for session names (e.g., "oj-")
    pub session_prefix: String,
}

impl TmuxAdapter {
    pub fn new(session_prefix: impl Into<String>) -> Self {
        Self {
            session_prefix: session_prefix.into(),
        }
    }

    fn prefixed_name(&self, name: &str) -> String {
        if name.starts_with(&self.session_prefix) {
            name.to_string()
        } else {
            format!("{}{}", self.session_prefix, name)
        }
    }
}

impl Default for TmuxAdapter {
    fn default() -> Self {
        Self::new("oj-")
    }
}

#[async_trait]
impl SessionAdapter for TmuxAdapter {
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError> {
        let session_name = self.prefixed_name(name);

        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session_name, "-c"])
            .arg(cwd)
            .arg(cmd)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("duplicate session") {
                return Err(SessionError::AlreadyExists(session_name));
            }
            return Err(SessionError::SpawnFailed(stderr.into_owned()));
        }

        Ok(SessionId(session_name))
    }

    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .args(["send-keys", "-t", &id.0, input, "Enter"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no server") || stderr.contains("can't find") {
                return Err(SessionError::NotFound(id.clone()));
            }
            return Err(SessionError::SpawnFailed(stderr.into_owned()));
        }

        Ok(())
    }

    async fn kill(&self, id: &SessionId) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .args(["kill-session", "-t", &id.0])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no server") || stderr.contains("can't find") {
                return Err(SessionError::NotFound(id.clone()));
            }
            return Err(SessionError::SpawnFailed(stderr.into_owned()));
        }

        Ok(())
    }

    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError> {
        let output = Command::new("tmux")
            .args(["has-session", "-t", &id.0])
            .output()
            .await?;

        Ok(output.status.success())
    }

    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError> {
        let output = Command::new("tmux")
            .args([
                "capture-pane",
                "-t",
                &id.0,
                "-p",
                "-S",
                &format!("-{}", lines),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no server") || stderr.contains("can't find") {
                return Err(SessionError::NotFound(id.clone()));
            }
            return Err(SessionError::SpawnFailed(stderr.into_owned()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}:#{session_created}"])
            .output()
            .await;

        let output = match output {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new()); // tmux not installed
            }
            Err(e) => return Err(e.into()),
        };

        if !output.status.success() {
            // No sessions or tmux not running
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sessions = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<_> = line.splitn(2, ':').collect();
                if parts.len() != 2 {
                    return None;
                }
                let name = parts[0];
                if !name.starts_with(&self.session_prefix) {
                    return None;
                }

                let created_ts = parts[1].parse::<i64>().unwrap_or(0);
                let created_at = chrono::DateTime::from_timestamp(created_ts, 0)
                    .unwrap_or_else(chrono::Utc::now);

                Some(SessionInfo {
                    id: SessionId(name.to_string()),
                    name: name.to_string(),
                    created_at,
                    attached: false, // Would need another tmux query to determine
                })
            })
            .collect();

        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixed_name_adds_prefix() {
        let adapter = TmuxAdapter::new("oj-");
        assert_eq!(adapter.prefixed_name("test"), "oj-test");
        assert_eq!(adapter.prefixed_name("oj-test"), "oj-test");
    }
}

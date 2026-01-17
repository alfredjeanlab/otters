//! Real wk (issue tracker) adapter implementation

use super::traits::{IssueAdapter, IssueError, IssueInfo};
use async_trait::async_trait;
use tokio::process::Command;

/// Real wk issue adapter
#[derive(Clone, Default)]
pub struct WkAdapter;

impl WkAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl IssueAdapter for WkAdapter {
    async fn list(&self, labels: Option<&[&str]>) -> Result<Vec<IssueInfo>, IssueError> {
        let mut cmd = Command::new("wk");
        cmd.arg("list").arg("--format=json");

        if let Some(labels) = labels {
            for label in labels {
                cmd.arg("--label").arg(label);
            }
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let issues: Vec<IssueInfo> = serde_json::from_str(&stdout).unwrap_or_default();

        Ok(issues)
    }

    async fn get(&self, id: &str) -> Result<IssueInfo, IssueError> {
        let output = Command::new("wk")
            .args(["show", id, "--format=json"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Err(IssueError::NotFound(id.to_string()));
            }
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let issue: IssueInfo =
            serde_json::from_str(&stdout).map_err(|e| IssueError::CommandFailed(e.to_string()))?;

        Ok(issue)
    }

    async fn start(&self, id: &str) -> Result<(), IssueError> {
        let output = Command::new("wk").args(["start", id]).output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Err(IssueError::NotFound(id.to_string()));
            }
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        Ok(())
    }

    async fn done(&self, id: &str) -> Result<(), IssueError> {
        let output = Command::new("wk").args(["done", id]).output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Err(IssueError::NotFound(id.to_string()));
            }
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        Ok(())
    }

    async fn note(&self, id: &str, message: &str) -> Result<(), IssueError> {
        let output = Command::new("wk")
            .args(["note", id, message])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Err(IssueError::NotFound(id.to_string()));
            }
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        Ok(())
    }

    async fn create(
        &self,
        kind: &str,
        title: &str,
        labels: &[&str],
        parent: Option<&str>,
    ) -> Result<String, IssueError> {
        let mut cmd = Command::new("wk");
        cmd.args(["new", kind, title]);

        for label in labels {
            cmd.arg("-l").arg(label);
        }

        if let Some(parent) = parent {
            cmd.arg("--parent").arg(parent);
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IssueError::CommandFailed(stderr.into_owned()));
        }

        // Parse the created issue ID from output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let id = stdout
            .lines()
            .find_map(|line| {
                // Look for patterns like "Created issue-123" or "otters-123"
                if line.contains("Created") || line.contains("created") {
                    line.split_whitespace().last().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| stdout.trim().to_string());

        Ok(id)
    }
}

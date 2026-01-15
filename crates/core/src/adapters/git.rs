// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Real git adapter implementation

use super::traits::{MergeResult, RepoAdapter, RepoError, WorktreeInfo};
use crate::effect::MergeStrategy;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Real git repository adapter
#[derive(Clone)]
pub struct GitAdapter {
    /// Root of the repository
    pub repo_root: PathBuf,
}

impl GitAdapter {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    async fn get_conflicted_files(&self, path: &Path) -> Result<Vec<String>, RepoError> {
        let output = Command::new("git")
            .current_dir(path)
            .args(["diff", "--name-only", "--diff-filter=U"])
            .output()
            .await?;

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect())
    }
}

impl Default for GitAdapter {
    fn default() -> Self {
        Self::new(".")
    }
}

#[async_trait]
impl RepoAdapter for GitAdapter {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        let output = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "add", "-b", branch])
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepoError::WorktreeError(stderr.into_owned()));
        }

        Ok(())
    }

    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError> {
        let output = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "remove"])
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepoError::WorktreeError(stderr.into_owned()));
        }

        Ok(())
    }

    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError> {
        let output = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepoError::CommandFailed {
                cmd: "git worktree list".to_string(),
                stderr: stderr.into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch = String::new();
        let mut current_head = String::new();
        let mut current_locked = false;

        for line in stdout.lines() {
            if let Some(worktree_path) = line.strip_prefix("worktree ") {
                if let Some(path) = current_path.take() {
                    worktrees.push(WorktreeInfo {
                        path,
                        branch: std::mem::take(&mut current_branch),
                        head: std::mem::take(&mut current_head),
                        locked: current_locked,
                    });
                    current_locked = false;
                }
                current_path = Some(PathBuf::from(worktree_path));
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                current_head = head.to_string();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                current_branch = branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string();
            } else if line == "locked" {
                current_locked = true;
            }
        }

        if let Some(path) = current_path {
            worktrees.push(WorktreeInfo {
                path,
                branch: current_branch,
                head: current_head,
                locked: current_locked,
            });
        }

        Ok(worktrees)
    }

    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError> {
        let output = Command::new("git")
            .current_dir(path)
            .args(["status", "--porcelain"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepoError::CommandFailed {
                cmd: "git status".to_string(),
                stderr: stderr.into_owned(),
            });
        }

        Ok(output.stdout.is_empty())
    }

    async fn merge(
        &self,
        path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult, RepoError> {
        let args: Vec<&str> = match strategy {
            MergeStrategy::FastForward => vec!["merge", "--ff-only", branch],
            MergeStrategy::Rebase => vec!["rebase", branch],
            MergeStrategy::Merge => vec!["merge", "--no-ff", branch],
        };

        let output = Command::new("git")
            .current_dir(path)
            .args(&args)
            .output()
            .await?;

        if output.status.success() {
            return Ok(match strategy {
                MergeStrategy::FastForward => MergeResult::FastForwarded,
                MergeStrategy::Rebase => MergeResult::Rebased,
                MergeStrategy::Merge => MergeResult::Success,
            });
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);

        if combined.contains("conflict") || combined.contains("CONFLICT") {
            return Ok(MergeResult::Conflict {
                files: self.get_conflicted_files(path).await?,
            });
        }

        Err(RepoError::CommandFailed {
            cmd: args.join(" "),
            stderr: stderr.into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_adapter_default_repo_root() {
        let adapter = GitAdapter::default();
        assert_eq!(adapter.repo_root, PathBuf::from("."));
    }
}

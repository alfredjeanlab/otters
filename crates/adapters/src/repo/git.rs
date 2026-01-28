// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Git repository adapter

use super::{RepoAdapter, RepoError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Git-based repository adapter
#[derive(Clone)]
pub struct GitAdapter {
    root: PathBuf,
}

impl GitAdapter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl RepoAdapter for GitAdapter {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .arg("worktree")
            .arg("add")
            .arg(path)
            .arg("-b")
            .arg(branch)
            .output()
            .await
            .map_err(|e| RepoError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already exists") {
                return Err(RepoError::BranchExists(branch.to_string()));
            }
            return Err(RepoError::CommandFailed(stderr.to_string()));
        }

        Ok(())
    }

    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(path)
            .output()
            .await
            .map_err(|e| RepoError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("is not a working tree") {
                return Err(RepoError::WorktreeNotFound(path.display().to_string()));
            }
            return Err(RepoError::CommandFailed(stderr.to_string()));
        }

        Ok(())
    }

    async fn worktree_list(&self) -> Result<Vec<String>, RepoError> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .arg("worktree")
            .arg("list")
            .arg("--porcelain")
            .output()
            .await
            .map_err(|e| RepoError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RepoError::CommandFailed(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let worktrees: Vec<String> = stdout
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .map(|line| line.strip_prefix("worktree ").unwrap_or(line).to_string())
            .collect();

        Ok(worktrees)
    }
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Fake repository adapter for testing
#![cfg_attr(coverage_nightly, coverage(off))]

use super::{RepoAdapter, RepoError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Recorded repo call
#[derive(Debug, Clone)]
pub enum RepoCall {
    AddWorktree { branch: String, path: PathBuf },
    RemoveWorktree { path: PathBuf },
    ListWorktrees,
}

/// Fake worktree
#[derive(Debug, Clone)]
pub struct FakeWorktree {
    pub path: PathBuf,
    pub branch: String,
}

/// Fake repository adapter for testing
#[derive(Clone, Default)]
pub struct FakeRepoAdapter {
    worktrees: Arc<Mutex<HashMap<PathBuf, FakeWorktree>>>,
    calls: Arc<Mutex<Vec<RepoCall>>>,
}

impl FakeRepoAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all recorded calls
    pub fn calls(&self) -> Vec<RepoCall> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get a worktree by path
    pub fn get_worktree(&self, path: &Path) -> Option<FakeWorktree> {
        self.worktrees
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .cloned()
    }
}

#[async_trait]
impl RepoAdapter for FakeRepoAdapter {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(RepoCall::AddWorktree {
                branch: branch.to_string(),
                path: path.to_path_buf(),
            });

        let mut worktrees = self.worktrees.lock().unwrap_or_else(|e| e.into_inner());

        // Check if branch already exists
        if worktrees.values().any(|w| w.branch == branch) {
            return Err(RepoError::BranchExists(branch.to_string()));
        }

        worktrees.insert(
            path.to_path_buf(),
            FakeWorktree {
                path: path.to_path_buf(),
                branch: branch.to_string(),
            },
        );

        Ok(())
    }

    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(RepoCall::RemoveWorktree {
                path: path.to_path_buf(),
            });

        let mut worktrees = self.worktrees.lock().unwrap_or_else(|e| e.into_inner());

        if worktrees.remove(path).is_none() {
            return Err(RepoError::WorktreeNotFound(path.display().to_string()));
        }

        Ok(())
    }

    async fn worktree_list(&self) -> Result<Vec<String>, RepoError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(RepoCall::ListWorktrees);

        let worktrees = self.worktrees.lock().unwrap_or_else(|e| e.into_inner());
        Ok(worktrees.keys().map(|p| p.display().to_string()).collect())
    }
}

#[cfg(test)]
#[path = "fake_tests.rs"]
mod tests;

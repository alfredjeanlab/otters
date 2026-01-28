// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Repository management adapters

mod git;
mod noop;

pub use git::GitAdapter;
pub use noop::NoOpRepoAdapter;

// Test support - only compiled for tests or when explicitly requested
#[cfg(any(test, feature = "test-support"))]
mod fake;
#[cfg(any(test, feature = "test-support"))]
pub use fake::{FakeRepoAdapter, FakeWorktree, RepoCall};

use async_trait::async_trait;
use std::path::Path;
use thiserror::Error;

/// Errors from repo operations
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("worktree not found: {0}")]
    WorktreeNotFound(String),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("branch already exists: {0}")]
    BranchExists(String),
}

/// Adapter for repository operations (git worktrees, etc.)
#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync + 'static {
    /// Add a git worktree
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError>;

    /// Remove a git worktree
    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError>;

    /// List worktrees
    async fn worktree_list(&self) -> Result<Vec<String>, RepoError>;
}

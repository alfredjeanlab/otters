// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! No-op repo adapter for when git operations are disabled.

use super::{RepoAdapter, RepoError};
use async_trait::async_trait;
use std::path::Path;

/// Repo adapter that does nothing.
///
/// Used when git operations are disabled or in minimal deployments.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpRepoAdapter;

impl NoOpRepoAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RepoAdapter for NoOpRepoAdapter {
    async fn worktree_add(&self, _branch: &str, _path: &Path) -> Result<(), RepoError> {
        Ok(())
    }

    async fn worktree_remove(&self, _path: &Path) -> Result<(), RepoError> {
        Ok(())
    }

    async fn worktree_list(&self) -> Result<Vec<String>, RepoError> {
        Ok(Vec::new())
    }
}

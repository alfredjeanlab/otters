// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Adapter trait definitions for external integrations

use crate::effect::MergeStrategy;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

// =============================================================================
// Session Adapter (tmux)
// =============================================================================

/// Unique identifier for a tmux session
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Information about a tmux session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub attached: bool,
}

/// Errors from session operations
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session already exists: {0}")]
    AlreadyExists(String),
    #[error("session not found: {0}")]
    NotFound(SessionId),
    #[error("failed to spawn session: {0}")]
    SpawnFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Adapter for tmux session management
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync + 'static {
    /// Spawn a new tmux session
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError>;

    /// Send input to a session
    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError>;

    /// Kill a session
    async fn kill(&self, id: &SessionId) -> Result<(), SessionError>;

    /// Check if a session is alive
    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError>;

    /// Capture the pane content (last N lines)
    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError>;

    /// List all sessions
    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError>;
}

// =============================================================================
// Repo Adapter (git)
// =============================================================================

/// Information about a git worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: std::path::PathBuf,
    pub branch: String,
    pub head: String,
    pub locked: bool,
}

/// Result of a merge operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeResult {
    Success,
    FastForwarded,
    Rebased,
    Conflict { files: Vec<String> },
}

/// Errors from repo operations
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("worktree error: {0}")]
    WorktreeError(String),
    #[error("branch not found: {0}")]
    BranchNotFound(String),
    #[error("working directory not clean")]
    NotClean,
    #[error("command failed: {cmd} - {stderr}")]
    CommandFailed { cmd: String, stderr: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Adapter for git operations
#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync + 'static {
    /// Create a new worktree with a new branch
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError>;

    /// Remove a worktree
    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError>;

    /// List all worktrees
    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError>;

    /// Check if working directory is clean
    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError>;

    /// Perform a merge operation
    async fn merge(
        &self,
        path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult, RepoError>;
}

// =============================================================================
// Issue Adapter (wk CLI)
// =============================================================================

/// Information about an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub labels: Vec<String>,
}

/// Errors from issue operations
#[derive(Debug, Error)]
pub enum IssueError {
    #[error("issue not found: {0}")]
    NotFound(String),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Adapter for issue tracking (wk CLI)
#[async_trait]
pub trait IssueAdapter: Clone + Send + Sync + 'static {
    /// List issues with optional filters
    async fn list(&self, labels: Option<&[&str]>) -> Result<Vec<IssueInfo>, IssueError>;

    /// Get a single issue by ID
    async fn get(&self, id: &str) -> Result<IssueInfo, IssueError>;

    /// Start working on an issue
    async fn start(&self, id: &str) -> Result<(), IssueError>;

    /// Mark an issue as done
    async fn done(&self, id: &str) -> Result<(), IssueError>;

    /// Add a note to an issue
    async fn note(&self, id: &str, message: &str) -> Result<(), IssueError>;

    /// Create a new issue
    async fn create(
        &self,
        kind: &str,
        title: &str,
        labels: &[&str],
        parent: Option<&str>,
    ) -> Result<String, IssueError>;
}

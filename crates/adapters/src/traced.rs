// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Traced adapter wrappers for consistent observability

use crate::repo::{RepoAdapter, RepoError};
use crate::session::{SessionAdapter, SessionError};
use async_trait::async_trait;
use std::path::Path;

/// Wrapper that adds tracing to any SessionAdapter
#[derive(Clone)]
pub struct TracedSessionAdapter<S> {
    inner: S,
}

impl<S> TracedSessionAdapter<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<S: SessionAdapter> SessionAdapter for TracedSessionAdapter<S> {
    async fn spawn(
        &self,
        name: &str,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<String, SessionError> {
        let span = tracing::info_span!("session.spawn", name, cwd = %cwd.display());
        let _guard = span.enter();

        tracing::info!(cmd, env_count = env.len(), "starting");

        // Precondition: cwd must exist
        if !cwd.exists() {
            tracing::error!("working directory does not exist");
            return Err(SessionError::SpawnFailed(format!(
                "working directory does not exist: {}",
                cwd.display()
            )));
        }

        let start = std::time::Instant::now();
        let result = self.inner.spawn(name, cwd, cmd, env).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(session_id) => tracing::info!(
                session_id,
                elapsed_ms = elapsed.as_millis() as u64,
                "session created"
            ),
            Err(e) => tracing::error!(
                elapsed_ms = elapsed.as_millis() as u64,
                error = %e,
                "spawn failed"
            ),
        }

        result
    }

    async fn send(&self, id: &str, input: &str) -> Result<(), SessionError> {
        let span = tracing::info_span!("session.send", id);
        let _guard = span.enter();

        tracing::debug!(input_len = input.len(), "sending");
        let result = self.inner.send(id, input).await;

        match &result {
            Ok(()) => tracing::debug!("sent"),
            Err(e) => tracing::error!(error = %e, "send failed"),
        }

        result
    }

    async fn kill(&self, id: &str) -> Result<(), SessionError> {
        let span = tracing::info_span!("session.kill", id);
        let _guard = span.enter();

        let result = self.inner.kill(id).await;
        // kill() failing is often acceptable (session already gone)
        match &result {
            Ok(()) => tracing::info!("killed"),
            Err(e) => tracing::warn!(error = %e, "kill failed (may be expected)"),
        }

        result
    }

    async fn is_alive(&self, id: &str) -> Result<bool, SessionError> {
        let result = self.inner.is_alive(id).await;
        tracing::trace!(id, alive = ?result.as_ref().ok(), "checked");
        result
    }

    async fn capture_output(&self, id: &str, lines: u32) -> Result<String, SessionError> {
        let span = tracing::info_span!("session.capture", id, lines);
        let _guard = span.enter();

        let result = self.inner.capture_output(id, lines).await;
        tracing::debug!(
            captured_len = result.as_ref().map(|s| s.len()).ok(),
            "captured"
        );
        result
    }

    async fn is_process_running(&self, id: &str, pattern: &str) -> Result<bool, SessionError> {
        self.inner.is_process_running(id, pattern).await
    }
}

/// Wrapper that adds tracing to any RepoAdapter
#[derive(Clone)]
pub struct TracedRepoAdapter<R> {
    inner: R,
}

impl<R> TracedRepoAdapter<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<R: RepoAdapter> RepoAdapter for TracedRepoAdapter<R> {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        let span = tracing::info_span!("repo.worktree_add", branch, path = %path.display());
        let _guard = span.enter();

        tracing::info!("adding worktree");

        // Precondition: parent directory should exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tracing::error!(parent = %parent.display(), "parent directory does not exist");
                return Err(RepoError::CommandFailed(format!(
                    "parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }

        let start = std::time::Instant::now();
        let result = self.inner.worktree_add(branch, path).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(()) => tracing::info!(elapsed_ms = elapsed.as_millis() as u64, "worktree added"),
            Err(e) => {
                tracing::error!(elapsed_ms = elapsed.as_millis() as u64, error = %e, "failed")
            }
        }

        result
    }

    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError> {
        let span = tracing::info_span!("repo.worktree_remove", path = %path.display());
        let _guard = span.enter();

        let result = self.inner.worktree_remove(path).await;
        match &result {
            Ok(()) => tracing::info!("worktree removed"),
            Err(e) => tracing::warn!(error = %e, "worktree remove failed"),
        }

        result
    }

    async fn worktree_list(&self) -> Result<Vec<String>, RepoError> {
        let result = self.inner.worktree_list().await;
        tracing::trace!(
            count = result.as_ref().map(|v| v.len()).ok(),
            "listed worktrees"
        );
        result
    }
}

#[cfg(test)]
#[path = "traced_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Effect executor that processes effects from state machines

use crate::adapters::{IssueAdapter, NotifyAdapter, RepoAdapter, SessionAdapter};
use crate::effect::{Effect, LogLevel};
use crate::storage::WalStore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("session error: {0}")]
    Session(#[from] crate::adapters::SessionError),
    #[error("repo error: {0}")]
    Repo(#[from] crate::adapters::RepoError),
    #[error("issue error: {0}")]
    Issue(#[from] crate::adapters::IssueError),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::WalStoreError),
}

/// Adapters bundle for effect execution
pub trait Adapters: Clone + Send + Sync + 'static {
    type Sessions: SessionAdapter;
    type Repos: RepoAdapter;
    type Issues: IssueAdapter;
    type Notify: NotifyAdapter;

    fn sessions(&self) -> Self::Sessions;
    fn repos(&self) -> Self::Repos;
    fn issues(&self) -> Self::Issues;
    fn notify(&self) -> Self::Notify;
}

/// Executes effects from state machines
pub struct Executor<A: Adapters> {
    adapters: A,
    #[allow(dead_code)] // Epic 7: Storage & Durability - effect persistence
    store: WalStore,
}

impl<A: Adapters> Executor<A> {
    pub fn new(adapters: A, store: WalStore) -> Self {
        Self { adapters, store }
    }

    /// Execute a single effect
    pub async fn execute(&self, effect: Effect) -> Result<(), ExecutorError> {
        match effect {
            Effect::Emit(event) => {
                tracing::info!(?event, "event emitted");
                Ok(())
            }
            Effect::SpawnSession { name, cwd, command } => {
                self.adapters
                    .sessions()
                    .spawn(&name, &cwd, &command)
                    .await?;
                Ok(())
            }
            Effect::KillSession { name } => {
                let id = crate::adapters::SessionId(name);
                self.adapters.sessions().kill(&id).await?;
                Ok(())
            }
            Effect::SendToSession { name, input } => {
                let id = crate::adapters::SessionId(name);
                self.adapters.sessions().send(&id, &input).await?;
                Ok(())
            }
            Effect::CreateWorktree { branch, path } => {
                self.adapters.repos().worktree_add(&branch, &path).await?;
                Ok(())
            }
            Effect::RemoveWorktree { path } => {
                self.adapters.repos().worktree_remove(&path).await?;
                Ok(())
            }
            Effect::Merge {
                path,
                branch,
                strategy,
            } => {
                self.adapters
                    .repos()
                    .merge(&path, &branch, strategy)
                    .await?;
                Ok(())
            }
            Effect::SaveState { kind, id } => {
                // State saving is handled by the caller
                tracing::debug!(kind, id, "save state requested");
                Ok(())
            }
            Effect::Log { level, message } => {
                match level {
                    LogLevel::Debug => tracing::debug!("{}", message),
                    LogLevel::Info => tracing::info!("{}", message),
                    LogLevel::Warn => tracing::warn!("{}", message),
                    LogLevel::Error => tracing::error!("{}", message),
                }
                Ok(())
            }
            Effect::SaveCheckpoint {
                pipeline_id,
                checkpoint,
            } => {
                tracing::debug!(
                    ?pipeline_id,
                    sequence = checkpoint.sequence,
                    "checkpoint saved"
                );
                Ok(())
            }
            Effect::ScheduleTask { task_id, delay } => {
                tracing::debug!(?task_id, ?delay, "task scheduled");
                Ok(())
            }
            Effect::CancelTask { task_id } => {
                tracing::debug!(?task_id, "task cancelled");
                Ok(())
            }
            Effect::SetTimer { id, duration } => {
                tracing::debug!(id, ?duration, "timer set");
                Ok(())
            }
            Effect::CancelTimer { id } => {
                tracing::debug!(id, "timer cancelled");
                Ok(())
            }
        }
    }

    /// Execute multiple effects
    pub async fn execute_all(&self, effects: Vec<Effect>) -> Result<(), ExecutorError> {
        for effect in effects {
            self.execute(effect).await?;
        }
        Ok(())
    }
}

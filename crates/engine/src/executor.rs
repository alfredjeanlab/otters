// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Effect executor

use crate::{RuntimeDeps, Scheduler};
use oj_adapters::{NotifyAdapter, RepoAdapter, SessionAdapter};
use oj_core::{Effect, Event};
use oj_storage::{MaterializedState, Wal};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors that can occur during effect execution
#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error("session error: {0}")]
    Session(#[from] oj_adapters::session::SessionError),
    #[error("repo error: {0}")]
    Repo(#[from] oj_adapters::repo::RepoError),
    #[error("notify error: {0}")]
    Notify(#[from] oj_adapters::notify::NotifyError),
    #[error("storage error: {0}")]
    Storage(#[from] oj_storage::WalError),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),
    #[error("shell execution error: {0}")]
    Shell(String),
}

/// Executes effects using the configured adapters
pub struct Executor<S, R, N> {
    sessions: S,
    repos: R,
    notify: N,
    wal: Arc<Mutex<Wal>>,
    state: Arc<Mutex<MaterializedState>>,
    scheduler: Arc<Mutex<Scheduler>>,
    clock: oj_core::SystemClock,
}

impl<S, R, N> Executor<S, R, N>
where
    S: SessionAdapter,
    R: RepoAdapter,
    N: NotifyAdapter,
{
    /// Create a new executor
    pub fn new(deps: RuntimeDeps<S, R, N>, scheduler: Arc<Mutex<Scheduler>>) -> Self {
        Self {
            sessions: deps.sessions,
            repos: deps.repos,
            notify: deps.notify,
            wal: deps.wal,
            state: deps.state,
            scheduler,
            clock: oj_core::SystemClock,
        }
    }

    /// Execute a single effect with tracing
    ///
    /// Returns an optional event that should be fed back into the event loop.
    pub async fn execute(&self, effect: Effect) -> Result<Option<Event>, ExecuteError> {
        use oj_core::TracedEffect;

        let op_name = effect.name();
        let span = tracing::info_span!("effect", effect = op_name);
        let _guard = span.enter();

        tracing::info!(fields = ?effect.fields(), "executing");

        let start = std::time::Instant::now();
        let result = self.execute_inner(effect).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(event) => tracing::info!(
                elapsed_ms = elapsed.as_millis() as u64,
                has_event = event.is_some(),
                "completed"
            ),
            Err(e) => tracing::error!(
                elapsed_ms = elapsed.as_millis() as u64,
                error = %e,
                "failed"
            ),
        }

        result
    }

    /// Inner execution logic for a single effect
    async fn execute_inner(&self, effect: Effect) -> Result<Option<Event>, ExecuteError> {
        match effect {
            Effect::Emit { event } => {
                // Log the event and send notification
                let message = format!("{:?}", event);
                eprintln!("[EVENT] {}", message);
                self.notify.send("events", &message).await?;
                Ok(None)
            }

            Effect::Spawn {
                workspace_id,
                command,
                env,
                cwd,
            } => {
                let workspace_path = {
                    let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    state
                        .workspaces
                        .get(&workspace_id)
                        .map(|w| w.path.clone())
                        .ok_or_else(|| ExecuteError::WorkspaceNotFound(workspace_id.clone()))?
                };

                // Use cwd override if provided, otherwise default to workspace path
                let effective_cwd = cwd.unwrap_or(workspace_path);

                // TracedSessionAdapter handles logging and precondition validation
                self.sessions
                    .spawn(&workspace_id, &effective_cwd, &command, &env)
                    .await?;

                Ok(None)
            }

            Effect::Send { session_id, input } => {
                self.sessions.send(&session_id, &input).await?;
                Ok(None)
            }

            Effect::Kill { session_id } => {
                self.sessions.kill(&session_id).await?;
                Ok(None)
            }

            Effect::WorktreeAdd { branch, path } => {
                // TracedRepoAdapter handles logging and precondition validation
                self.repos.worktree_add(&branch, &path).await?;
                Ok(None)
            }

            Effect::WorktreeRemove { path } => {
                self.repos.worktree_remove(&path).await?;
                Ok(None)
            }

            Effect::SetTimer { id, duration } => {
                let now = oj_core::Clock::now(&self.clock);
                self.scheduler
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .set_timer(id, duration, now);
                Ok(None)
            }

            Effect::CancelTimer { id } => {
                self.scheduler
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cancel_timer(&id);
                Ok(None)
            }

            Effect::Persist { operation } => {
                {
                    let mut wal = self.wal.lock().unwrap_or_else(|e| e.into_inner());
                    wal.append(&operation)?;
                }
                {
                    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.apply(&operation);
                }
                Ok(None)
            }

            Effect::Shell {
                pipeline_id,
                phase,
                command,
                cwd,
                env,
            } => {
                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&command)
                    .current_dir(&cwd)
                    .envs(&env)
                    .output()
                    .await
                    .map_err(|e| ExecuteError::Shell(e.to_string()))?;

                let exit_code = output.status.code().unwrap_or(-1);

                // Log output
                if !output.stdout.is_empty() {
                    tracing::info!(
                        pipeline_id,
                        phase,
                        stdout = %String::from_utf8_lossy(&output.stdout),
                        "shell stdout"
                    );
                }
                if !output.stderr.is_empty() {
                    tracing::warn!(
                        pipeline_id,
                        phase,
                        stderr = %String::from_utf8_lossy(&output.stderr),
                        "shell stderr"
                    );
                }

                // Return event to feed back into loop
                Ok(Some(Event::ShellCompleted {
                    pipeline_id,
                    phase,
                    exit_code,
                }))
            }

            Effect::Notify { title, message } => {
                // Send desktop notification
                // Use terminal-notifier on macOS, notify-send on Linux
                #[cfg(target_os = "macos")]
                {
                    let _ = tokio::process::Command::new("terminal-notifier")
                        .args(["-title", &title, "-message", &message, "-sound", "default"])
                        .output()
                        .await;
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = tokio::process::Command::new("notify-send")
                        .args([&title, &message])
                        .output()
                        .await;
                }
                tracing::info!(title, message, "desktop notification sent");
                Ok(None)
            }
        }
    }

    /// Execute multiple effects in order
    ///
    /// Returns any events that were produced by effects (to be fed back into the event loop).
    pub async fn execute_all(&self, effects: Vec<Effect>) -> Result<Vec<Event>, ExecuteError> {
        let mut result_events = Vec::new();
        for effect in effects {
            if let Some(event) = self.execute(effect).await? {
                result_events.push(event);
            }
        }
        Ok(result_events)
    }

    /// Get a reference to the state
    pub fn state(&self) -> Arc<Mutex<MaterializedState>> {
        Arc::clone(&self.state)
    }

    /// Get a reference to the scheduler
    pub fn scheduler(&self) -> Arc<Mutex<Scheduler>> {
        Arc::clone(&self.scheduler)
    }

    /// Get the worktree root path
    pub fn worktree_root(&self) -> PathBuf {
        PathBuf::from("worktrees")
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

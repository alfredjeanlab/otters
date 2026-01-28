// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Runtime for the Otter Jobs engine

use crate::monitor::{self, ActionEffects};
use crate::phases;
use crate::session_log::{find_session_log, SessionLogWatcher, SessionState};
use crate::{error::RuntimeError, Executor, Scheduler};
use oj_adapters::{NotifyAdapter, RepoAdapter, SessionAdapter};
use oj_core::{Clock, Effect, Event, IdGen, Operation, PhaseStatus, Pipeline};
use oj_runbook::Runbook;
use oj_storage::{MaterializedState, Wal};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Runtime path configuration
pub struct RuntimeConfig {
    /// Root directory of the project
    pub project_root: PathBuf,
    /// Directory where worktrees are created
    pub worktree_root: PathBuf,
}

/// Runtime adapter dependencies
pub struct RuntimeDeps<S, R, N> {
    pub sessions: S,
    pub repos: R,
    pub notify: N,
    pub wal: Arc<Mutex<Wal>>,
    pub state: Arc<Mutex<MaterializedState>>,
}

/// Runtime that coordinates the system
pub struct Runtime<S, R, N, C: Clock, I: IdGen> {
    executor: Executor<S, R, N>,
    runbook: Runbook,
    clock: C,
    id_gen: I,
    project_root: PathBuf,
    worktree_root: PathBuf,
    /// Session log watchers, keyed by pipeline ID
    session_watchers: Mutex<HashMap<String, SessionLogWatcher>>,
}

impl<S, R, N, C, I> Runtime<S, R, N, C, I>
where
    S: SessionAdapter,
    R: RepoAdapter,
    N: NotifyAdapter,
    C: Clock,
    I: IdGen,
{
    /// Create a new runtime
    pub fn new(
        deps: RuntimeDeps<S, R, N>,
        runbook: Runbook,
        clock: C,
        id_gen: I,
        config: RuntimeConfig,
    ) -> Self {
        Self {
            executor: Executor::new(deps, Arc::new(Mutex::new(Scheduler::new()))),
            runbook,
            clock,
            id_gen,
            project_root: config.project_root,
            worktree_root: config.worktree_root,
            session_watchers: Mutex::new(HashMap::new()),
        }
    }

    /// Handle an incoming event
    ///
    /// Returns any events that were produced by effects (to be fed back into the event loop).
    pub async fn handle_event(&self, event: Event) -> Result<Vec<Event>, RuntimeError> {
        let mut result_events = Vec::new();

        match &event {
            Event::CommandInvoked { command, args } => {
                result_events.extend(self.handle_command(command, args).await?);
            }

            Event::SessionExited {
                session_id,
                exit_code,
            } => {
                result_events.extend(self.handle_session_exit(session_id, *exit_code).await?);
            }

            Event::AgentDone { pipeline_id } | Event::AgentError { pipeline_id, .. } => {
                result_events.extend(self.handle_agent_event(pipeline_id, &event).await?);
            }

            Event::ShellCompleted {
                pipeline_id,
                phase,
                exit_code,
            } => {
                result_events.extend(
                    self.handle_shell_completed(pipeline_id, phase, *exit_code)
                        .await?,
                );
            }

            Event::Timer { id } => {
                result_events.extend(self.handle_timer(id).await?);
            }

            Event::Custom { name, data } => {
                result_events.extend(self.handle_custom_event(name, data).await?);
            }

            _ => {
                // Other events are informational
            }
        }

        Ok(result_events)
    }

    async fn handle_command(
        &self,
        command: &str,
        args: &HashMap<String, String>,
    ) -> Result<Vec<Event>, RuntimeError> {
        use oj_runbook::RunDirective;

        let cmd_def = self
            .runbook
            .get_command(command)
            .ok_or_else(|| RuntimeError::CommandNotFound(command.to_string()))?;

        match &cmd_def.run {
            RunDirective::Pipeline {
                pipeline: pipeline_name,
            } => {
                let pipeline_def = self
                    .runbook
                    .get_pipeline(pipeline_name)
                    .ok_or_else(|| RuntimeError::PipelineDefNotFound(pipeline_name.to_string()))?;

                let pipeline_id = self.id_gen.next();
                let name = args
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| pipeline_id.clone());
                let workspace_path = self.worktree_root.join(&name);
                let initial_phase = pipeline_def
                    .first_phase()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "init".to_string());

                let effects = vec![
                    Effect::Persist {
                        operation: Operation::WorkspaceCreate {
                            id: pipeline_id.clone(),
                            path: workspace_path.clone(),
                            branch: format!("feature/{}", name),
                        },
                    },
                    Effect::WorktreeAdd {
                        branch: format!("feature/{}", name),
                        path: workspace_path.clone(),
                    },
                    Effect::Persist {
                        operation: Operation::PipelineCreate {
                            id: pipeline_id.clone(),
                            kind: pipeline_name.to_string(),
                            name: name.clone(),
                            inputs: args.clone(),
                            initial_phase,
                        },
                    },
                    Effect::Emit {
                        event: Event::Custom {
                            name: "pipeline:created".to_string(),
                            data: serde_json::json!({"id": pipeline_id, "name": name, "kind": pipeline_name}),
                        },
                    },
                ];

                let mut result_events = self.executor.execute_all(effects).await?;
                if let Some(first_phase) = pipeline_def.first_phase() {
                    result_events.extend(
                        self.start_phase(&pipeline_id, &first_phase.name, args, &workspace_path)
                            .await?,
                    );
                }
                Ok(result_events)
            }
            RunDirective::Shell(cmd) => Err(RuntimeError::InvalidRunDirective {
                context: "command".to_string(),
                directive: format!("shell ({})", cmd),
            }),
            RunDirective::Agent { agent } => Err(RuntimeError::InvalidRunDirective {
                context: "command".to_string(),
                directive: format!("agent ({})", agent),
            }),
            RunDirective::Strategy { strategy } => Err(RuntimeError::InvalidRunDirective {
                context: "command".to_string(),
                directive: format!("strategy ({})", strategy),
            }),
        }
    }

    async fn handle_session_exit(
        &self,
        session_id: &str,
        exit_code: i32,
    ) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = {
            let state = self.executor.state();
            let guard = state.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .pipelines
                .values()
                .find(|p| p.session_id.as_ref() == Some(&session_id.to_string()))
                .cloned()
        };
        let Some(pipeline) = pipeline else {
            return Ok(vec![]);
        };

        let event = Event::SessionExited {
            session_id: session_id.to_string(),
            exit_code,
        };
        let (new_pipeline, effects) = pipeline.transition(&event, &self.clock);
        let mut events = self.executor.execute_all(effects).await?;
        if new_pipeline.phase_status == PhaseStatus::Completed {
            events.extend(self.advance_pipeline(&new_pipeline).await?);
        }
        Ok(events)
    }

    async fn handle_agent_event(
        &self,
        pipeline_id: &str,
        event: &Event,
    ) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = self
            .get_pipeline(pipeline_id)
            .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;

        match event {
            Event::AgentDone { .. } => self.advance_pipeline(&pipeline).await,
            Event::AgentError { error, .. } => self.fail_pipeline(&pipeline, error).await,
            _ => Ok(vec![]),
        }
    }

    /// Handle custom events (delegates to events module)
    async fn handle_custom_event(
        &self,
        name: &str,
        data: &serde_json::Value,
    ) -> Result<Vec<Event>, RuntimeError> {
        crate::events::handle_custom_event(&self.executor, name, data, |id| self.get_pipeline(id))
            .await
    }

    /// Handle timer events
    async fn handle_timer(&self, id: &str) -> Result<Vec<Event>, RuntimeError> {
        // Session monitor timer: session:<pipeline_id>:check
        if id.starts_with("session:") && id.ends_with(":check") {
            let pipeline_id = id
                .strip_prefix("session:")
                .and_then(|s| s.strip_suffix(":check"))
                .unwrap_or(id);
            return self.handle_session_monitor(pipeline_id).await;
        }
        Ok(vec![])
    }

    /// Handle a shell command completing
    async fn handle_shell_completed(
        &self,
        pipeline_id: &str,
        phase: &str,
        exit_code: i32,
    ) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = {
            let state = self.executor.state();
            let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
            state_guard.pipelines.get(pipeline_id).cloned()
        };

        let Some(pipeline) = pipeline else {
            return Err(RuntimeError::PipelineNotFound(pipeline_id.to_string()));
        };

        // Verify we're in the expected phase
        if pipeline.phase != phase {
            tracing::warn!(
                pipeline_id,
                expected = phase,
                actual = %pipeline.phase,
                "shell completed for unexpected phase"
            );
            return Ok(vec![]);
        }

        if exit_code == 0 {
            self.advance_pipeline(&pipeline).await
        } else {
            self.fail_pipeline(&pipeline, &format!("shell exited with code {}", exit_code))
                .await
        }
    }

    /// Start a pipeline phase by dispatching based on RunDirective
    async fn start_phase(
        &self,
        pipeline_id: &str,
        phase_name: &str,
        inputs: &HashMap<String, String>,
        workspace_path: &Path,
    ) -> Result<Vec<Event>, RuntimeError> {
        use oj_runbook::RunDirective;

        // Get the pipeline definition to find the phase
        let pipeline = {
            let state = self.executor.state();
            let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
            state_guard.pipelines.get(pipeline_id).cloned()
        };

        let Some(pipeline) = pipeline else {
            return Err(RuntimeError::PipelineNotFound(pipeline_id.to_string()));
        };

        let pipeline_def = self
            .runbook
            .get_pipeline(&pipeline.kind)
            .ok_or_else(|| RuntimeError::PipelineDefNotFound(pipeline.kind.clone()))?;

        let phase_def = pipeline_def.get_phase(phase_name).ok_or_else(|| {
            RuntimeError::PipelineNotFound(format!("phase {} not found", phase_name))
        })?;

        let mut result_events = Vec::new();

        // Mark phase as running
        let effects = phases::phase_start_effects(pipeline_id, phase_name);
        result_events.extend(self.executor.execute_all(effects).await?);

        // Dispatch based on run directive
        match &phase_def.run {
            RunDirective::Shell(cmd) => {
                // Build template variables
                let mut vars = inputs.clone();
                vars.insert("pipeline_id".to_string(), pipeline_id.to_string());
                vars.insert("name".to_string(), pipeline.name.clone());
                vars.insert(
                    "workspace".to_string(),
                    workspace_path.display().to_string(),
                );

                let command = oj_runbook::interpolate(cmd, &vars);

                let effects = vec![Effect::Shell {
                    pipeline_id: pipeline_id.to_string(),
                    phase: phase_name.to_string(),
                    command,
                    cwd: workspace_path.to_path_buf(),
                    env: HashMap::new(),
                }];

                result_events.extend(self.executor.execute_all(effects).await?);
            }

            RunDirective::Agent { agent } => {
                result_events.extend(self.spawn_agent(pipeline_id, agent, inputs).await?);
            }

            RunDirective::Pipeline { pipeline } => {
                return Err(RuntimeError::InvalidRunDirective {
                    context: format!("phase {}", phase_name),
                    directive: format!("nested pipeline ({})", pipeline),
                });
            }

            RunDirective::Strategy { strategy } => {
                return Err(RuntimeError::InvalidRunDirective {
                    context: format!("phase {}", phase_name),
                    directive: format!("strategy ({})", strategy),
                });
            }
        }

        Ok(result_events)
    }

    /// Advance pipeline to next phase
    async fn advance_pipeline(&self, pipeline: &Pipeline) -> Result<Vec<Event>, RuntimeError> {
        // If current phase is terminal (done/failed), complete the pipeline
        // This handles the case where a "done" phase has a run command that just finished
        if pipeline.is_terminal() {
            return self.complete_pipeline(pipeline).await;
        }

        let pipeline_def = self.runbook.get_pipeline(&pipeline.kind);
        let current_phase_def = pipeline_def
            .as_ref()
            .and_then(|p| p.get_phase(&pipeline.phase));

        // Determine next phase: explicit next > sequential order > complete
        let next_phase_name = if let Some(phase_def) = current_phase_def {
            if let Some(next) = &phase_def.next {
                Some(next.clone())
            } else if let Some(p) = pipeline_def.as_ref() {
                p.next_phase(&pipeline.phase).map(|pd| pd.name.clone())
            } else {
                None
            }
        } else {
            // Phase not found in runbook - no next phase
            None
        };

        let mut result_events = Vec::new();

        match next_phase_name {
            Some(next_phase) => {
                let effects = phases::phase_transition_effects(pipeline, &next_phase);
                result_events.extend(self.executor.execute_all(effects).await?);

                let has_phase_def = pipeline_def
                    .as_ref()
                    .and_then(|p| p.get_phase(&next_phase))
                    .is_some();
                let is_terminal = next_phase == "done" || next_phase == "failed";

                if !has_phase_def && is_terminal {
                    result_events.extend(self.complete_pipeline(pipeline).await?);
                } else {
                    result_events.extend(
                        self.start_phase(
                            &pipeline.id,
                            &next_phase,
                            &pipeline.inputs,
                            &self.workspace_path(pipeline),
                        )
                        .await?,
                    );
                }
            }
            None => {
                let effects = phases::phase_transition_effects(pipeline, "done");
                result_events.extend(self.executor.execute_all(effects).await?);
                result_events.extend(self.complete_pipeline(pipeline).await?);
            }
        }

        Ok(result_events)
    }

    /// Handle pipeline failure
    async fn fail_pipeline(
        &self,
        pipeline: &Pipeline,
        error: &str,
    ) -> Result<Vec<Event>, RuntimeError> {
        let pipeline_def = self.runbook.get_pipeline(&pipeline.kind);
        let on_fail = pipeline_def
            .as_ref()
            .and_then(|p| p.get_phase(&pipeline.phase))
            .and_then(|p| p.on_fail.as_ref());

        let mut result_events = Vec::new();

        if let Some(on_fail) = on_fail {
            let effects = phases::failure_transition_effects(pipeline, on_fail, error);
            result_events.extend(self.executor.execute_all(effects).await?);
            result_events.extend(
                self.start_phase(
                    &pipeline.id,
                    on_fail,
                    &pipeline.inputs,
                    &self.workspace_path(pipeline),
                )
                .await?,
            );
        } else {
            let effects = phases::failure_effects(pipeline, error);
            result_events.extend(self.executor.execute_all(effects).await?);
        }

        Ok(result_events)
    }

    /// Complete a pipeline
    async fn complete_pipeline(&self, pipeline: &Pipeline) -> Result<Vec<Event>, RuntimeError> {
        let effects = phases::completion_effects(pipeline);
        Ok(self.executor.execute_all(effects).await?)
    }

    /// Spawn an agent for a pipeline
    async fn spawn_agent(
        &self,
        pipeline_id: &str,
        agent_name: &str,
        inputs: &HashMap<String, String>,
    ) -> Result<Vec<Event>, RuntimeError> {
        let agent_def = self
            .runbook
            .get_agent(agent_name)
            .ok_or_else(|| RuntimeError::AgentNotFound(agent_name.to_string()))?;

        let pipeline = self
            .get_pipeline(pipeline_id)
            .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;
        let workspace_path = self.workspace_path(&pipeline);

        let mut effects = crate::spawn::build_spawn_effects(
            agent_def,
            &pipeline,
            pipeline_id,
            agent_name,
            inputs,
            &workspace_path,
            &self.project_root,
        )?;

        // Start session monitoring after spawn
        effects.push(self.start_session_monitor(pipeline_id));

        Ok(self.executor.execute_all(effects).await?)
    }

    /// Get current pipelines
    pub fn pipelines(&self) -> HashMap<String, Pipeline> {
        let state = self.executor.state();
        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        state_guard.pipelines.clone()
    }

    /// Get a specific pipeline by ID or unique prefix
    pub fn get_pipeline(&self, id: &str) -> Option<Pipeline> {
        let state = self.executor.state();
        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        state_guard.get_pipeline(id).cloned()
    }

    /// Get workspace path for a pipeline
    fn workspace_path(&self, pipeline: &Pipeline) -> PathBuf {
        pipeline
            .workspace_path
            .clone()
            .unwrap_or_else(|| self.worktree_root.join(&pipeline.name))
    }

    /// Start session monitoring for an agent
    ///
    /// Sets a timer that will periodically check the session log state.
    fn start_session_monitor(&self, pipeline_id: &str) -> Effect {
        Effect::SetTimer {
            id: format!("session:{}:check", pipeline_id),
            duration: Duration::from_secs(10),
        }
    }

    /// Handle session monitor timer
    async fn handle_session_monitor(&self, pipeline_id: &str) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = self
            .get_pipeline(pipeline_id)
            .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;

        if pipeline.is_terminal() {
            return Ok(vec![]);
        }

        let agent_def = monitor::get_agent_def(&self.runbook, &pipeline)?.clone();
        let workspace_path = pipeline
            .workspace_path
            .as_ref()
            .ok_or_else(|| RuntimeError::PipelineNotFound("no workspace".into()))?;

        let session_id = pipeline.session_id.clone().unwrap_or_default();
        let log_path = match find_session_log(workspace_path, &session_id) {
            Some(path) => path,
            None => {
                self.executor
                    .execute(self.start_session_monitor(pipeline_id))
                    .await?;
                return Ok(vec![]);
            }
        };

        let state = {
            let mut watchers = self
                .session_watchers
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let watcher = watchers
                .entry(pipeline_id.to_string())
                .or_insert_with(|| SessionLogWatcher::new(log_path));
            watcher.check_state()
        };

        match state {
            SessionState::Working | SessionState::Unknown => {
                self.executor
                    .execute(self.start_session_monitor(pipeline_id))
                    .await?;
                Ok(vec![])
            }
            SessionState::WaitingForInput => {
                let effects = monitor::build_action_effects(
                    &pipeline,
                    &agent_def,
                    &agent_def.on_idle,
                    "idle",
                    &pipeline.inputs,
                )?;
                self.execute_action_effects(&pipeline, effects).await
            }
            SessionState::Failed(reason) => {
                let error_msg = monitor::failure_to_message(&reason);
                let error_type = monitor::failure_to_error_type(&reason);
                tracing::error!(pipeline_id = %pipeline.id, error = error_msg, "agent error");
                let action = agent_def.on_error.action_for(error_type.as_ref());
                let effects = monitor::build_action_effects(
                    &pipeline,
                    &agent_def,
                    &action,
                    error_msg,
                    &pipeline.inputs,
                )?;
                self.execute_action_effects(&pipeline, effects).await
            }
        }
    }

    /// Handle Claude process exit (on_exit trigger)
    pub async fn handle_claude_exited(
        &self,
        pipeline_id: &str,
    ) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = self
            .get_pipeline(pipeline_id)
            .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;

        if pipeline.is_terminal() {
            return Ok(vec![]);
        }

        let agent_def = monitor::get_agent_def(&self.runbook, &pipeline)?.clone();
        tracing::info!(pipeline_id = %pipeline.id, "claude process exited");

        let effects = monitor::build_action_effects(
            &pipeline,
            &agent_def,
            &agent_def.on_exit,
            "exit",
            &pipeline.inputs,
        )?;
        self.execute_action_effects(&pipeline, effects).await
    }

    /// Handle tmux session exit (session is gone)
    pub async fn handle_tmux_exited(&self, pipeline_id: &str) -> Result<Vec<Event>, RuntimeError> {
        let pipeline = self
            .get_pipeline(pipeline_id)
            .ok_or_else(|| RuntimeError::PipelineNotFound(pipeline_id.to_string()))?;

        tracing::error!(pipeline_id = %pipeline.id, "tmux session exited unexpectedly");
        self.fail_pipeline(&pipeline, "tmux session exited").await
    }

    async fn execute_action_effects(
        &self,
        pipeline: &Pipeline,
        effects: ActionEffects,
    ) -> Result<Vec<Event>, RuntimeError> {
        match effects {
            ActionEffects::Nudge { mut effects } => {
                effects.push(self.start_session_monitor(&pipeline.id));
                Ok(self.executor.execute_all(effects).await?)
            }
            ActionEffects::AdvancePipeline => self.advance_pipeline(pipeline).await,
            ActionEffects::FailPipeline { error } => self.fail_pipeline(pipeline, &error).await,
            ActionEffects::Restart {
                kill_session,
                workspace_path,
                agent_name,
                inputs,
            } => {
                self.kill_and_respawn(
                    kill_session,
                    Some(workspace_path),
                    &pipeline.id,
                    &agent_name,
                    &inputs,
                )
                .await
            }
            ActionEffects::Recover {
                kill_session,
                agent_name,
                inputs,
            } => {
                self.kill_and_respawn(kill_session, None, &pipeline.id, &agent_name, &inputs)
                    .await
            }
            ActionEffects::Escalate { effects } => Ok(self.executor.execute_all(effects).await?),
        }
    }

    async fn kill_and_respawn(
        &self,
        kill_session: Option<String>,
        workspace_path: Option<Option<std::path::PathBuf>>,
        pipeline_id: &str,
        agent_name: &str,
        inputs: &HashMap<String, String>,
    ) -> Result<Vec<Event>, RuntimeError> {
        if let Some(sid) = kill_session {
            self.executor
                .execute(Effect::Kill { session_id: sid })
                .await?;
        }
        if let Some(Some(path)) = workspace_path {
            self.executor
                .execute(Effect::WorktreeRemove { path })
                .await?;
        }
        self.spawn_agent(pipeline_id, agent_name, inputs).await
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

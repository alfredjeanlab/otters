// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Agent spawning functionality

use crate::error::RuntimeError;
use crate::ExecuteError;
use oj_core::{Effect, Operation, Pipeline};
use oj_runbook::AgentDef;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Session monitoring interval (10 seconds)
pub const SESSION_MONITOR_INTERVAL: Duration = Duration::from_secs(10);

/// Spawn an agent for a pipeline
///
/// Returns the effects to execute for spawning the agent.
pub fn build_spawn_effects(
    agent_def: &AgentDef,
    pipeline: &Pipeline,
    pipeline_id: &str,
    agent_name: &str,
    inputs: &HashMap<String, String>,
    workspace_path: &Path,
    project_root: &Path,
) -> Result<Vec<Effect>, RuntimeError> {
    tracing::debug!(
        pipeline_id,
        agent_name,
        workspace_path = %workspace_path.display(),
        project_root = %project_root.display(),
        "building spawn effects"
    );

    // Build variables for interpolation
    let mut vars = inputs.clone();
    vars.insert("pipeline_id".to_string(), pipeline_id.to_string());
    vars.insert("name".to_string(), pipeline.name.clone());
    vars.insert(
        "workspace".to_string(),
        workspace_path.display().to_string(),
    );

    // Get prompt
    let prompt = agent_def
        .get_prompt(&vars)
        .map_err(|e| RuntimeError::PromptError {
            agent: agent_name.to_string(),
            message: e.to_string(),
        })?;

    // Prepare workspace with CLAUDE.md and settings
    tracing::debug!(workspace_path = %workspace_path.display(), "preparing workspace");
    crate::workspace::prepare_for_agent(workspace_path, project_root, &pipeline.name, &prompt)
        .map_err(|e| {
            tracing::error!(error = %e, "workspace preparation failed");
            RuntimeError::Execute(ExecuteError::Shell(e.to_string()))
        })?;

    let command = agent_def.build_command(&vars);
    let mut env = agent_def.build_env(&vars);

    // Always set OJ_PROJECT_ROOT so agents can find the daemon
    env.push((
        "OJ_PROJECT_ROOT".to_string(),
        project_root.display().to_string(),
    ));

    // Inherit OJ_SOCKET_DIR if set (for test isolation)
    if let Ok(socket_dir) = std::env::var("OJ_SOCKET_DIR") {
        env.push(("OJ_SOCKET_DIR".to_string(), socket_dir));
    }

    // Determine effective working directory from agent cwd config
    let effective_cwd = agent_def.cwd.as_ref().map(|cwd_template| {
        let cwd_str = oj_runbook::interpolate(cwd_template, &vars);
        if Path::new(&cwd_str).is_absolute() {
            PathBuf::from(cwd_str)
        } else {
            workspace_path.join(cwd_str)
        }
    });

    tracing::info!(
        pipeline_id,
        agent_name,
        command,
        effective_cwd = ?effective_cwd,
        "spawn effects prepared"
    );

    Ok(vec![
        Effect::Persist {
            operation: Operation::SessionCreate {
                id: pipeline_id.to_string(),
                pipeline_id: pipeline_id.to_string(),
            },
        },
        Effect::Spawn {
            workspace_id: pipeline_id.to_string(),
            command,
            env,
            cwd: effective_cwd,
        },
        // Start session monitoring timer
        Effect::SetTimer {
            id: format!("session:{}:check", pipeline_id),
            duration: SESSION_MONITOR_INTERVAL,
        },
    ])
}

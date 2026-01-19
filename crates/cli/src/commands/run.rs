// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Run command - start pipelines using runbook definitions

use crate::adapters::make_engine;
use anyhow::{bail, Result};
use clap::Parser;
use oj_core::workspace::{Workspace, WorkspaceId};
use oj_core::{Adapters, RepoAdapter};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

/// Run a pipeline from a runbook definition
#[derive(Parser)]
pub struct RunCommand {
    /// Runbook name (e.g., "build", "bugfix")
    runbook: String,
    /// Pipeline name within the runbook (defaults to runbook-specific default)
    #[arg(short, long)]
    pipeline: Option<String>,
    /// Pipeline inputs as key=value pairs
    #[arg(short, long, value_parser = parse_key_val)]
    input: Vec<(String, String)>,
}

pub async fn handle(command: RunCommand) -> Result<()> {
    run_pipeline(command.runbook, command.pipeline, command.input).await
}

async fn run_pipeline(
    runbook_name: String,
    pipeline_name: Option<String>,
    inputs: Vec<(String, String)>,
) -> Result<()> {
    let mut engine = make_engine()?;
    engine.load()?;

    // Check if runbooks are loaded
    if engine.runbook_registry().is_none() {
        bail!(
            "No runbooks loaded. Ensure runbooks directory exists and contains valid runbook files."
        );
    }

    // Determine pipeline name - use runbook-specific defaults
    let pipeline_name = pipeline_name.unwrap_or_else(|| match runbook_name.as_str() {
        "build" => "build".to_string(),
        "bugfix" => "fix".to_string(),
        _ => runbook_name.clone(),
    });

    // Collect inputs into BTreeMap
    let inputs_map: BTreeMap<String, String> = inputs.into_iter().collect();

    // Generate unique ID and workspace config based on inputs
    let (pipeline_id, workspace_id, workspace_path, branch) =
        generate_ids_and_workspace(&runbook_name, &inputs_map);

    // Create workspace first
    let workspace = Workspace::new(&workspace_id, &pipeline_id, workspace_path.clone(), &branch);
    engine.add_workspace(workspace)?;

    // Create the pipeline from runbook (engine handles persistence)
    let mut pipeline = match engine.create_runbook_pipeline(
        &pipeline_id,
        &runbook_name,
        &pipeline_name,
        inputs_map.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            bail!(
                "Failed to create pipeline from runbook '{}': {}",
                runbook_name,
                e
            );
        }
    };

    // Set workspace on pipeline and re-add to persist the update
    pipeline.workspace_id = Some(WorkspaceId(workspace_id.clone()));
    engine.add_pipeline(pipeline.clone())?;

    // Create git worktree
    engine
        .adapters()
        .repos()
        .worktree_add(&branch, &workspace_path)
        .await?;

    // Generate CLAUDE.md in workspace directory
    let claude_md = generate_claude_md(&runbook_name, &pipeline_id, &inputs_map);
    std::fs::write(workspace_path.join("CLAUDE.md"), claude_md)?;

    // Start the first phase task
    let task_id = engine
        .start_phase_task(&oj_core::pipeline::PipelineId(pipeline_id.clone()))
        .await?;

    println!("Started {} pipeline '{}'", runbook_name, pipeline_id);
    println!("Workspace: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!("Task: {}", task_id.0);
    println!();
    println!(
        "Session started. Attach with: tmux attach -t oj-{}-init",
        pipeline_id
    );

    Ok(())
}

fn generate_ids_and_workspace(
    runbook_name: &str,
    inputs: &BTreeMap<String, String>,
) -> (String, String, PathBuf, String) {
    match runbook_name {
        "build" => {
            let name = inputs.get("name").map(|s| s.as_str()).unwrap_or_else(|| {
                // Fallback if no name provided
                "unnamed"
            });
            let pipeline_id = format!("{}-{}", runbook_name, name);
            let workspace_id = format!("build-{}", name);
            let workspace_path = PathBuf::from(format!(".worktrees/build-{}", name));
            let branch = format!("build-{}", name);
            (pipeline_id, workspace_id, workspace_path, branch)
        }
        "bugfix" => {
            let bug_id = inputs.get("bug").map(|s| s.as_str()).unwrap_or_else(|| {
                // Fallback if no bug provided
                "unknown"
            });
            let pipeline_id = format!("{}-{}", runbook_name, bug_id);
            let workspace_id = format!("bugfix-{}", bug_id);
            let workspace_path = PathBuf::from(format!(".worktrees/bugfix-{}", bug_id));
            let branch = format!("bugfix-{}", bug_id);
            (pipeline_id, workspace_id, workspace_path, branch)
        }
        _ => {
            // Generic fallback using timestamp
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let pipeline_id = format!("{}-{}", runbook_name, timestamp);
            let workspace_id = pipeline_id.clone();
            let workspace_path = PathBuf::from(format!(".worktrees/{}", pipeline_id));
            let branch = pipeline_id.clone();
            (pipeline_id, workspace_id, workspace_path, branch)
        }
    }
}

fn generate_claude_md(
    runbook_name: &str,
    pipeline_id: &str,
    inputs: &BTreeMap<String, String>,
) -> String {
    match runbook_name {
        "build" => {
            let name = inputs
                .get("name")
                .map(|s| s.as_str())
                .unwrap_or(pipeline_id);
            let prompt = inputs
                .get("prompt")
                .map(|s| s.as_str())
                .unwrap_or("No prompt provided");
            format!(
                r#"# {name}

## Task
{prompt}

## Signaling

When you complete a phase, signal completion:
```bash
oj done
```

If you encounter an error you cannot resolve:
```bash
oj done --error "description of the issue"
```

## Environment

- `OTTER_PIPELINE`: {pipeline_id}
- `OTTER_TASK`: Current task ID
- `OTTER_PHASE`: Current phase (init, plan, decompose, execute, merge)

## Guidelines

1. Work only within this workspace directory
2. Commit your changes before signaling completion
3. Signal `oj done` when the phase objective is complete
"#
            )
        }
        "bugfix" => {
            let bug_id = inputs.get("bug").map(|s| s.as_str()).unwrap_or("unknown");
            format!(
                r#"# Bugfix for Issue #{bug_id}

## Task
Fix the issue described in issue #{bug_id}.

## Signaling

When you complete a phase, signal completion:
```bash
oj done
```

If you encounter an error you cannot resolve:
```bash
oj done --error "description of the issue"
```

## Environment

- `OTTER_PIPELINE`: {pipeline_id}
- `OTTER_TASK`: Current task ID
- `OTTER_PHASE`: Current phase (init, fix, verify, merge, cleanup)

## Guidelines

1. Understand the issue thoroughly before making changes
2. Write tests that reproduce the bug
3. Fix the bug
4. Verify all tests pass
5. Commit your changes before signaling completion
"#
            )
        }
        _ => {
            format!(
                r#"# {pipeline_id}

## Task
Run the {runbook_name} pipeline.

## Signaling

When you complete a phase, signal completion:
```bash
oj done
```

If you encounter an error you cannot resolve:
```bash
oj done --error "description of the issue"
```

## Environment

- `OTTER_PIPELINE`: {pipeline_id}
- `OTTER_TASK`: Current task ID
- `OTTER_PHASE`: Current phase

## Guidelines

1. Work only within this workspace directory
2. Commit your changes before signaling completion
3. Signal `oj done` when the phase objective is complete
"#
            )
        }
    }
}

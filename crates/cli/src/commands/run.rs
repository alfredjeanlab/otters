// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Run command - start pipelines using the Engine

use crate::adapters::make_engine;
use anyhow::Result;
use clap::Subcommand;
use oj_core::pipeline::Pipeline;
use oj_core::workspace::{Workspace, WorkspaceId};
use oj_core::{Adapters, RepoAdapter};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum RunCommand {
    /// Start a build pipeline
    Build {
        /// Name for this build
        name: String,
        /// Prompt describing what to build
        prompt: String,
    },
    /// Start a bugfix pipeline
    Bugfix {
        /// Issue ID to fix
        id: String,
    },
}

pub async fn handle(command: RunCommand) -> Result<()> {
    match command {
        RunCommand::Build { name, prompt } => run_build(name, prompt).await,
        RunCommand::Bugfix { id } => run_bugfix(id).await,
    }
}

async fn run_build(name: String, prompt: String) -> Result<()> {
    let mut engine = make_engine()?;
    engine.load()?;

    // Create workspace
    let workspace_id = format!("build-{}", name);
    let workspace_path = PathBuf::from(format!(".worktrees/build-{}", name));
    let branch = format!("build-{}", name);

    let workspace = Workspace::new(&workspace_id, &name, workspace_path.clone(), &branch);
    engine.add_workspace(workspace)?;

    // Create pipeline with workspace reference
    let pipeline_id = format!("build-{}", name);
    let pipeline = Pipeline::new_build(&pipeline_id, &name, &prompt)
        .with_workspace(WorkspaceId(workspace_id.clone()));
    engine.add_pipeline(pipeline.clone())?;

    // Create git worktree first (it creates the directory)
    engine
        .adapters()
        .repos()
        .worktree_add(&branch, &workspace_path)
        .await?;

    // Generate CLAUDE.md in workspace directory (after worktree exists)
    let claude_md = generate_claude_md(&name, &prompt, &pipeline_id);
    std::fs::write(workspace_path.join("CLAUDE.md"), claude_md)?;

    // Start the first phase task (spawns tmux session)
    let task_id = engine
        .start_phase_task(&oj_core::pipeline::PipelineId(pipeline_id.clone()))
        .await?;

    println!("Started build pipeline '{}'", name);
    println!("Workspace: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!("Task: {}", task_id.0);
    println!();
    println!(
        "Session started. Attach with: tmux attach -t oj-{}-init",
        name
    );

    Ok(())
}

async fn run_bugfix(id: String) -> Result<()> {
    let mut engine = make_engine()?;
    engine.load()?;

    let name = format!("bugfix-{}", id);
    let workspace_path = PathBuf::from(format!(".worktrees/{}", name));
    let branch = format!("bugfix-{}", id);

    let workspace = Workspace::new(&name, &name, workspace_path.clone(), &branch);
    engine.add_workspace(workspace)?;

    let pipeline =
        Pipeline::new_bugfix(&name, &name, &id).with_workspace(WorkspaceId(name.clone()));
    engine.add_pipeline(pipeline.clone())?;

    // Create git worktree first (it creates the directory)
    engine
        .adapters()
        .repos()
        .worktree_add(&branch, &workspace_path)
        .await?;

    // Generate CLAUDE.md in workspace directory (after worktree exists)
    let claude_md = generate_bugfix_claude_md(&id, &pipeline.id.0);
    std::fs::write(workspace_path.join("CLAUDE.md"), claude_md)?;

    let task_id = engine.start_phase_task(&pipeline.id).await?;

    println!("Started bugfix pipeline for issue #{}", id);
    println!("Workspace: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!("Task: {}", task_id.0);
    println!();
    println!(
        "Session started. Attach with: tmux attach -t oj-{}-init",
        name
    );

    Ok(())
}

fn generate_claude_md(name: &str, prompt: &str, pipeline_id: &str) -> String {
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

fn generate_bugfix_claude_md(issue_id: &str, pipeline_id: &str) -> String {
    format!(
        r#"# Bugfix for Issue #{issue_id}

## Task
Fix the issue described in issue #{issue_id}.

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

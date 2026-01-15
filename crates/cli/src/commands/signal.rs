// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Signal commands (done, checkpoint) routed through Engine

use crate::adapters::make_engine;
use anyhow::{bail, Result};
use oj_core::workspace::WorkspaceId;

pub async fn handle_done(error: Option<String>) -> Result<()> {
    let mut engine = make_engine()?;
    engine.load()?;

    let workspace_id = detect_workspace_id()?;

    engine.signal_done(&workspace_id, error.clone()).await?;

    let pipeline = engine
        .find_pipeline_by_workspace(&workspace_id)
        .ok_or_else(|| anyhow::anyhow!("No pipeline found for workspace"))?;

    match &error {
        Some(reason) => {
            println!("Pipeline '{}' phase failed: {}", pipeline.name, reason);
            println!("Phase: {} -> failed", pipeline.phase.name());
        }
        None => {
            println!("Pipeline '{}' phase complete", pipeline.name);
            println!("Current phase: {}", pipeline.phase.name());

            if pipeline.phase.is_terminal() {
                println!("\nPipeline completed successfully!");
            } else {
                println!(
                    "\nNext: Continue working on the {} phase",
                    pipeline.phase.name()
                );
            }
        }
    }

    Ok(())
}

pub async fn handle_checkpoint() -> Result<()> {
    let mut engine = make_engine()?;
    engine.load()?;

    let workspace_id = detect_workspace_id()?;

    engine.signal_checkpoint(&workspace_id).await?;

    let pipeline = engine
        .find_pipeline_by_workspace(&workspace_id)
        .ok_or_else(|| anyhow::anyhow!("No pipeline found for workspace"))?;

    println!(
        "Checkpoint saved for pipeline '{}' at phase '{}'",
        pipeline.name,
        pipeline.phase.name()
    );

    Ok(())
}

/// Detect workspace ID from environment or current directory
fn detect_workspace_id() -> Result<WorkspaceId> {
    // Try OTTER_PIPELINE env var first
    if let Ok(pipeline_id) = std::env::var("OTTER_PIPELINE") {
        return Ok(WorkspaceId(pipeline_id));
    }

    // Try OTTER_TASK env var
    if let Ok(task) = std::env::var("OTTER_TASK") {
        return Ok(WorkspaceId(task));
    }

    // Fall back to detecting from current directory
    let cwd = std::env::current_dir()?;

    // Check if we're in a .worktrees subdirectory
    if let Some(name) = cwd.file_name().and_then(|s| s.to_str()) {
        if cwd
            .parent()
            .map(|p| p.ends_with(".worktrees"))
            .unwrap_or(false)
        {
            return Ok(WorkspaceId(name.to_string()));
        }
    }

    // Try to find by scanning parent directories for .build/operations
    let mut dir = cwd.clone();
    for _ in 0..5 {
        let store_path = dir.join(".build/operations/pipelines");
        if store_path.exists() {
            // We found the root, now try to match cwd to a workspace
            if let Ok(rel_path) = cwd.strip_prefix(&dir) {
                let components: Vec<_> = rel_path.components().collect();
                if components.len() >= 2 {
                    let first = components[0].as_os_str().to_string_lossy();
                    if first == ".worktrees" {
                        return Ok(WorkspaceId(
                            components[1].as_os_str().to_string_lossy().to_string(),
                        ));
                    }
                }
            }
            break;
        }
        if !dir.pop() {
            break;
        }
    }

    bail!(
        "Could not detect workspace. Set OTTER_PIPELINE or run from within a workspace directory."
    )
}

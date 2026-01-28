// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Workspace preparation for agent execution

use std::fs;
use std::io;
use std::path::Path;

/// Prepare workspace for agent execution
///
/// Creates the workspace directory if needed, writes CLAUDE.md with the agent
/// instructions, and copies project settings if they exist.
pub fn prepare_for_agent(
    workspace_path: &Path,
    project_root: &Path,
    pipeline_name: &str,
    prompt: &str,
) -> io::Result<()> {
    // Ensure workspace exists
    fs::create_dir_all(workspace_path)?;

    // 1. Write CLAUDE.md with agent instructions
    let claude_md = workspace_path.join("CLAUDE.md");
    let content = format!(
        "# {name}\n\n\
         {prompt}\n\n\
         ## Completion\n\n\
         When done, run: `oj done`\n\
         On error, run: `oj done --error \"description\"`\n",
        name = pipeline_name,
        prompt = prompt,
    );
    fs::write(&claude_md, content)?;

    // 2. Copy settings if they exist
    let project_settings = project_root.join(".claude/settings.json");
    if project_settings.exists() {
        let claude_dir = workspace_path.join(".claude");
        fs::create_dir_all(&claude_dir)?;
        fs::copy(&project_settings, claude_dir.join("settings.local.json"))?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;

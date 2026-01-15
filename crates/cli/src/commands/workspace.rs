// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Workspace commands

use anyhow::bail;
use clap::Subcommand;
use oj_core::storage::JsonStore;
use oj_core::workspace::{Workspace, WorkspaceState};
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// List all workspaces
    List,
    /// Create a new workspace
    Create {
        /// Type of workspace (build, bugfix)
        kind: String,
        /// Name for the workspace
        name: String,
    },
    /// Show details of a workspace
    Show {
        /// Workspace name
        name: String,
    },
    /// Delete a workspace
    Delete {
        /// Workspace name
        name: String,
        /// Force deletion even if dirty
        #[arg(long)]
        force: bool,
    },
}

#[derive(Serialize)]
struct WorkspaceInfo {
    id: String,
    name: String,
    branch: String,
    state: String,
    path: String,
}

impl fmt::Display for WorkspaceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<20} {:<20} {:<10} {}",
            self.name, self.branch, self.state, self.path
        )
    }
}

pub async fn handle(command: WorkspaceCommand) -> anyhow::Result<()> {
    match command {
        WorkspaceCommand::List => list_workspaces().await,
        WorkspaceCommand::Create { kind, name } => create_workspace(kind, name).await,
        WorkspaceCommand::Show { name } => show_workspace(name).await,
        WorkspaceCommand::Delete { name, force } => delete_workspace(name, force).await,
    }
}

async fn list_workspaces() -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let ids = store.list_workspaces()?;

    if ids.is_empty() {
        println!("No workspaces found.");
        return Ok(());
    }

    println!("{:<20} {:<20} {:<10} PATH", "NAME", "BRANCH", "STATE");
    println!("{}", "-".repeat(70));

    for id in ids {
        if let Ok(workspace) = store.load_workspace(&id) {
            let state_str = match &workspace.state {
                WorkspaceState::Creating => "creating",
                WorkspaceState::Ready => "ready",
                WorkspaceState::InUse { .. } => "in_use",
                WorkspaceState::Dirty => "dirty",
                WorkspaceState::Stale => "stale",
            };
            let info = WorkspaceInfo {
                id: workspace.id.0,
                name: workspace.name,
                branch: workspace.branch,
                state: state_str.to_string(),
                path: workspace.path.display().to_string(),
            };
            println!("{}", info);
        }
    }

    Ok(())
}

async fn create_workspace(kind: String, name: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    let workspace_id = format!("{}-{}", kind, name);
    let workspace_path = PathBuf::from(format!(".worktrees/{}-{}", kind, name));
    let branch = format!("{}-{}", kind, name);

    let workspace = Workspace::new(&workspace_id, &name, workspace_path.clone(), &branch);
    store.save_workspace(&workspace)?;

    println!("Created workspace '{}'", workspace_id);
    println!("Path: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!();
    println!("To create the git worktree:");
    println!(
        "  git worktree add {} -b {}",
        workspace_path.display(),
        branch
    );

    Ok(())
}

async fn show_workspace(name: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let workspace = store.load_workspace(&name)?;

    println!("Workspace: {}", workspace.name);
    println!("ID: {}", workspace.id.0);
    println!("Path: {}", workspace.path.display());
    println!("Branch: {}", workspace.branch);
    println!("State: {:?}", workspace.state);
    println!("Created: {}", workspace.created_at);

    Ok(())
}

async fn delete_workspace(name: String, force: bool) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let workspace = store.load_workspace(&name)?;

    if matches!(workspace.state, WorkspaceState::Dirty) && !force {
        bail!(
            "Workspace '{}' has uncommitted changes. Use --force to delete anyway.",
            name
        );
    }

    store.delete("workspaces", &name)?;
    println!("Deleted workspace '{}'", name);
    println!();
    println!("To remove the git worktree:");
    println!("  git worktree remove {}", workspace.path.display());

    Ok(())
}

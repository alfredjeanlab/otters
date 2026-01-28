// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj session` - Session management commands

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List all sessions
    List,
    /// Send input to a session
    Send {
        /// Session ID
        id: String,
        /// Input to send
        input: String,
    },
    /// Attach to a session (opens tmux)
    Attach {
        /// Session ID
        id: String,
    },
}

/// Attach to a tmux session
pub fn attach(id: &str) -> anyhow::Result<()> {
    let session_name = format!("oj-{}", id);
    let status = std::process::Command::new("tmux")
        .args(["attach", "-t", &session_name])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to attach to session {}", session_name);
    }
    Ok(())
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! oj - Otter Jobs CLI
//!
//! A CLI tool for managing automated workflows with Claude Code.

use clap::{Parser, Subcommand};
use clap_complete::Shell;

mod adapters;
mod commands;
mod completions;
mod error;
mod output;

#[derive(Parser)]
#[command(name = "oj")]
#[command(about = "Otter Jobs - Agentic development orchestration")]
#[command(
    long_about = "oj orchestrates AI-assisted development workflows, managing \
                        workspaces, sessions, and pipelines for automated coding tasks."
)]
#[command(version)]
#[command(after_help = "EXAMPLES:
    oj run build --input name=auth --input prompt=\"Add authentication\"
    oj run bugfix --input bug=123
    oj pipeline list --active
    oj workspace list
    oj completions bash > ~/.local/share/bash-completion/completions/oj")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format (text or json)
    #[arg(long, global = true, default_value = "text")]
    format: output::OutputFormat,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a pipeline from a runbook definition
    Run(commands::run::RunCommand),
    /// Manage pipelines
    Pipeline {
        #[command(subcommand)]
        command: commands::pipeline::PipelineCommand,
    },
    /// Manage workspaces
    Workspace {
        #[command(subcommand)]
        command: commands::workspace::WorkspaceCommand,
    },
    /// Manage sessions
    Session {
        #[command(subcommand)]
        command: commands::session::SessionCommand,
    },
    /// Manage the merge queue
    Queue {
        #[command(subcommand)]
        command: commands::queue::QueueCommand,
    },
    /// Signal completion of current phase
    Done {
        /// Mark as failed with this error message
        #[arg(long)]
        error: Option<String>,
    },
    /// Save checkpoint and continue
    Checkpoint,
    /// Run background daemon for polling and tick loops
    Daemon(commands::daemon::DaemonArgs),
    /// Generate shell completions
    #[command(after_help = "INSTALL:
    # Bash
    oj completions bash > ~/.local/share/bash-completion/completions/oj

    # Zsh (add ~/.zfunc to fpath)
    oj completions zsh > ~/.zfunc/_oj

    # Fish
    oj completions fish > ~/.config/fish/completions/oj.fish")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run(command) => commands::run::handle(command).await,
        Commands::Pipeline { command } => commands::pipeline::handle(command).await,
        Commands::Workspace { command } => commands::workspace::handle(command).await,
        Commands::Session { command } => commands::session::handle(command).await,
        Commands::Queue { command } => commands::queue::handle(command).await,
        Commands::Done { error } => commands::signal::handle_done(error).await,
        Commands::Checkpoint => commands::signal::handle_checkpoint().await,
        Commands::Daemon(args) => commands::daemon::handle(args).await,
        Commands::Completions { shell } => {
            completions::generate_completions::<Cli>(shell);
            Ok(())
        }
    }
}

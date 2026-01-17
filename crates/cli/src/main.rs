//! oj - Otter Jobs CLI
//!
//! A CLI tool for managing automated workflows with Claude Code.

use clap::{Parser, Subcommand};

mod commands;
mod output;

#[derive(Parser)]
#[command(name = "oj")]
#[command(about = "Otter Jobs - Automated workflow management")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format (text or json)
    #[arg(long, global = true, default_value = "text")]
    format: output::OutputFormat,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a pipeline (build or bugfix)
    Run {
        #[command(subcommand)]
        command: commands::run::RunCommand,
    },
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
        Commands::Run { command } => commands::run::handle(command).await,
        Commands::Pipeline { command } => commands::pipeline::handle(command).await,
        Commands::Workspace { command } => commands::workspace::handle(command).await,
        Commands::Session { command } => commands::session::handle(command).await,
        Commands::Queue { command } => commands::queue::handle(command).await,
        Commands::Done { error } => commands::signal::handle_done(error).await,
        Commands::Checkpoint => commands::signal::handle_checkpoint().await,
    }
}

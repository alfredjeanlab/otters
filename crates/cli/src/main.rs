// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! oj - Otter Jobs CLI

mod client;
mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{daemon, done, emit, pipeline, run, session, worker};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::client::{find_project_root, DaemonClient};
use oj_core::Event;

#[derive(Parser)]
#[command(
    name = "oj",
    version,
    about = "Otter Jobs - Agentic development automation"
)]
struct Cli {
    /// Repository root directory
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command from the runbook
    Run(run::RunArgs),
    /// Worker management
    Worker(worker::WorkerArgs),
    /// Pipeline management
    Pipeline(pipeline::PipelineArgs),
    /// Session management
    Session(session::SessionArgs),
    /// Emit an event
    Emit(emit::EmitArgs),
    /// Signal agent completion
    Done(done::DoneArgs),
    /// Daemon management
    Daemon(daemon::DaemonArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle daemon command separately (doesn't need client connection)
    if let Commands::Daemon(args) = cli.command {
        return daemon::daemon(args, cli.repo).await;
    }

    // All other commands go through the daemon
    let project_root = cli.repo.map_or_else(find_project_root, Ok)?;
    let client = DaemonClient::connect_or_start(project_root.clone())?;

    // Load runbook for argument parsing
    let runbook = load_runbook(&project_root)?;

    match cli.command {
        Commands::Run(args) => {
            let cmd_def = runbook
                .get_command(&args.command)
                .ok_or_else(|| anyhow::anyhow!("unknown command: {}", args.command))?;

            let named: HashMap<String, String> = args.named_args.into_iter().collect();

            // Validate required arguments before starting the command
            cmd_def.validate_args(&args.args, &named)?;

            let parsed_args = cmd_def.parse_args(&args.args, &named);

            client
                .send_event(Event::CommandInvoked {
                    command: args.command.clone(),
                    args: parsed_args,
                })
                .await?;

            println!("Started: {}", args.command);
        }

        Commands::Done(args) => {
            let pipeline_id =
                std::env::var("OJ_PIPELINE").map_err(|_| anyhow::anyhow!("OJ_PIPELINE not set"))?;

            let event = match args.error {
                Some(error) => Event::AgentError { pipeline_id, error },
                None => Event::AgentDone {
                    pipeline_id: pipeline_id.clone(),
                },
            };

            client.send_event(event).await?;
            println!("Signaled completion");
        }

        Commands::Emit(args) => {
            let data: serde_json::Value = serde_json::from_str(&args.data)?;

            client
                .send_event(Event::Custom {
                    name: args.event,
                    data,
                })
                .await?;

            println!("Event emitted");
        }

        Commands::Pipeline(args) => {
            use commands::pipeline::PipelineCommand;

            match args.command {
                PipelineCommand::List => {
                    let pipelines = client.list_pipelines().await?;

                    if pipelines.is_empty() {
                        println!("No pipelines");
                    } else {
                        println!(
                            "{:<12} {:<20} {:<10} {:<15} STATUS",
                            "ID", "NAME", "KIND", "PHASE"
                        );
                        for p in pipelines {
                            println!(
                                "{:<12} {:<20} {:<10} {:<15} {}",
                                &p.id[..12.min(p.id.len())],
                                &p.name[..20.min(p.name.len())],
                                &p.kind[..10.min(p.kind.len())],
                                p.phase,
                                p.phase_status
                            );
                        }
                    }
                }
                PipelineCommand::Show { id } => {
                    if let Some(p) = client.get_pipeline(&id).await? {
                        println!("Pipeline: {}", p.id);
                        println!("  Name: {}", p.name);
                        println!("  Kind: {}", p.kind);
                        println!("  Phase: {} ({})", p.phase, p.phase_status);
                        if let Some(ws) = &p.workspace_path {
                            println!("  Workspace: {}", ws.display());
                        }
                        if let Some(session) = &p.session_id {
                            println!("  Session: {}", session);
                        }
                        if let Some(error) = &p.error {
                            println!("  Error: {}", error);
                        }
                        if !p.inputs.is_empty() {
                            println!("  Inputs:");
                            for (k, v) in &p.inputs {
                                println!("    {}: {}", k, v);
                            }
                        }
                    } else {
                        println!("Pipeline not found: {}", id);
                    }
                }
                PipelineCommand::Resume { id } => {
                    client.pipeline_resume(&id).await?;
                    println!("Resumed monitoring for pipeline {}", id);
                }
                PipelineCommand::Fail { id, error } => {
                    let error = error.unwrap_or_else(|| "manual failure".to_string());
                    client.pipeline_fail(&id, &error).await?;
                    println!("Marked pipeline {} as failed", id);
                }
            }
        }

        Commands::Session(args) => {
            use commands::session::SessionCommand;

            match args.command {
                SessionCommand::List => {
                    let sessions = client.list_sessions().await?;
                    if sessions.is_empty() {
                        println!("No sessions");
                    } else {
                        println!("{:<20} PIPELINE", "SESSION");
                        for s in sessions {
                            println!(
                                "{:<20} {}",
                                s.id,
                                s.pipeline_id.unwrap_or_else(|| "-".to_string())
                            );
                        }
                    }
                }
                SessionCommand::Send { id, input } => {
                    client.session_send(&id, &input).await?;
                    println!("Sent to session {}", id);
                }
                SessionCommand::Attach { id } => {
                    session::attach(&id)?;
                }
            }
        }

        Commands::Worker(_) => {
            anyhow::bail!("Worker commands not yet supported")
        }

        Commands::Daemon(_) => unreachable!(),
    }

    Ok(())
}

fn load_runbook(project_root: &std::path::Path) -> Result<oj_runbook::Runbook> {
    let runbook_path = project_root.join(".oj/runbooks");
    if runbook_path.exists() {
        let mut content = String::new();
        for entry in std::fs::read_dir(&runbook_path)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                content.push_str(&std::fs::read_to_string(&path)?);
                content.push('\n');
            }
        }
        Ok(oj_runbook::parse_runbook(&content)?)
    } else {
        Ok(oj_runbook::Runbook::default())
    }
}

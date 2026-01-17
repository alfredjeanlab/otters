//! Pipeline commands

use anyhow::bail;
use clap::Subcommand;
use oj_core::clock::SystemClock;
use oj_core::pipeline::PipelineEvent;
use oj_core::storage::JsonStore;
use serde::Serialize;
use std::fmt;

#[derive(Subcommand)]
pub enum PipelineCommand {
    /// List all pipelines
    List,
    /// Show details of a pipeline
    Show {
        /// Pipeline name
        name: String,
    },
    /// Manually transition a pipeline
    Transition {
        /// Pipeline name
        name: String,
        /// Event to trigger (complete, failed)
        event: String,
        /// Error message for failed events
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Serialize)]
struct PipelineInfo {
    id: String,
    name: String,
    kind: String,
    phase: String,
}

impl fmt::Display for PipelineInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:<20} {:<10} {:<10}", self.name, self.kind, self.phase)
    }
}

pub async fn handle(command: PipelineCommand) -> anyhow::Result<()> {
    match command {
        PipelineCommand::List => list_pipelines().await,
        PipelineCommand::Show { name } => show_pipeline(name).await,
        PipelineCommand::Transition {
            name,
            event,
            reason,
        } => transition_pipeline(name, event, reason).await,
    }
}

async fn list_pipelines() -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let ids = store.list_pipelines()?;

    if ids.is_empty() {
        println!("No pipelines found.");
        return Ok(());
    }

    println!("{:<20} {:<10} {:<10}", "NAME", "KIND", "PHASE");
    println!("{}", "-".repeat(42));

    for id in ids {
        if let Ok(pipeline) = store.load_pipeline(&id) {
            let info = PipelineInfo {
                id: pipeline.id.0,
                name: pipeline.name,
                kind: format!("{:?}", pipeline.kind).to_lowercase(),
                phase: pipeline.phase.name().to_string(),
            };
            println!("{}", info);
        }
    }

    Ok(())
}

async fn show_pipeline(name: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let pipeline = store.load_pipeline(&name)?;

    println!("Pipeline: {}", pipeline.name);
    println!("ID: {}", pipeline.id.0);
    println!("Kind: {:?}", pipeline.kind);
    println!("Phase: {}", pipeline.phase.name());
    println!("Created: {}", pipeline.created_at);

    if !pipeline.inputs.is_empty() {
        println!();
        println!("Inputs:");
        for (key, value) in &pipeline.inputs {
            println!("  {}: {}", key, value);
        }
    }

    if let Some(ref workspace_id) = pipeline.workspace_id {
        println!();
        println!("Workspace: {}", workspace_id.0);
    }

    Ok(())
}

async fn transition_pipeline(
    name: String,
    event: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;
    let pipeline = store.load_pipeline(&name)?;

    let event = match event.as_str() {
        "complete" => PipelineEvent::PhaseComplete,
        "failed" => PipelineEvent::PhaseFailed {
            reason: reason.unwrap_or_else(|| "Manual failure".to_string()),
        },
        "unblocked" => PipelineEvent::Unblocked,
        other => bail!("Unknown event: {}. Use 'complete', 'failed', or 'unblocked'", other),
    };

    let clock = SystemClock;
    let (pipeline, _effects) = pipeline.transition(event, &clock);
    store.save_pipeline(&pipeline)?;

    println!("Pipeline '{}' transitioned to: {}", name, pipeline.phase.name());

    Ok(())
}

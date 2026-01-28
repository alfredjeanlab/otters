// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj pipeline` - Pipeline management commands

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct PipelineArgs {
    #[command(subcommand)]
    pub command: PipelineCommand,
}

#[derive(Subcommand)]
pub enum PipelineCommand {
    /// List all pipelines
    List,
    /// Show details of a pipeline
    Show {
        /// Pipeline ID or name
        id: String,
    },
    /// Resume monitoring for an escalated pipeline
    Resume {
        /// Pipeline ID or name
        id: String,
    },
    /// Mark a pipeline as failed
    Fail {
        /// Pipeline ID or name
        id: String,
        /// Error message
        #[arg(short, long)]
        error: Option<String>,
    },
}

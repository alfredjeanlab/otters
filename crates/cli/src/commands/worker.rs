// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj worker` - Worker management commands

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct WorkerArgs {
    #[command(subcommand)]
    pub command: WorkerCommand,
}

#[derive(Subcommand)]
pub enum WorkerCommand {
    /// Start a worker
    Start {
        /// Worker name
        name: String,
    },
    /// Stop a worker
    Stop {
        /// Worker name
        name: String,
    },
    /// Wake a worker to check for work
    Wake {
        /// Worker name
        name: String,
    },
}

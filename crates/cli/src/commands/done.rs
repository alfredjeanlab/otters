// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj done` - Signal agent completion

use clap::Args;

#[derive(Args)]
pub struct DoneArgs {
    /// Report an error instead of success
    #[arg(long)]
    pub error: Option<String>,
}

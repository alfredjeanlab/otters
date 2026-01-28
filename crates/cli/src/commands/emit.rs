// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj emit <event>` - Emit events to the system

use clap::Args;

#[derive(Args)]
pub struct EmitArgs {
    /// Event name
    pub event: String,

    /// Event data as JSON
    #[arg(short, long, default_value = "{}")]
    pub data: String,
}

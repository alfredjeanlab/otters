// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj run <command> [args]` - Run a command from the runbook

use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct RunArgs {
    /// Command to run (e.g., "build")
    pub command: String,

    /// Positional arguments for the command
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Named arguments (key=value)
    #[arg(short = 'a', long = "arg", value_parser = parse_key_val)]
    pub named_args: Vec<(String, String)>,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid key=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

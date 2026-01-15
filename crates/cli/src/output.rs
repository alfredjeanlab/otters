// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Output formatting for CLI commands

use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Print output in the specified format
pub fn print<T: Serialize + std::fmt::Display>(value: &T, format: OutputFormat) {
    match format {
        OutputFormat::Text => println!("{}", value),
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(value) {
                println!("{}", json);
            }
        }
    }
}

/// Print a list of items
pub fn print_list<T: Serialize + std::fmt::Display>(items: &[T], format: OutputFormat) {
    match format {
        OutputFormat::Text => {
            for item in items {
                println!("{}", item);
            }
        }
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(items) {
                println!("{}", json);
            }
        }
    }
}

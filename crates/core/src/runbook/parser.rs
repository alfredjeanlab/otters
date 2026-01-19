// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! TOML parsing for runbooks (syntactic layer).
//!
//! This module provides functions to parse raw runbook TOML into
//! `RawRunbook` structs. No validation is performed at this layer -
//! that's the job of the validator.

use super::types::RawRunbook;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    /// TOML syntax error
    #[error("TOML syntax error: {0}")]
    Toml(#[from] toml::de::Error),

    /// IO error reading file
    #[error("IO error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Parse a runbook from TOML string content.
///
/// # Example
///
/// ```ignore
/// let toml = r#"
/// [command.hello]
/// run = "echo hello"
/// "#;
///
/// let runbook = parse_runbook(toml)?;
/// assert!(runbook.command.contains_key("hello"));
/// ```
pub fn parse_runbook(toml_content: &str) -> Result<RawRunbook, ParseError> {
    let runbook: RawRunbook = toml::from_str(toml_content)?;
    Ok(runbook)
}

/// Parse a runbook from a TOML file.
///
/// # Example
///
/// ```ignore
/// let runbook = parse_runbook_file(Path::new("runbooks/build.toml"))?;
/// ```
pub fn parse_runbook_file(path: &Path) -> Result<RawRunbook, ParseError> {
    let content = std::fs::read_to_string(path).map_err(|e| ParseError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_runbook(&content)
}

/// Get the name of a runbook file (without extension).
///
/// # Example
///
/// ```ignore
/// let name = runbook_name(Path::new("runbooks/build.toml"));
/// assert_eq!(name, Some("build"));
/// ```
pub fn runbook_name(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|s| s.to_str())
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;

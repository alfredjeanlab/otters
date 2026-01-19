// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Input source parsing for runbook templates.
//!
//! This module provides parsers for various input formats that can be used
//! to populate template contexts from command output or file contents:
//!
//! - **JSON**: Parse structured JSON data
//! - **Lines**: Parse output as a list of lines
//! - **CSV**: Parse comma-separated values
//! - **Key-Value**: Parse `key=value` or `key: value` pairs
//!
//! # Example
//!
//! ```ignore
//! use oj_core::runbook::input::{parse_input, InputFormat};
//!
//! // Parse JSON
//! let json = r#"{"name": "auth", "count": 42}"#;
//! let value = parse_input(json, InputFormat::Json)?;
//!
//! // Parse lines
//! let lines = "line1\nline2\nline3";
//! let value = parse_input(lines, InputFormat::Lines)?;
//! ```

use super::template::ContextValue;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during input parsing.
#[derive(Debug, Error)]
pub enum InputError {
    /// JSON parsing error
    #[error("JSON parse error: {0}")]
    Json(String),

    /// CSV parsing error
    #[error("CSV parse error: {0}")]
    Csv(String),

    /// Key-value parsing error
    #[error("Key-value parse error: {0}")]
    KeyValue(String),

    /// Invalid format specification
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

/// Input format specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputFormat {
    /// Automatic detection based on content
    #[default]
    Auto,
    /// JSON object or array
    Json,
    /// Lines of text (one item per line)
    Lines,
    /// Comma-separated values
    Csv,
    /// Key-value pairs (key=value or key: value)
    KeyValue,
    /// Raw text (no parsing, returns as single string)
    Raw,
}

impl std::str::FromStr for InputFormat {
    type Err = InputError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" | "" => Ok(InputFormat::Auto),
            "json" => Ok(InputFormat::Json),
            "lines" | "line" => Ok(InputFormat::Lines),
            "csv" => Ok(InputFormat::Csv),
            "kv" | "keyvalue" | "key-value" | "key_value" => Ok(InputFormat::KeyValue),
            "raw" | "text" => Ok(InputFormat::Raw),
            other => Err(InputError::InvalidFormat(other.to_string())),
        }
    }
}

/// Parse input text according to the specified format.
pub fn parse_input(input: &str, format: InputFormat) -> Result<ContextValue, InputError> {
    match format {
        InputFormat::Auto => parse_auto(input),
        InputFormat::Json => parse_json(input),
        InputFormat::Lines => Ok(parse_lines(input)),
        InputFormat::Csv => parse_csv(input),
        InputFormat::KeyValue => parse_key_value(input),
        InputFormat::Raw => Ok(ContextValue::String(input.to_string())),
    }
}

/// Automatically detect and parse the input format.
fn parse_auto(input: &str) -> Result<ContextValue, InputError> {
    let trimmed = input.trim();

    // Try JSON if it looks like JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if let Ok(value) = parse_json(input) {
            return Ok(value);
        }
    }

    // Try key-value if it has key=value or key: value patterns
    if looks_like_key_value(trimmed) {
        if let Ok(value) = parse_key_value(input) {
            return Ok(value);
        }
    }

    // Default to lines if multiline, otherwise raw string
    if input.contains('\n') {
        Ok(parse_lines(input))
    } else {
        Ok(ContextValue::String(input.to_string()))
    }
}

/// Check if input looks like key-value pairs.
fn looks_like_key_value(s: &str) -> bool {
    let lines: Vec<&str> = s.lines().collect();
    if lines.is_empty() {
        return false;
    }

    // Check if most lines look like key-value pairs
    let kv_count = lines
        .iter()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && (line.contains('=') || line.contains(": "))
        })
        .count();

    // At least half the non-empty lines should look like key-value
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count();
    non_empty > 0 && kv_count * 2 >= non_empty
}

/// Parse JSON input.
fn parse_json(input: &str) -> Result<ContextValue, InputError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| InputError::Json(e.to_string()))?;

    Ok(json_to_context_value(&value))
}

/// Convert a serde_json::Value to a ContextValue.
fn json_to_context_value(value: &serde_json::Value) -> ContextValue {
    match value {
        serde_json::Value::Null => ContextValue::Null,
        serde_json::Value::Bool(b) => ContextValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ContextValue::Number(i)
            } else if let Some(f) = n.as_f64() {
                ContextValue::Float(f)
            } else {
                ContextValue::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => ContextValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            ContextValue::List(arr.iter().map(json_to_context_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, ContextValue> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_context_value(v)))
                .collect();
            ContextValue::Object(map)
        }
    }
}

/// Parse lines input.
fn parse_lines(input: &str) -> ContextValue {
    let lines: Vec<ContextValue> = input
        .lines()
        .map(|line| ContextValue::String(line.to_string()))
        .collect();
    ContextValue::List(lines)
}

/// Parse CSV input.
fn parse_csv(input: &str) -> Result<ContextValue, InputError> {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return Ok(ContextValue::List(vec![]));
    }

    // First line is headers
    let headers: Vec<&str> = lines[0].split(',').map(|s| s.trim()).collect();

    // Remaining lines are data rows
    let rows: Vec<ContextValue> = lines[1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let values: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            let mut row: HashMap<String, ContextValue> = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                let value = values
                    .get(i)
                    .map(|v| ContextValue::String((*v).to_string()))
                    .unwrap_or(ContextValue::Null);
                row.insert((*header).to_string(), value);
            }
            ContextValue::Object(row)
        })
        .collect();

    Ok(ContextValue::List(rows))
}

/// Parse key-value input.
fn parse_key_value(input: &str) -> Result<ContextValue, InputError> {
    let mut map: HashMap<String, ContextValue> = HashMap::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Try key=value first
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Remove surrounding quotes if present
            let value = strip_quotes(value);
            map.insert(key.to_string(), parse_kv_value(value));
        } else if let Some((key, value)) = line.split_once(':') {
            // Only use colon if there's a space after it (avoid paths like /foo/bar:baz)
            let value = value.trim();
            if !value.is_empty() && !value.starts_with('/') {
                let key = key.trim();
                let value = strip_quotes(value);
                map.insert(key.to_string(), parse_kv_value(value));
            }
        }
    }

    Ok(ContextValue::Object(map))
}

/// Strip surrounding quotes from a string.
fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Parse a key-value value, trying to infer the type.
fn parse_kv_value(s: &str) -> ContextValue {
    // Try boolean
    match s.to_lowercase().as_str() {
        "true" | "yes" | "on" => return ContextValue::Bool(true),
        "false" | "no" | "off" => return ContextValue::Bool(false),
        "null" | "none" => return ContextValue::Null,
        _ => {}
    }

    // Try integer
    if let Ok(n) = s.parse::<i64>() {
        return ContextValue::Number(n);
    }

    // Try float
    if let Ok(f) = s.parse::<f64>() {
        return ContextValue::Float(f);
    }

    // Default to string
    ContextValue::String(s.to_string())
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;

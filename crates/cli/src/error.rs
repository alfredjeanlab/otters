// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! User-friendly error display with context and suggestions.
//!
//! This module provides enhanced error types that include:
//! - What went wrong (message)
//! - Why it might have happened (context)
//! - How to fix it (suggestions)

use std::fmt;

/// Error with context and recovery suggestions for user-friendly display.
#[derive(Debug)]
pub struct OjError {
    /// What went wrong
    pub message: String,
    /// Why it might have happened
    pub context: Vec<String>,
    /// How to fix it
    pub suggestions: Vec<String>,
    /// Original error if any
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

// Error construction methods - allow dead_code as not all may be used immediately
#[allow(dead_code)]
impl OjError {
    /// Create a new error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            context: Vec::new(),
            suggestions: Vec::new(),
            source: None,
        }
    }

    /// Add context about why this error might have happened.
    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }

    /// Add a suggestion for how to fix this error.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    /// Set the source error that caused this error.
    pub fn with_source<E: std::error::Error + Send + Sync + 'static>(mut self, source: E) -> Self {
        self.source = Some(Box::new(source));
        self
    }
}

impl fmt::Display for OjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "error: {}", self.message)?;

        if !self.context.is_empty() {
            writeln!(f)?;
            for ctx in &self.context {
                writeln!(f, "  -> {}", ctx)?;
            }
        }

        if !self.suggestions.is_empty() {
            writeln!(f)?;
            writeln!(f, "suggestions:")?;
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, suggestion)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for OjError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Common error builders for typical failure scenarios.
// These helpers are provided for use as error handling is integrated.
// Allow dead_code since not all may be used immediately.
#[allow(dead_code)]
impl OjError {
    /// Error for when a lock cannot be acquired.
    pub fn lock_held(lock_id: &str, holder: &str, duration_secs: u64) -> Self {
        OjError::new(format!("Failed to acquire lock '{}'", lock_id))
            .with_context(format!("Lock is currently held by '{}'", holder))
            .with_context(format!("Lock has been held for {} seconds", duration_secs))
            .with_suggestion("Wait for the current holder to release")
            .with_suggestion(format!(
                "Force release with: oj lock release {} --force",
                lock_id
            ))
            .with_suggestion("Check if the holder process is stuck: oj session status")
    }

    /// Error for when a pipeline cannot be found.
    pub fn pipeline_not_found(pipeline_id: &str) -> Self {
        OjError::new(format!("Pipeline '{}' not found", pipeline_id))
            .with_context("The pipeline may have completed or been deleted")
            .with_suggestion("List active pipelines: oj pipeline list")
            .with_suggestion("Check pipeline history: oj pipeline list --all")
    }

    /// Error for when a session is not responding.
    pub fn session_unresponsive(session_id: &str) -> Self {
        OjError::new(format!("Session '{}' is not responding", session_id))
            .with_context("The Claude CLI may be hung or waiting for input")
            .with_context("Network issues may be preventing communication")
            .with_suggestion(format!(
                "Check session status: oj session show {}",
                session_id
            ))
            .with_suggestion(format!(
                "View last output: oj session capture {} --tail 50",
                session_id
            ))
            .with_suggestion(format!(
                "Send interrupt: oj session send {} --interrupt",
                session_id
            ))
            .with_suggestion(format!(
                "Kill if unresponsive: oj session kill {}",
                session_id
            ))
    }

    /// Error for when WAL corruption is detected.
    pub fn wal_corruption(position: u64) -> Self {
        OjError::new("WAL checksum mismatch detected")
            .with_context(format!("Corruption detected at position {}", position))
            .with_context("This may be caused by a crash during write or disk corruption")
            .with_suggestion("Try recovery mode: oj daemon start --recover")
            .with_suggestion("Restore from snapshot: oj maintenance restore-snapshot --latest")
    }

    /// Error for when resources are exhausted.
    pub fn resource_limit(resource: &str, current: usize, limit: usize) -> Self {
        OjError::new(format!("{} limit exceeded", resource))
            .with_context(format!("Current: {}, Limit: {}", current, limit))
            .with_suggestion(format!(
                "Wait for {} to be released",
                resource.to_lowercase()
            ))
            .with_suggestion("Check resource usage: oj status --resources")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = OjError::new("Something went wrong")
            .with_context("First context")
            .with_context("Second context")
            .with_suggestion("Try this")
            .with_suggestion("Or this");

        let output = format!("{}", err);
        assert!(output.contains("error: Something went wrong"));
        assert!(output.contains("-> First context"));
        assert!(output.contains("-> Second context"));
        assert!(output.contains("1. Try this"));
        assert!(output.contains("2. Or this"));
    }

    #[test]
    fn test_lock_held_error() {
        let err = OjError::lock_held("merge-lock", "pipeline-123", 300);
        let output = format!("{}", err);
        assert!(output.contains("merge-lock"));
        assert!(output.contains("pipeline-123"));
        assert!(output.contains("300 seconds"));
    }
}

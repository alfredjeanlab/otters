// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Error types for the engine runtime

use crate::ExecuteError;
use thiserror::Error;

/// Errors that can occur in the runtime
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("execute error: {0}")]
    Execute(#[from] ExecuteError),
    #[error("pipeline not found: {0}")]
    PipelineNotFound(String),
    #[error("command not found: {0}")]
    CommandNotFound(String),
    #[error("pipeline definition not found: {0}")]
    PipelineDefNotFound(String),
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("prompt error for agent {agent}: {message}")]
    PromptError { agent: String, message: String },
    #[error("invalid run directive for {context}: {directive}")]
    InvalidRunDirective { context: String, directive: String },
}

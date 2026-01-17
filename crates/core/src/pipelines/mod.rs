//! Hardcoded pipeline implementations

pub mod build;
pub mod bugfix;

use crate::pipeline::Phase;
use std::time::Duration;

/// Configuration for a pipeline phase
#[derive(Debug, Clone)]
pub struct PhaseConfig {
    /// Shell commands to run before the task
    pub run: Option<Vec<String>>,
    /// Task configuration for Claude session
    pub task: Option<TaskConfig>,
    /// Next phase after completion
    pub next: Phase,
}

/// Configuration for a Claude task
#[derive(Debug, Clone)]
pub struct TaskConfig {
    /// Command to run (e.g., "claude --print")
    pub command: String,
    /// Optional prompt file
    pub prompt_file: Option<String>,
    /// Maximum time for the task
    pub timeout: Duration,
    /// Time without output before considering idle
    pub idle_timeout: Duration,
}

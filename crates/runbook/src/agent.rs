// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Agent definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

/// An agent definition from the runbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    /// Agent name (set from table key, not from TOML content)
    #[serde(default)]
    pub name: String,
    /// Command to run (e.g., "claude --print")
    pub run: String,
    /// Prompt template for the agent
    #[serde(default)]
    pub prompt: Option<String>,
    /// Path to file containing prompt template
    #[serde(default)]
    pub prompt_file: Option<PathBuf>,
    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Working directory (relative to workspace)
    #[serde(default)]
    pub cwd: Option<String>,

    /// What to do when Claude is waiting for input (stop_reason: end_turn)
    #[serde(default)]
    pub on_idle: ActionConfig,

    /// What to do when Claude process exits (without calling oj done)
    #[serde(default = "default_on_exit")]
    pub on_exit: ActionConfig,

    /// What to do on API errors (unauthorized, credits, network)
    #[serde(default = "default_on_error")]
    pub on_error: ErrorActionConfig,
}

/// Action configuration - simple or with options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ActionConfig {
    Simple(AgentAction),
    WithOptions {
        action: AgentAction,
        #[serde(default)]
        message: Option<String>,
        /// For recover: false = replace prompt (default), true = append to prompt
        #[serde(default)]
        append: bool,
    },
}

impl Default for ActionConfig {
    fn default() -> Self {
        ActionConfig::Simple(AgentAction::Nudge)
    }
}

impl ActionConfig {
    /// Create a simple action config with no message
    pub fn simple(action: AgentAction) -> Self {
        ActionConfig::Simple(action)
    }

    /// Create an action config with a replacement message
    pub fn with_message(action: AgentAction, message: &str) -> Self {
        ActionConfig::WithOptions {
            action,
            message: Some(message.to_string()),
            append: false,
        }
    }

    /// Create an action config with an append message
    pub fn with_append(action: AgentAction, message: &str) -> Self {
        ActionConfig::WithOptions {
            action,
            message: Some(message.to_string()),
            append: true,
        }
    }

    pub fn action(&self) -> &AgentAction {
        match self {
            ActionConfig::Simple(a) => a,
            ActionConfig::WithOptions { action, .. } => action,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            ActionConfig::Simple(_) => None,
            ActionConfig::WithOptions { message, .. } => message.as_deref(),
        }
    }

    pub fn append(&self) -> bool {
        match self {
            ActionConfig::Simple(_) => false,
            ActionConfig::WithOptions { append, .. } => *append,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentAction {
    #[default]
    Nudge, // Send message prompting to continue
    Done,     // Treat as success, advance pipeline
    Fail,     // Mark pipeline as failed
    Restart,  // Fresh workspace, clean re-spawn
    Recover,  // Re-spawn with modified prompt
    Escalate, // Notify human
}

/// Error action configuration - simple or per-error-type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorActionConfig {
    /// Same action for all errors: on_error = "escalate"
    Simple(ActionConfig),
    /// Per-error with fallthrough: [[on_error]]
    ByType(Vec<ErrorMatch>),
}

impl Default for ErrorActionConfig {
    fn default() -> Self {
        ErrorActionConfig::Simple(ActionConfig::Simple(AgentAction::Escalate))
    }
}

impl ErrorActionConfig {
    /// Find the action config for a given error type
    pub fn action_for(&self, error_type: Option<&ErrorType>) -> ActionConfig {
        match self {
            ErrorActionConfig::Simple(config) => config.clone(),
            ErrorActionConfig::ByType(matches) => matches
                .iter()
                .find(|m| m.error_match.is_none() || m.error_match.as_ref() == error_type)
                .map(|m| ActionConfig::WithOptions {
                    action: m.action.clone(),
                    message: m.message.clone(),
                    append: m.append,
                })
                .unwrap_or_else(|| ActionConfig::Simple(AgentAction::Escalate)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMatch {
    /// Error type to match (None = catch-all)
    #[serde(rename = "match")]
    pub error_match: Option<ErrorType>,
    pub action: AgentAction,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub append: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    Unauthorized,
    OutOfCredits,
    NoInternet,
    RateLimited,
}

fn default_on_exit() -> ActionConfig {
    ActionConfig::Simple(AgentAction::Escalate)
}

fn default_on_error() -> ErrorActionConfig {
    ErrorActionConfig::default()
}

impl Default for AgentDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            run: String::new(),
            prompt: None,
            prompt_file: None,
            env: HashMap::new(),
            cwd: None,
            on_idle: ActionConfig::default(),
            on_exit: default_on_exit(),
            on_error: default_on_error(),
        }
    }
}

impl AgentDef {
    /// Build the command with interpolated variables
    pub fn build_command(&self, vars: &HashMap<String, String>) -> String {
        crate::template::interpolate(&self.run, vars)
    }

    /// Build the environment variables with interpolated values
    pub fn build_env(&self, vars: &HashMap<String, String>) -> Vec<(String, String)> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), crate::template::interpolate(v, vars)))
            .collect()
    }

    /// Get the prompt text with variables interpolated
    ///
    /// Reads from prompt_file if specified, otherwise uses prompt field.
    /// Returns empty string if neither is set.
    pub fn get_prompt(&self, vars: &HashMap<String, String>) -> io::Result<String> {
        let template = if let Some(ref file) = self.prompt_file {
            std::fs::read_to_string(file)?
        } else if let Some(ref prompt) = self.prompt {
            prompt.clone()
        } else {
            return Ok(String::new());
        };
        Ok(crate::template::interpolate(&template, vars))
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Pipeline definitions

use crate::command::RunDirective;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A phase within a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDef {
    /// Phase name
    pub name: String,
    /// What to run: shell command, agent, or strategy
    pub run: RunDirective,
    /// Next phase on success
    #[serde(default)]
    pub next: Option<String>,
    /// Phase to go to on failure
    #[serde(default)]
    pub on_fail: Option<String>,
}

impl PhaseDef {
    /// Check if this phase runs a shell command
    pub fn is_shell(&self) -> bool {
        self.run.is_shell()
    }

    /// Check if this phase invokes an agent
    pub fn is_agent(&self) -> bool {
        self.run.is_agent()
    }

    /// Check if this phase invokes a strategy
    pub fn is_strategy(&self) -> bool {
        self.run.is_strategy()
    }

    /// Get the agent name if this phase invokes an agent
    pub fn agent_name(&self) -> Option<&str> {
        self.run.agent_name()
    }

    /// Get the shell command if this phase runs a shell command
    pub fn shell_command(&self) -> Option<&str> {
        self.run.shell_command()
    }
}

/// A pipeline definition from the runbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDef {
    /// Pipeline name
    pub name: String,
    /// Required input variables
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Default values for inputs
    #[serde(default)]
    pub defaults: HashMap<String, String>,
    /// Ordered phases
    #[serde(default)]
    pub phases: Vec<PhaseDef>,
}

impl PipelineDef {
    /// Get a phase by name
    pub fn get_phase(&self, name: &str) -> Option<&PhaseDef> {
        self.phases.iter().find(|p| p.name == name)
    }

    /// Get the first phase
    pub fn first_phase(&self) -> Option<&PhaseDef> {
        self.phases.first()
    }

    /// Get the next phase after the given phase
    pub fn next_phase(&self, current: &str) -> Option<&PhaseDef> {
        let phase = self.get_phase(current)?;
        if let Some(next) = &phase.next {
            self.get_phase(next)
        } else {
            // Default: next in order
            let idx = self.phases.iter().position(|p| p.name == current)?;
            self.phases.get(idx + 1)
        }
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

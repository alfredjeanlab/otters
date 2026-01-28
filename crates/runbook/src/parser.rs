// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Runbook TOML parsing

use crate::{
    AgentDef, ArgSpec, ArgSpecError, CommandDef, PhaseDef, PipelineDef, RunDirective, WorkerDef,
};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during runbook parsing
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid format: {0}")]
    InvalidFormat(String),
    #[error("invalid argument spec: {0}")]
    ArgSpec(#[from] ArgSpecError),
}

/// A parsed runbook
#[derive(Debug, Clone, Default)]
pub struct Runbook {
    pub commands: HashMap<String, CommandDef>,
    pub workers: HashMap<String, WorkerDef>,
    pub pipelines: HashMap<String, PipelineDef>,
    pub agents: HashMap<String, AgentDef>,
}

impl Runbook {
    /// Get a command definition by name
    pub fn get_command(&self, name: &str) -> Option<&CommandDef> {
        self.commands.get(name)
    }

    /// Get a pipeline definition by name
    pub fn get_pipeline(&self, name: &str) -> Option<&PipelineDef> {
        self.pipelines.get(name)
    }

    /// Get an agent definition by name
    pub fn get_agent(&self, name: &str) -> Option<&AgentDef> {
        self.agents.get(name)
    }

    /// Get a worker definition by name
    pub fn get_worker(&self, name: &str) -> Option<&WorkerDef> {
        self.workers.get(name)
    }
}

/// Parse a runbook from TOML content
pub fn parse_runbook(content: &str) -> Result<Runbook, ParseError> {
    let raw: toml::Value = toml::from_str(content)?;
    let table = raw
        .as_table()
        .ok_or_else(|| ParseError::InvalidFormat("root must be a table".to_string()))?;

    let mut runbook = Runbook::default();

    // Parse commands
    if let Some(commands) = table.get("command").and_then(|v| v.as_table()) {
        for (name, value) in commands {
            let cmd = parse_command(name, value)?;
            runbook.commands.insert(name.clone(), cmd);
        }
    }

    // Parse workers
    if let Some(workers) = table.get("worker").and_then(|v| v.as_table()) {
        for (name, value) in workers {
            let worker = parse_worker(name, value)?;
            runbook.workers.insert(name.clone(), worker);
        }
    }

    // Parse pipelines
    if let Some(pipelines) = table.get("pipeline").and_then(|v| v.as_table()) {
        for (name, value) in pipelines {
            let pipeline = parse_pipeline(name, value)?;
            runbook.pipelines.insert(name.clone(), pipeline);
        }
    }

    // Parse agents
    if let Some(agents) = table.get("agent").and_then(|v| v.as_table()) {
        for (name, value) in agents {
            let agent = parse_agent(name, value)?;
            runbook.agents.insert(name.clone(), agent);
        }
    }

    Ok(runbook)
}

fn parse_command(name: &str, value: &toml::Value) -> Result<CommandDef, ParseError> {
    let table = value
        .as_table()
        .ok_or_else(|| ParseError::InvalidFormat(format!("command.{} must be a table", name)))?;

    // Parse run directive - can be string or table
    let run_value = table
        .get("run")
        .ok_or_else(|| ParseError::MissingField(format!("command.{}.run", name)))?;
    let run: RunDirective = run_value
        .clone()
        .try_into()
        .map_err(|e| ParseError::InvalidFormat(format!("command.{}.run: {}", name, e)))?;

    // Parse args - can be string or table
    let args = if let Some(args_value) = table.get("args") {
        args_value
            .clone()
            .try_into()
            .map_err(|e| ParseError::InvalidFormat(format!("command.{}.args: {}", name, e)))?
    } else {
        ArgSpec::default()
    };

    let defaults = table
        .get("defaults")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(CommandDef {
        name: name.to_string(),
        args,
        defaults,
        run,
    })
}

fn parse_worker(name: &str, value: &toml::Value) -> Result<WorkerDef, ParseError> {
    let table = value
        .as_table()
        .ok_or_else(|| ParseError::InvalidFormat(format!("worker.{} must be a table", name)))?;

    let concurrency = table
        .get("concurrency")
        .and_then(|v| v.as_integer())
        .map(|v| v as u32)
        .unwrap_or(1);

    let pipelines = table
        .get("pipelines")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(WorkerDef {
        name: name.to_string(),
        concurrency,
        pipelines,
    })
}

fn parse_pipeline(name: &str, value: &toml::Value) -> Result<PipelineDef, ParseError> {
    let table = value
        .as_table()
        .ok_or_else(|| ParseError::InvalidFormat(format!("pipeline.{} must be a table", name)))?;

    let inputs = table
        .get("inputs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let defaults = table
        .get("defaults")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Support both "phase" (from [[pipeline.X.phase]]) and "phases" key names
    let phases_arr = table
        .get("phase")
        .and_then(|v| v.as_array())
        .or_else(|| table.get("phases").and_then(|v| v.as_array()));

    let phases = if let Some(arr) = phases_arr {
        arr.iter().filter_map(|v| parse_phase(v).ok()).collect()
    } else {
        Vec::new()
    };

    Ok(PipelineDef {
        name: name.to_string(),
        inputs,
        defaults,
        phases,
    })
}

fn parse_phase(value: &toml::Value) -> Result<PhaseDef, ParseError> {
    let table = value
        .as_table()
        .ok_or_else(|| ParseError::InvalidFormat("phase must be a table".to_string()))?;

    let name = table
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ParseError::MissingField("phase.name".to_string()))?
        .to_string();

    // Parse run directive - can be string or table
    // Also support legacy "agent" field for backwards compatibility
    let run = if let Some(run_value) = table.get("run") {
        run_value
            .clone()
            .try_into()
            .map_err(|e| ParseError::InvalidFormat(format!("phase.{}.run: {}", name, e)))?
    } else if let Some(agent_name) = table.get("agent").and_then(|v| v.as_str()) {
        // Legacy support: phase.agent = "name" => run = { agent = "name" }
        RunDirective::Agent {
            agent: agent_name.to_string(),
        }
    } else {
        return Err(ParseError::MissingField(format!("phase.{}.run", name)));
    };

    let next = table.get("next").and_then(|v| v.as_str()).map(String::from);
    let on_fail = table
        .get("on_fail")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(PhaseDef {
        name,
        run,
        next,
        on_fail,
    })
}

fn parse_agent(name: &str, value: &toml::Value) -> Result<AgentDef, ParseError> {
    // Deserialize using serde to get proper handling of on_idle/on_exit/on_error
    let mut agent: AgentDef = value.clone().try_into().map_err(|e: toml::de::Error| {
        ParseError::InvalidFormat(format!("agent.{}: {}", name, e))
    })?;
    agent.name = name.to_string();
    Ok(agent)
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;

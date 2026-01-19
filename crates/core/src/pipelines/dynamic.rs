// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Dynamic pipeline creation from runbook definitions.
//!
//! This module provides the bridge between runbook definitions (TOML) and
//! runtime pipeline execution. It converts:
//!
//! - `PipelineDef` → `Pipeline` (state machine)
//! - `PhaseDef` → `PhaseConfig` (phase configuration)
//! - Template rendering with pipeline context
//!
//! # Example
//!
//! ```ignore
//! use oj_core::runbook::{load_runbook_file, RunbookRegistry};
//! use oj_core::pipelines::dynamic::{create_pipeline, DynamicPhaseConfig};
//!
//! // Load runbook
//! let mut registry = RunbookRegistry::new();
//! registry.load_directory(Path::new("runbooks"))?;
//!
//! // Create pipeline from definition
//! let runbook = registry.get("build").unwrap();
//! let def = runbook.pipelines.get("main").unwrap();
//! let pipeline = create_pipeline("build-1", def, inputs, &clock)?;
//! ```

use crate::clock::Clock;
use crate::pipeline::Pipeline;
use crate::runbook::{
    Context, ContextValue, FailAction, PhaseAction, PhaseDef, PhaseNext, PipelineDef, TaskDef,
    TemplateEngine, TemplateError,
};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during dynamic pipeline creation.
#[derive(Debug, Error)]
pub enum DynamicError {
    /// Template rendering failed
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    /// Missing required input
    #[error("Missing required input: {name}")]
    MissingInput { name: String },

    /// Invalid pipeline definition
    #[error("Invalid pipeline definition: {reason}")]
    InvalidDefinition { reason: String },
}

/// Configuration for a dynamically-defined phase.
///
/// This is the runtime representation of a phase definition,
/// with all templates rendered and values resolved.
#[derive(Debug, Clone)]
pub struct DynamicPhaseConfig {
    /// Phase name
    pub name: String,
    /// Shell commands to run (if any)
    pub run: Option<Vec<String>>,
    /// Task configuration (if any)
    pub task: Option<DynamicTaskConfig>,
    /// Strategy to execute (if any)
    pub strategy: Option<String>,
    /// Pre-guard conditions
    pub pre_guards: Vec<String>,
    /// Post-guard conditions
    pub post_guards: Vec<String>,
    /// Lock to acquire
    pub lock: Option<String>,
    /// Semaphore to acquire
    pub semaphore: Option<String>,
    /// Next phase on success
    pub next: DynamicPhaseNext,
    /// Action on failure
    pub on_fail: DynamicFailAction,
}

/// Task configuration for a dynamic phase.
#[derive(Debug, Clone)]
pub struct DynamicTaskConfig {
    /// Command to run (e.g., "claude --print")
    pub command: String,
    /// Prompt template (rendered)
    pub prompt: Option<String>,
    /// Path to prompt file
    pub prompt_file: Option<String>,
    /// Working directory
    pub cwd: Option<String>,
    /// Environment variables
    pub env: BTreeMap<String, String>,
    /// Maximum execution time
    pub timeout: Duration,
    /// Time without output before considering stuck
    pub idle_timeout: Duration,
}

/// Next phase specification for dynamic pipelines.
#[derive(Debug, Clone)]
pub enum DynamicPhaseNext {
    /// Go to named phase
    Phase(String),
    /// Pipeline complete
    Done,
}

impl From<&PhaseNext> for DynamicPhaseNext {
    fn from(next: &PhaseNext) -> Self {
        match next {
            PhaseNext::Phase(p) => DynamicPhaseNext::Phase(p.clone()),
            PhaseNext::Done => DynamicPhaseNext::Done,
        }
    }
}

/// Failure action for dynamic pipelines.
#[derive(Debug, Clone)]
pub enum DynamicFailAction {
    /// Escalate to pipeline failure
    Escalate,
    /// Go to specific phase
    GotoPhase(String),
    /// Use recovery strategy
    UseStrategy(String),
    /// Retry with configuration
    Retry { max: u32, interval: Duration },
}

impl From<&FailAction> for DynamicFailAction {
    fn from(action: &FailAction) -> Self {
        match action {
            FailAction::Escalate => DynamicFailAction::Escalate,
            FailAction::GotoPhase(p) => DynamicFailAction::GotoPhase(p.clone()),
            FailAction::UseStrategy(s) => DynamicFailAction::UseStrategy(s.clone()),
            FailAction::Retry { max, interval } => DynamicFailAction::Retry {
                max: *max,
                interval: *interval,
            },
        }
    }
}

/// Builder for template context from pipeline state.
pub struct ContextBuilder {
    context: Context,
}

impl ContextBuilder {
    /// Create a new context builder.
    pub fn new() -> Self {
        Self {
            context: Context::new(),
        }
    }

    /// Add pipeline inputs to context.
    pub fn with_inputs(mut self, inputs: &BTreeMap<String, String>) -> Self {
        for (k, v) in inputs {
            self.context = self.context.with_string(k, v);
        }
        self
    }

    /// Add defaults, rendering them with current context.
    pub fn with_defaults(
        mut self,
        defaults: &BTreeMap<String, String>,
        engine: &TemplateEngine,
    ) -> Result<Self, TemplateError> {
        for (k, template) in defaults {
            let value = engine.render_simple(template, &self.context)?;
            self.context = self.context.with_string(k, value);
        }
        Ok(self)
    }

    /// Add pipeline runtime values.
    pub fn with_pipeline(mut self, pipeline: &Pipeline) -> Self {
        self.context = self
            .context
            .with_string("pipeline_id", pipeline.id.0.clone())
            .with_string("pipeline_name", pipeline.name.clone())
            .with_string("phase", pipeline.phase.name());
        self
    }

    /// Add a string value.
    pub fn with_string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context = self.context.with_string(key, value);
        self
    }

    /// Add an object value.
    pub fn with_object(
        mut self,
        key: impl Into<String>,
        value: HashMap<String, ContextValue>,
    ) -> Self {
        self.context = self.context.with_object(key, value);
        self
    }

    /// Build the context.
    pub fn build(self) -> Context {
        self.context
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a pipeline from a runbook definition.
///
/// This creates the initial Pipeline state machine from the runbook definition.
/// The pipeline starts in the `Init` phase.
pub fn create_pipeline(
    id: impl Into<String>,
    def: &PipelineDef,
    inputs: BTreeMap<String, String>,
    _clock: &impl Clock,
) -> Result<Pipeline, DynamicError> {
    let id = id.into();

    // Validate required inputs
    for required in &def.inputs {
        if !inputs.contains_key(required) && !def.defaults.contains_key(required) {
            return Err(DynamicError::MissingInput {
                name: required.clone(),
            });
        }
    }

    // Build the full inputs including defaults
    let mut full_inputs = inputs.clone();
    let engine = TemplateEngine::new();

    // Render defaults with current inputs as context
    let context = ContextBuilder::new().with_inputs(&full_inputs).build();

    for (k, template) in &def.defaults {
        if !full_inputs.contains_key(k) {
            let value = engine.render_simple(template, &context)?;
            full_inputs.insert(k.clone(), value);
        }
    }

    // Create the pipeline with all inputs
    let mut pipeline = Pipeline::new_dynamic(&id, &def.name, full_inputs);

    // Store the initial phase name for dynamic resolution
    pipeline
        .outputs
        .insert("_runbook_phase".to_string(), def.initial_phase.clone());
    pipeline
        .outputs
        .insert("_runbook_pipeline".to_string(), def.name.clone());

    Ok(pipeline)
}

/// Convert a phase definition to runtime configuration.
///
/// This renders all templates in the phase definition using the provided context.
pub fn phase_to_config(
    phase_def: &PhaseDef,
    task_def: Option<&TaskDef>,
    engine: &TemplateEngine,
    context: &Context,
) -> Result<DynamicPhaseConfig, DynamicError> {
    // Render the action
    let (run, task, strategy) = match &phase_def.action {
        PhaseAction::Run { command } => {
            let rendered = engine.render_simple(command, context)?;
            (Some(vec![rendered]), None, None)
        }
        PhaseAction::Task { name: _ } => {
            if let Some(task_def) = task_def {
                let task_config = task_to_config(task_def, engine, context)?;
                (None, Some(task_config), None)
            } else {
                // No task definition provided, use name as reference
                (None, None, None)
            }
        }
        PhaseAction::Strategy { name } => (None, None, Some(name.clone())),
        PhaseAction::None => (None, None, None),
    };

    Ok(DynamicPhaseConfig {
        name: phase_def.name.clone(),
        run,
        task,
        strategy,
        pre_guards: phase_def.pre_guards.clone(),
        post_guards: phase_def.post_guards.clone(),
        lock: phase_def.lock.clone(),
        semaphore: phase_def.semaphore.clone(),
        next: DynamicPhaseNext::from(&phase_def.next),
        on_fail: DynamicFailAction::from(&phase_def.on_fail),
    })
}

/// Convert a task definition to runtime configuration.
fn task_to_config(
    task_def: &TaskDef,
    engine: &TemplateEngine,
    context: &Context,
) -> Result<DynamicTaskConfig, DynamicError> {
    // Render prompt if present
    let prompt = if let Some(ref p) = task_def.prompt {
        Some(engine.render(p, context)?)
    } else {
        None
    };

    // Render environment variables
    let mut env = BTreeMap::new();
    for (k, v) in &task_def.env {
        let rendered = engine.render_simple(v, context)?;
        env.insert(k.clone(), rendered);
    }

    Ok(DynamicTaskConfig {
        command: task_def
            .command
            .clone()
            .unwrap_or_else(|| "claude".to_string()),
        prompt,
        prompt_file: task_def.prompt_file.clone(),
        cwd: task_def.cwd.clone(),
        env,
        timeout: task_def
            .timeout
            .unwrap_or_else(|| Duration::from_secs(30 * 60)),
        idle_timeout: task_def
            .idle_timeout
            .unwrap_or_else(|| Duration::from_secs(2 * 60)),
    })
}

/// Check if a pipeline is dynamically defined.
///
/// Returns true if the pipeline was created from a runbook definition.
pub fn is_dynamic_pipeline(pipeline: &Pipeline) -> bool {
    pipeline.outputs.contains_key("_runbook_pipeline")
}

/// Get the current dynamic phase name.
///
/// Returns the runbook phase name for dynamic pipelines, or None for hardcoded pipelines.
pub fn get_dynamic_phase(pipeline: &Pipeline) -> Option<&str> {
    pipeline.outputs.get("_runbook_phase").map(|s| s.as_str())
}

/// Set the current dynamic phase.
pub fn set_dynamic_phase(pipeline: &mut Pipeline, phase: &str) {
    pipeline
        .outputs
        .insert("_runbook_phase".to_string(), phase.to_string());
}

#[cfg(test)]
#[path = "dynamic_tests.rs"]
mod tests;

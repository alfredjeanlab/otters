// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! ActionExecutor - executes actions when triggered
//!
//! The ActionExecutor handles the actual execution of actions, including:
//! - Running shell commands
//! - Starting tasks
//! - Evaluating decision rules

use super::{Action, ActionExecution, DecisionRule};
use std::collections::BTreeMap;
use std::time::Duration;

/// Context for action execution (template interpolation)
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    pub variables: BTreeMap<String, String>,
    pub source: String,
}

impl ExecutionContext {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            variables: BTreeMap::new(),
            source: source.into(),
        }
    }

    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }
}

/// Result of executing an action
#[derive(Debug, Clone)]
pub enum ActionResult {
    /// Command was executed, with output
    CommandOutput {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    /// Task was started
    TaskStarted { task_id: String },
    /// A rule matched and returned an action to take
    RuleMatched {
        action: String,
        delay: Option<Duration>,
    },
    /// No operation was performed
    NoOp,
}

/// Error when executing an action
#[derive(Debug, Clone)]
pub enum ExecutionError {
    /// Command execution failed
    CommandFailed { message: String },
    /// Task creation failed
    TaskFailed { message: String },
    /// No rule matched in decision rules
    NoRuleMatched,
    /// Condition evaluation failed
    ConditionFailed { message: String },
    /// Generic error
    Other { message: String },
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::CommandFailed { message } => write!(f, "command failed: {}", message),
            ExecutionError::TaskFailed { message } => write!(f, "task failed: {}", message),
            ExecutionError::NoRuleMatched => write!(f, "no rule matched"),
            ExecutionError::ConditionFailed { message } => {
                write!(f, "condition evaluation failed: {}", message)
            }
            ExecutionError::Other { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Trait for running shell commands
pub trait CommandRunner: Send + Sync {
    /// Run a command with optional timeout
    fn run(&self, command: &str, timeout: Option<Duration>) -> Result<CommandOutput, String>;
}

/// Output from running a command
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// No-op command runner for testing
pub struct NoOpCommandRunner;

impl CommandRunner for NoOpCommandRunner {
    fn run(&self, _command: &str, _timeout: Option<Duration>) -> Result<CommandOutput, String> {
        Ok(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        })
    }
}

/// Trait for starting tasks
pub trait TaskStarter: Send + Sync {
    /// Start a task with the given inputs
    fn start(&self, task: &str, inputs: &BTreeMap<String, String>) -> Result<String, String>;
}

/// No-op task starter for testing
pub struct NoOpTaskStarter;

impl TaskStarter for NoOpTaskStarter {
    fn start(&self, task: &str, _inputs: &BTreeMap<String, String>) -> Result<String, String> {
        Ok(format!("task:{}", task))
    }
}

/// Trait for evaluating conditions
pub trait ConditionEvaluator: Send + Sync {
    /// Evaluate a condition expression
    fn evaluate(&self, condition: &str, context: &ExecutionContext) -> Result<bool, String>;
}

/// Simple condition evaluator that always returns true
pub struct AlwaysTrueEvaluator;

impl ConditionEvaluator for AlwaysTrueEvaluator {
    fn evaluate(&self, _condition: &str, _context: &ExecutionContext) -> Result<bool, String> {
        Ok(true)
    }
}

/// Executes actions when triggered
pub struct ActionExecutor<'a> {
    command_runner: &'a dyn CommandRunner,
    task_starter: &'a dyn TaskStarter,
    condition_evaluator: &'a dyn ConditionEvaluator,
}

impl<'a> ActionExecutor<'a> {
    /// Create a new ActionExecutor
    pub fn new(
        command_runner: &'a dyn CommandRunner,
        task_starter: &'a dyn TaskStarter,
        condition_evaluator: &'a dyn ConditionEvaluator,
    ) -> Self {
        Self {
            command_runner,
            task_starter,
            condition_evaluator,
        }
    }

    /// Execute an action
    pub fn execute(
        &self,
        action: &Action,
        context: &ExecutionContext,
    ) -> Result<ActionResult, ExecutionError> {
        match &action.execution {
            ActionExecution::Command { run, timeout } => {
                let interpolated = self.interpolate(run, context);
                match self.command_runner.run(&interpolated, *timeout) {
                    Ok(output) => Ok(ActionResult::CommandOutput {
                        stdout: output.stdout,
                        stderr: output.stderr,
                        exit_code: output.exit_code,
                    }),
                    Err(e) => Err(ExecutionError::CommandFailed { message: e }),
                }
            }
            ActionExecution::Task { task, inputs } => {
                let interpolated_task = self.interpolate(task, context);
                let interpolated_inputs: BTreeMap<String, String> = inputs
                    .iter()
                    .map(|(k, v)| (k.clone(), self.interpolate(v, context)))
                    .collect();

                match self
                    .task_starter
                    .start(&interpolated_task, &interpolated_inputs)
                {
                    Ok(task_id) => Ok(ActionResult::TaskStarted { task_id }),
                    Err(e) => Err(ExecutionError::TaskFailed { message: e }),
                }
            }
            ActionExecution::Rules { rules } => self.evaluate_rules(rules, context),
            ActionExecution::None => Ok(ActionResult::NoOp),
        }
    }

    /// Evaluate decision rules in order
    fn evaluate_rules(
        &self,
        rules: &[DecisionRule],
        context: &ExecutionContext,
    ) -> Result<ActionResult, ExecutionError> {
        for rule in rules {
            // Check if this is an else clause
            if rule.is_else == Some(true) {
                return Ok(ActionResult::RuleMatched {
                    action: rule.then.clone(),
                    delay: rule.delay,
                });
            }

            // If there's a condition, evaluate it
            if let Some(condition) = &rule.condition {
                match self.condition_evaluator.evaluate(condition, context) {
                    Ok(true) => {
                        return Ok(ActionResult::RuleMatched {
                            action: rule.then.clone(),
                            delay: rule.delay,
                        });
                    }
                    Ok(false) => continue,
                    Err(e) => return Err(ExecutionError::ConditionFailed { message: e }),
                }
            } else {
                // No condition means always match (like a default)
                return Ok(ActionResult::RuleMatched {
                    action: rule.then.clone(),
                    delay: rule.delay,
                });
            }
        }

        Err(ExecutionError::NoRuleMatched)
    }

    /// Interpolate variables in a string
    fn interpolate(&self, template: &str, context: &ExecutionContext) -> String {
        let mut result = template.to_string();
        for (key, value) in &context.variables {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result = result.replace("{source}", &context.source);
        result
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

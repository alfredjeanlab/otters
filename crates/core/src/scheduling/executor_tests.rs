use std::collections::{BTreeMap, HashMap};
// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::scheduling::{Action, ActionConfig, ActionId, DecisionRule};
use std::sync::RwLock;
use std::time::Duration;

/// Fake command runner that records commands
struct FakeCommandRunner {
    commands: RwLock<Vec<String>>,
    result: RwLock<Result<CommandOutput, String>>,
}

impl FakeCommandRunner {
    fn new() -> Self {
        Self {
            commands: RwLock::new(vec![]),
            result: RwLock::new(Ok(CommandOutput {
                stdout: "success".to_string(),
                stderr: String::new(),
                exit_code: 0,
            })),
        }
    }

    fn set_result(&self, result: Result<CommandOutput, String>) {
        *self.result.write().unwrap() = result;
    }

    fn commands(&self) -> Vec<String> {
        self.commands.read().unwrap().clone()
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(&self, command: &str, _timeout: Option<Duration>) -> Result<CommandOutput, String> {
        self.commands.write().unwrap().push(command.to_string());
        self.result.read().unwrap().clone()
    }
}

/// Fake task starter that records tasks
struct FakeTaskStarter {
    tasks: RwLock<Vec<(String, BTreeMap<String, String>)>>,
}

impl FakeTaskStarter {
    fn new() -> Self {
        Self {
            tasks: RwLock::new(vec![]),
        }
    }

    fn tasks(&self) -> Vec<(String, BTreeMap<String, String>)> {
        self.tasks.read().unwrap().clone()
    }
}

impl TaskStarter for FakeTaskStarter {
    fn start(&self, task: &str, inputs: &BTreeMap<String, String>) -> Result<String, String> {
        self.tasks
            .write()
            .unwrap()
            .push((task.to_string(), inputs.clone()));
        Ok(format!("task-id-{}", task))
    }
}

/// Fake condition evaluator
struct FakeConditionEvaluator {
    results: RwLock<HashMap<String, bool>>,
}

impl FakeConditionEvaluator {
    fn new() -> Self {
        Self {
            results: RwLock::new(HashMap::new()),
        }
    }

    fn set_result(&self, condition: &str, result: bool) {
        self.results
            .write()
            .unwrap()
            .insert(condition.to_string(), result);
    }
}

impl ConditionEvaluator for FakeConditionEvaluator {
    fn evaluate(&self, condition: &str, _context: &ExecutionContext) -> Result<bool, String> {
        Ok(*self
            .results
            .read()
            .unwrap()
            .get(condition)
            .unwrap_or(&false))
    }
}

#[test]
fn execute_command_action() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let config = ActionConfig::new("nudge", Duration::from_secs(30))
        .with_command("oj session nudge {session}");
    let action = Action::new(ActionId::new("nudge"), config);

    let context = ExecutionContext::new("watcher:idle").with_variable("session", "agent-1");

    let result = executor.execute(&action, &context).unwrap();

    match result {
        ActionResult::CommandOutput { exit_code, .. } => {
            assert_eq!(exit_code, 0);
        }
        _ => panic!("expected CommandOutput"),
    }

    // Verify the command was interpolated correctly
    let commands = command_runner.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0], "oj session nudge agent-1");
}

#[test]
fn execute_task_action() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let config = ActionConfig::new("restart", Duration::from_secs(60)).with_task("restart-session");
    let action = Action::new(ActionId::new("restart"), config);

    let context = ExecutionContext::new("watcher:stuck");

    let result = executor.execute(&action, &context).unwrap();

    match result {
        ActionResult::TaskStarted { task_id } => {
            assert_eq!(task_id, "task-id-restart-session");
        }
        _ => panic!("expected TaskStarted"),
    }

    // Verify task was started
    let tasks = task_starter.tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].0, "restart-session");
}

#[test]
fn execute_rules_action_first_match() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = FakeConditionEvaluator::new();
    evaluator.set_result("count > 3", true);

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let rules = vec![
        DecisionRule::new("escalate").with_condition("count > 3"),
        DecisionRule::new("nudge").with_condition("count > 1"),
        DecisionRule::new("log").as_else(),
    ];

    let config = ActionConfig::new("response", Duration::from_secs(30)).with_rules(rules);
    let action = Action::new(ActionId::new("response"), config);

    let context = ExecutionContext::new("watcher:test");

    let result = executor.execute(&action, &context).unwrap();

    match result {
        ActionResult::RuleMatched { action, delay } => {
            assert_eq!(action, "escalate");
            assert!(delay.is_none());
        }
        _ => panic!("expected RuleMatched"),
    }
}

#[test]
fn execute_rules_action_else_clause() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = FakeConditionEvaluator::new();
    // All conditions evaluate to false

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let rules = vec![
        DecisionRule::new("escalate").with_condition("count > 3"),
        DecisionRule::new("nudge").with_condition("count > 1"),
        DecisionRule::new("log").as_else(),
    ];

    let config = ActionConfig::new("response", Duration::from_secs(30)).with_rules(rules);
    let action = Action::new(ActionId::new("response"), config);

    let context = ExecutionContext::new("watcher:test");

    let result = executor.execute(&action, &context).unwrap();

    match result {
        ActionResult::RuleMatched { action, .. } => {
            assert_eq!(action, "log");
        }
        _ => panic!("expected RuleMatched"),
    }
}

#[test]
fn execute_rules_no_match_error() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = FakeConditionEvaluator::new();

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let rules = vec![
        DecisionRule::new("escalate").with_condition("count > 3"),
        DecisionRule::new("nudge").with_condition("count > 1"),
        // No else clause
    ];

    let config = ActionConfig::new("response", Duration::from_secs(30)).with_rules(rules);
    let action = Action::new(ActionId::new("response"), config);

    let context = ExecutionContext::new("watcher:test");

    let result = executor.execute(&action, &context);

    assert!(matches!(result, Err(ExecutionError::NoRuleMatched)));
}

#[test]
fn execute_none_action() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let config = ActionConfig::new("noop", Duration::from_secs(30));
    let action = Action::new(ActionId::new("noop"), config);

    let context = ExecutionContext::new("test");

    let result = executor.execute(&action, &context).unwrap();

    assert!(matches!(result, ActionResult::NoOp));
}

#[test]
fn command_failure_returns_error() {
    let command_runner = FakeCommandRunner::new();
    command_runner.set_result(Err("command not found".to_string()));
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let config =
        ActionConfig::new("bad", Duration::from_secs(30)).with_command("nonexistent-command");
    let action = Action::new(ActionId::new("bad"), config);

    let context = ExecutionContext::new("test");

    let result = executor.execute(&action, &context);

    assert!(matches!(result, Err(ExecutionError::CommandFailed { .. })));
}

#[test]
fn interpolation_replaces_source() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let config = ActionConfig::new("log", Duration::from_secs(30))
        .with_command("echo triggered by {source}");
    let action = Action::new(ActionId::new("log"), config);

    let context = ExecutionContext::new("watcher:idle-checker");

    executor.execute(&action, &context).unwrap();

    let commands = command_runner.commands();
    assert_eq!(commands[0], "echo triggered by watcher:idle-checker");
}

#[test]
fn rule_with_delay() {
    let command_runner = FakeCommandRunner::new();
    let task_starter = FakeTaskStarter::new();
    let evaluator = AlwaysTrueEvaluator;

    let executor = ActionExecutor::new(&command_runner, &task_starter, &evaluator);

    let rules = vec![DecisionRule::new("delayed-action")
        .with_condition("always")
        .with_delay(Duration::from_secs(60))];

    let config = ActionConfig::new("response", Duration::from_secs(30)).with_rules(rules);
    let action = Action::new(ActionId::new("response"), config);

    let context = ExecutionContext::new("test");

    let result = executor.execute(&action, &context).unwrap();

    match result {
        ActionResult::RuleMatched { action, delay } => {
            assert_eq!(action, "delayed-action");
            assert_eq!(delay, Some(Duration::from_secs(60)));
        }
        _ => panic!("expected RuleMatched"),
    }
}

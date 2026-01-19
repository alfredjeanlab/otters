// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Runbook loader: converts validated runbooks to runtime types.
//!
//! This module provides:
//! - Runtime type definitions that match existing PhaseConfig patterns
//! - Loader that converts RawRunbook to Runbook
//! - RunbookRegistry for cross-runbook references
//!
//! # Example
//!
//! ```ignore
//! use oj_core::runbook::{parse_runbook, validate_runbook, load_runbook};
//!
//! let raw = parse_runbook(toml_content)?;
//! let validated = validate_runbook(&raw)?;
//! let runbook = load_runbook(&validated)?;
//! ```

use super::types::{
    RawAction, RawCleanupAction, RawCron, RawDecisionRule, RawScanner, RawScannerCondition,
    RawScannerSource, RawWatcher, RawWatcherCondition, RawWatcherResponse, RawWatcherSource,
};
use super::validator::ValidatedRunbook;
use crate::scheduling::{
    ActionConfig, ActionExecution, ActionId, CleanupAction, CronConfig, DecisionRule,
    ScannerCondition, ScannerConfig, ScannerId, ScannerSource, WatcherCondition, WatcherConfig,
    WatcherId, WatcherResponse, WatcherSource,
};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

// ============================================================================
// Error types
// ============================================================================

/// Errors that can occur during runbook loading.
#[derive(Debug, Error)]
pub enum LoadError {
    /// IO error reading runbook file
    #[error("IO error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(#[from] super::parser::ParseError),

    /// Validation error
    #[error("Validation failed: {0}")]
    Validation(#[from] super::validator::ValidationErrors),

    /// Missing reference
    #[error("Missing reference: {kind} '{name}' not found")]
    MissingReference { kind: &'static str, name: String },

    /// Invalid reference syntax
    #[error("Invalid reference syntax: {reference}")]
    InvalidReferenceSyntax { reference: String },

    /// Invalid duration format
    #[error("Invalid duration '{value}' in {field}: {reason}")]
    InvalidDuration {
        field: String,
        value: String,
        reason: String,
    },

    /// Invalid value in field
    #[error("Invalid value '{value}' in {field}: expected {expected}")]
    InvalidValue {
        field: String,
        value: String,
        expected: String,
    },

    /// Missing required field
    #[error("Missing required field '{field}' in {context}")]
    MissingField { field: String, context: String },
}

// ============================================================================
// Runtime types
// ============================================================================

/// A loaded runbook with all references resolved.
#[derive(Debug, Clone, Default)]
pub struct Runbook {
    pub name: String,
    pub commands: HashMap<String, Command>,
    pub workers: HashMap<String, WorkerDef>,
    pub queues: HashMap<String, QueueDef>,
    pub pipelines: HashMap<String, PipelineDef>,
    pub tasks: HashMap<String, TaskDef>,
    pub guards: HashMap<String, GuardDef>,
    pub strategies: HashMap<String, StrategyDef>,
    pub locks: HashMap<String, LockDef>,
    pub semaphores: HashMap<String, SemaphoreDef>,
    pub config: HashMap<String, serde_json::Value>,
    pub functions: HashMap<String, FunctionDef>,
    // Scheduling primitives
    pub crons: HashMap<String, CronConfig>,
    pub actions: HashMap<String, ActionConfig>,
    pub watchers: HashMap<String, WatcherConfig>,
    pub scanners: HashMap<String, ScannerConfig>,
}

/// A command entrypoint.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub args: Option<String>,
    pub aliases: BTreeMap<String, String>,
    pub defaults: BTreeMap<String, String>,
    pub run: Option<String>,
}

/// A worker definition.
#[derive(Debug, Clone)]
pub struct WorkerDef {
    pub name: String,
    pub queue: Option<String>,
    pub handler: Option<String>,
    pub concurrency: u32,
    pub idle_action: Option<String>,
    pub wake_on: Vec<String>,
    pub on_unhealthy: Option<String>,
}

/// A queue definition.
#[derive(Debug, Clone)]
pub struct QueueDef {
    pub name: String,
    pub source: Option<String>,
    pub filter: Option<String>,
    pub order: Option<String>,
    pub visibility_timeout: Option<Duration>,
    pub max_retries: Option<u32>,
    pub on_exhaust: Option<String>,
    pub dead_letter: Option<DeadLetterConfig>,
}

/// Dead letter queue configuration.
#[derive(Debug, Clone)]
pub struct DeadLetterConfig {
    pub retention: Option<String>,
    pub on_add: Option<String>,
}

/// A pipeline definition (workflow).
#[derive(Debug, Clone)]
pub struct PipelineDef {
    pub name: String,
    pub inputs: Vec<String>,
    pub defaults: BTreeMap<String, String>,
    pub phases: HashMap<String, PhaseDef>,
    pub initial_phase: String,
}

/// A phase within a pipeline.
#[derive(Debug, Clone)]
pub struct PhaseDef {
    pub name: String,
    pub action: PhaseAction,
    pub pre_guards: Vec<String>,
    pub post_guards: Vec<String>,
    pub lock: Option<String>,
    pub semaphore: Option<String>,
    pub next: PhaseNext,
    pub on_fail: FailAction,
}

/// Action to perform in a phase.
#[derive(Debug, Clone)]
pub enum PhaseAction {
    /// Run shell command
    Run { command: String },
    /// Execute a task
    Task { name: String },
    /// Execute a strategy
    Strategy { name: String },
    /// No action (pure transition phase)
    None,
}

/// Where to go after phase completion.
#[derive(Debug, Clone)]
pub enum PhaseNext {
    Phase(String),
    Done,
}

/// What to do when a phase fails.
#[derive(Debug, Clone)]
pub enum FailAction {
    /// Escalate to pipeline failure
    Escalate,
    /// Go to a specific phase
    GotoPhase(String),
    /// Use a recovery strategy
    UseStrategy(String),
    /// Retry with configuration
    Retry { max: u32, interval: Duration },
}

/// A task definition.
#[derive(Debug, Clone)]
pub struct TaskDef {
    pub name: String,
    pub command: Option<String>,
    pub prompt: Option<String>,
    pub prompt_file: Option<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub heartbeat: Option<String>,
    pub timeout: Option<Duration>,
    pub idle_timeout: Option<Duration>,
    pub on_stuck: Vec<String>,
    pub on_timeout: Option<String>,
    pub checkpoint_interval: Option<Duration>,
    pub checkpoint: Option<String>,
}

/// A guard definition.
#[derive(Debug, Clone)]
pub struct GuardDef {
    pub name: String,
    pub condition: Option<String>,
    pub wake_on: Vec<String>,
    pub timeout: Option<Duration>,
    pub on_timeout: Option<String>,
    pub retry_max: Option<u32>,
    pub retry_interval: Option<Duration>,
}

/// A strategy definition.
#[derive(Debug, Clone)]
pub struct StrategyDef {
    pub name: String,
    pub checkpoint: Option<String>,
    pub attempts: Vec<AttemptDef>,
    pub on_exhausted: ExhaustedAction,
}

/// An attempt within a strategy.
#[derive(Debug, Clone)]
pub struct AttemptDef {
    pub name: String,
    pub run: Option<String>,
    pub task: Option<String>,
    pub timeout: Option<Duration>,
    pub rollback: Option<String>,
}

/// What to do when all attempts are exhausted.
#[derive(Debug, Clone, Default)]
pub enum ExhaustedAction {
    /// Fail the strategy
    #[default]
    Fail,
    /// Escalate to pipeline failure
    Escalate,
    /// Go to a specific phase
    GotoPhase(String),
}

/// A lock definition.
#[derive(Debug, Clone)]
pub struct LockDef {
    pub name: String,
    pub timeout: Option<Duration>,
    pub heartbeat: Option<Duration>,
    pub on_stale: Vec<String>,
}

/// A semaphore definition.
#[derive(Debug, Clone)]
pub struct SemaphoreDef {
    pub name: String,
    pub max: Option<u32>,
    pub slot_timeout: Option<Duration>,
    pub slot_heartbeat: Option<Duration>,
    pub on_orphan: Option<String>,
    pub on_orphan_work: Option<String>,
}

/// A function definition.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub body: String,
}

// ============================================================================
// Loader implementation
// ============================================================================

/// Load a validated runbook into runtime types.
pub fn load_runbook(validated: &ValidatedRunbook) -> Result<Runbook, LoadError> {
    let raw = &validated.raw;
    let mut runbook = Runbook::default();

    // Load commands
    for (name, raw_cmd) in &raw.command {
        let defaults = raw_cmd
            .defaults
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();

        runbook.commands.insert(
            name.clone(),
            Command {
                name: name.clone(),
                args: raw_cmd.args.clone(),
                aliases: raw_cmd.aliases.clone(),
                defaults,
                run: raw_cmd.run.clone(),
            },
        );
    }

    // Load workers
    for (name, raw_worker) in &raw.worker {
        runbook.workers.insert(
            name.clone(),
            WorkerDef {
                name: name.clone(),
                queue: raw_worker.queue.clone(),
                handler: raw_worker.handler.clone(),
                concurrency: raw_worker.concurrency.unwrap_or(1),
                idle_action: raw_worker.idle_action.clone(),
                wake_on: raw_worker.wake_on.clone(),
                on_unhealthy: raw_worker.on_unhealthy.clone(),
            },
        );
    }

    // Load queues
    for (name, raw_queue) in &raw.queue {
        let dead_letter = raw_queue.dead.as_ref().map(|dl| DeadLetterConfig {
            retention: dl.retention.clone(),
            on_add: dl.on_add.clone(),
        });

        runbook.queues.insert(
            name.clone(),
            QueueDef {
                name: name.clone(),
                source: raw_queue.source.clone(),
                filter: raw_queue.filter.clone(),
                order: raw_queue.order.clone(),
                visibility_timeout: raw_queue.visibility_timeout,
                max_retries: raw_queue.max_retries,
                on_exhaust: raw_queue.on_exhaust.clone(),
                dead_letter,
            },
        );
    }

    // Load pipelines
    for (name, raw_pipeline) in &raw.pipeline {
        let mut phases = HashMap::new();
        let mut initial_phase = String::new();

        for (i, raw_phase) in raw_pipeline.phase.iter().enumerate() {
            let phase_name = raw_phase.name.clone();
            if i == 0 {
                initial_phase = phase_name.clone();
            }

            let action = if let Some(ref run) = raw_phase.run {
                PhaseAction::Run {
                    command: run.clone(),
                }
            } else if let Some(ref task) = raw_phase.task {
                PhaseAction::Task { name: task.clone() }
            } else if let Some(ref strategy) = raw_phase.strategy {
                PhaseAction::Strategy {
                    name: strategy.clone(),
                }
            } else {
                PhaseAction::None
            };

            let next = match raw_phase.next.as_deref() {
                Some("done") | None if i == raw_pipeline.phase.len() - 1 => PhaseNext::Done,
                Some(phase) => PhaseNext::Phase(phase.to_string()),
                None if i + 1 < raw_pipeline.phase.len() => {
                    PhaseNext::Phase(raw_pipeline.phase[i + 1].name.clone())
                }
                None => PhaseNext::Done,
            };

            let on_fail = parse_fail_action(raw_phase.on_fail.as_deref());

            phases.insert(
                phase_name.clone(),
                PhaseDef {
                    name: phase_name,
                    action,
                    pre_guards: raw_phase.pre.clone(),
                    post_guards: raw_phase.post.clone(),
                    lock: raw_phase.lock.clone(),
                    semaphore: raw_phase.semaphore.clone(),
                    next,
                    on_fail,
                },
            );
        }

        runbook.pipelines.insert(
            name.clone(),
            PipelineDef {
                name: name.clone(),
                inputs: raw_pipeline.inputs.clone(),
                defaults: raw_pipeline.defaults.clone(),
                phases,
                initial_phase,
            },
        );
    }

    // Load tasks
    for (name, raw_task) in &raw.task {
        runbook.tasks.insert(
            name.clone(),
            TaskDef {
                name: name.clone(),
                command: raw_task.command.clone(),
                prompt: raw_task.prompt.clone(),
                prompt_file: raw_task.prompt_file.clone(),
                env: raw_task.env.clone(),
                cwd: raw_task.cwd.clone(),
                heartbeat: raw_task.heartbeat.clone(),
                timeout: raw_task.timeout,
                idle_timeout: raw_task.idle_timeout,
                on_stuck: raw_task.on_stuck.clone(),
                on_timeout: raw_task.on_timeout.clone(),
                checkpoint_interval: raw_task.checkpoint_interval,
                checkpoint: raw_task.checkpoint.clone(),
            },
        );
    }

    // Load guards
    for (name, raw_guard) in &raw.guard {
        runbook.guards.insert(
            name.clone(),
            GuardDef {
                name: name.clone(),
                condition: raw_guard.condition.clone(),
                wake_on: raw_guard.wake_on.clone(),
                timeout: raw_guard.timeout,
                on_timeout: raw_guard.on_timeout.clone(),
                retry_max: raw_guard.retry.as_ref().and_then(|r| r.max),
                retry_interval: raw_guard.retry.as_ref().and_then(|r| r.interval),
            },
        );
    }

    // Load strategies
    for (name, raw_strategy) in &raw.strategy {
        let attempts = raw_strategy
            .attempt
            .iter()
            .map(|a| AttemptDef {
                name: a.name.clone(),
                run: a.run.clone(),
                task: a.task.clone(),
                timeout: a.timeout,
                rollback: a.rollback.clone(),
            })
            .collect();

        let on_exhausted = match raw_strategy.on_exhaust.as_deref() {
            Some("escalate") => ExhaustedAction::Escalate,
            Some("fail") | None => ExhaustedAction::Fail,
            Some(phase) => ExhaustedAction::GotoPhase(phase.to_string()),
        };

        runbook.strategies.insert(
            name.clone(),
            StrategyDef {
                name: name.clone(),
                checkpoint: raw_strategy.checkpoint.clone(),
                attempts,
                on_exhausted,
            },
        );
    }

    // Load locks
    for (name, raw_lock) in &raw.lock {
        runbook.locks.insert(
            name.clone(),
            LockDef {
                name: name.clone(),
                timeout: raw_lock.timeout,
                heartbeat: raw_lock.heartbeat,
                on_stale: raw_lock.on_stale.clone(),
            },
        );
    }

    // Load semaphores
    for (name, raw_sem) in &raw.semaphore {
        runbook.semaphores.insert(
            name.clone(),
            SemaphoreDef {
                name: name.clone(),
                max: raw_sem.max,
                slot_timeout: raw_sem.slot_timeout,
                slot_heartbeat: raw_sem.slot_heartbeat,
                on_orphan: raw_sem.on_orphan.clone(),
                on_orphan_work: raw_sem.on_orphan_work.clone(),
            },
        );
    }

    // Load config
    for (key, value) in &raw.config {
        if let Ok(json_value) = serde_json::to_value(value) {
            runbook.config.insert(key.clone(), json_value);
        }
    }

    // Load functions
    for (name, body) in &raw.functions {
        runbook.functions.insert(
            name.clone(),
            FunctionDef {
                name: name.clone(),
                body: body.clone(),
            },
        );
    }

    // Load scheduling primitives
    for (name, raw_cron) in &raw.cron {
        runbook
            .crons
            .insert(name.clone(), load_cron(name, raw_cron)?);
    }

    for (name, raw_action) in &raw.action {
        runbook
            .actions
            .insert(name.clone(), load_action(name, raw_action)?);
    }

    for (name, raw_watcher) in &raw.watcher {
        runbook
            .watchers
            .insert(name.clone(), load_watcher(name, raw_watcher)?);
    }

    for (name, raw_scanner) in &raw.scanner {
        runbook
            .scanners
            .insert(name.clone(), load_scanner(name, raw_scanner)?);
    }

    Ok(runbook)
}

/// Parse a fail action string.
fn parse_fail_action(s: Option<&str>) -> FailAction {
    match s {
        Some("escalate") | None => FailAction::Escalate,
        Some(s) if s.starts_with("retry:") => {
            // Parse "retry:3" or "retry:3:1m"
            let parts: Vec<&str> = s[6..].split(':').collect();
            let max = parts.first().and_then(|p| p.parse().ok()).unwrap_or(3);
            let interval = parts
                .get(1)
                .and_then(|p| parse_duration(p))
                .unwrap_or_else(|| Duration::from_secs(60));
            FailAction::Retry { max, interval }
        }
        Some(s) if s.starts_with("strategy:") => FailAction::UseStrategy(s[9..].to_string()),
        Some(phase) => FailAction::GotoPhase(phase.to_string()),
    }
}

/// Parse a duration string like "30s", "5m", "1h".
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(Duration::from_secs(num)),
        "m" => Some(Duration::from_secs(num * 60)),
        "h" => Some(Duration::from_secs(num * 3600)),
        "d" => Some(Duration::from_secs(num * 86400)),
        _ => {
            // Try parsing the whole thing as seconds
            s.parse().ok().map(Duration::from_secs)
        }
    }
}

// ============================================================================
// Scheduling primitive loaders
// ============================================================================

/// Load a cron from raw TOML representation.
fn load_cron(name: &str, raw: &RawCron) -> Result<CronConfig, LoadError> {
    let interval = raw.interval.ok_or_else(|| LoadError::MissingField {
        field: "interval".into(),
        context: format!("cron.{}", name),
    })?;

    let mut config = CronConfig::new(name, interval);
    if raw.enabled {
        config = config.enabled();
    }
    config = config.with_watchers(raw.watchers.iter().map(WatcherId::new).collect());
    config = config.with_scanners(raw.scanners.iter().map(ScannerId::new).collect());

    Ok(config)
}

/// Load an action from raw TOML representation.
fn load_action(name: &str, raw: &RawAction) -> Result<ActionConfig, LoadError> {
    let cooldown = raw.cooldown.ok_or_else(|| LoadError::MissingField {
        field: "cooldown".into(),
        context: format!("action.{}", name),
    })?;

    let mut config = ActionConfig::new(name, cooldown);

    if let Some(cmd) = &raw.command {
        config.execution = ActionExecution::Command {
            run: cmd.clone(),
            timeout: None,
        };
    } else if let Some(task) = &raw.task {
        config.execution = ActionExecution::Task {
            task: task.clone(),
            inputs: raw
                .args
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
        };
    } else if !raw.rules.is_empty() {
        let rules = raw
            .rules
            .iter()
            .map(load_decision_rule)
            .collect::<Result<Vec<_>, _>>()?;
        config.execution = ActionExecution::Rules { rules };
    }

    Ok(config)
}

/// Load a decision rule from raw TOML representation.
fn load_decision_rule(raw: &RawDecisionRule) -> Result<DecisionRule, LoadError> {
    let mut rule = DecisionRule::new(&raw.then);
    if let Some(cond) = &raw.condition {
        rule = rule.with_condition(cond);
    }
    if raw.is_else.unwrap_or(false) {
        rule = rule.as_else();
    }
    if let Some(delay) = raw.delay {
        rule = rule.with_delay(delay);
    }
    Ok(rule)
}

/// Load a watcher from raw TOML representation.
fn load_watcher(name: &str, raw: &RawWatcher) -> Result<WatcherConfig, LoadError> {
    let source = load_watcher_source(name, &raw.source)?;
    let condition = load_watcher_condition(name, &raw.condition)?;
    let check_interval = raw.check_interval.ok_or_else(|| LoadError::MissingField {
        field: "check_interval".into(),
        context: format!("watcher.{}", name),
    })?;

    let responses = raw
        .response
        .iter()
        .map(load_watcher_response)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(WatcherConfig::new(name, source, condition, check_interval)
        .with_responses(responses)
        .with_wake_on(raw.wake_on.clone()))
}

/// Load a watcher source from raw TOML representation.
fn load_watcher_source(
    watcher_name: &str,
    raw: &RawWatcherSource,
) -> Result<WatcherSource, LoadError> {
    match raw.source_type.as_str() {
        "session" => Ok(WatcherSource::Session {
            name: raw
                .pattern
                .clone()
                .or_else(|| raw.name.clone())
                .unwrap_or_default(),
        }),
        "events" => Ok(WatcherSource::Events {
            pattern: raw.pattern.clone().unwrap_or_default(),
        }),
        "command" => Ok(WatcherSource::Command {
            command: raw.pattern.clone().unwrap_or_default(),
        }),
        "" => {
            // Default to session if no type specified
            Ok(WatcherSource::Session {
                name: raw
                    .pattern
                    .clone()
                    .or_else(|| raw.name.clone())
                    .unwrap_or_default(),
            })
        }
        other => Err(LoadError::InvalidValue {
            field: format!("watcher.{}.source.type", watcher_name),
            value: other.into(),
            expected: "session, events, command".into(),
        }),
    }
}

/// Load a watcher condition from raw TOML representation.
fn load_watcher_condition(
    watcher_name: &str,
    raw: &RawWatcherCondition,
) -> Result<WatcherCondition, LoadError> {
    match raw.condition_type.as_str() {
        "idle" => Ok(WatcherCondition::Idle {
            threshold: raw.threshold.unwrap_or(Duration::from_secs(300)),
        }),
        "exceeds" => Ok(WatcherCondition::Exceeds {
            threshold: raw.count.map(|c| c as u64).unwrap_or(10),
        }),
        "consecutive_failures" => Ok(WatcherCondition::ConsecutiveFailures {
            count: raw.count.unwrap_or(3),
        }),
        "matches" => Ok(WatcherCondition::Matches {
            pattern: raw.pattern.clone().unwrap_or_default(),
        }),
        "" => {
            // Default to idle if no type specified
            Ok(WatcherCondition::Idle {
                threshold: raw.threshold.unwrap_or(Duration::from_secs(300)),
            })
        }
        other => Err(LoadError::InvalidValue {
            field: format!("watcher.{}.condition.type", watcher_name),
            value: other.into(),
            expected: "idle, exceeds, consecutive_failures, matches".into(),
        }),
    }
}

/// Load a watcher response from raw TOML representation.
fn load_watcher_response(raw: &RawWatcherResponse) -> Result<WatcherResponse, LoadError> {
    let mut response = WatcherResponse::new(ActionId::new(&raw.action));
    if let Some(delay) = raw.delay {
        response = response.with_delay(delay);
    }
    if raw.requires_previous_failure {
        response = response.requires_previous_failure();
    }
    Ok(response)
}

/// Load a scanner from raw TOML representation.
fn load_scanner(name: &str, raw: &RawScanner) -> Result<ScannerConfig, LoadError> {
    let source = load_scanner_source(name, &raw.source)?;
    let condition = load_scanner_condition(name, &raw.condition)?;
    let cleanup = load_cleanup_action(name, &raw.cleanup)?;
    let interval = raw.interval.ok_or_else(|| LoadError::MissingField {
        field: "interval".into(),
        context: format!("scanner.{}", name),
    })?;

    Ok(ScannerConfig::new(
        name, source, condition, cleanup, interval,
    ))
}

/// Load a scanner source from raw TOML representation.
fn load_scanner_source(
    scanner_name: &str,
    raw: &RawScannerSource,
) -> Result<ScannerSource, LoadError> {
    match raw.source_type.as_str() {
        "locks" => Ok(ScannerSource::Locks),
        "sessions" => Ok(ScannerSource::Sessions),
        "worktrees" => Ok(ScannerSource::Worktrees),
        "pipelines" => Ok(ScannerSource::Pipelines),
        "tasks" => Ok(ScannerSource::Tasks),
        "queue" => Ok(ScannerSource::Queue {
            name: raw.name.clone().unwrap_or_default(),
        }),
        "" => {
            // Default to locks if no type specified
            Ok(ScannerSource::Locks)
        }
        other => Err(LoadError::InvalidValue {
            field: format!("scanner.{}.source.type", scanner_name),
            value: other.into(),
            expected: "locks, sessions, worktrees, pipelines, tasks, queue".into(),
        }),
    }
}

/// Load a scanner condition from raw TOML representation.
fn load_scanner_condition(
    scanner_name: &str,
    raw: &RawScannerCondition,
) -> Result<ScannerCondition, LoadError> {
    match raw.condition_type.as_str() {
        "stale" => Ok(ScannerCondition::Stale {
            threshold: raw.threshold.unwrap_or(Duration::from_secs(3600)),
        }),
        "orphaned" => Ok(ScannerCondition::Orphaned),
        "exceeded_attempts" => Ok(ScannerCondition::ExceededAttempts {
            max: raw.max.unwrap_or(3),
        }),
        "terminal_for" => Ok(ScannerCondition::TerminalFor {
            threshold: raw.threshold.unwrap_or(Duration::from_secs(86400)),
        }),
        "" => {
            // Default to stale if no type specified
            Ok(ScannerCondition::Stale {
                threshold: raw.threshold.unwrap_or(Duration::from_secs(3600)),
            })
        }
        other => Err(LoadError::InvalidValue {
            field: format!("scanner.{}.condition.type", scanner_name),
            value: other.into(),
            expected: "stale, orphaned, exceeded_attempts, terminal_for".into(),
        }),
    }
}

/// Load a cleanup action from raw TOML representation.
fn load_cleanup_action(
    scanner_name: &str,
    raw: &RawCleanupAction,
) -> Result<CleanupAction, LoadError> {
    match raw.action_type.as_str() {
        "release" => Ok(CleanupAction::Release),
        "delete" => Ok(CleanupAction::Delete),
        "archive" => Ok(CleanupAction::Archive {
            destination: raw.destination.clone().unwrap_or_default(),
        }),
        "dead_letter" => Ok(CleanupAction::DeadLetter),
        "" => {
            // Default to release if no type specified
            Ok(CleanupAction::Release)
        }
        other => Err(LoadError::InvalidValue {
            field: format!("scanner.{}.cleanup.type", scanner_name),
            value: other.into(),
            expected: "release, delete, archive, dead_letter".into(),
        }),
    }
}

// ============================================================================
// RunbookRegistry for cross-runbook references
// ============================================================================

/// Registry for loaded runbooks, supporting cross-runbook references.
#[derive(Debug, Default)]
pub struct RunbookRegistry {
    runbooks: HashMap<String, Runbook>,
    raw_runbooks: HashMap<String, super::types::RawRunbook>,
}

impl RunbookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a runbook to the registry.
    pub fn add(&mut self, name: impl Into<String>, runbook: Runbook) {
        self.runbooks.insert(name.into(), runbook);
    }

    /// Add a raw runbook to the registry for cross-reference validation.
    pub fn add_raw(&mut self, name: impl Into<String>, raw: super::types::RawRunbook) {
        self.raw_runbooks.insert(name.into(), raw);
    }

    /// Get a runbook by name.
    pub fn get(&self, name: &str) -> Option<&Runbook> {
        self.runbooks.get(name)
    }

    /// Get a raw runbook by name.
    pub fn get_raw(&self, name: &str) -> Option<&super::types::RawRunbook> {
        self.raw_runbooks.get(name)
    }

    /// Load all runbooks from a directory.
    pub fn load_directory(&mut self, dir: &Path) -> Result<Vec<String>, LoadError> {
        let mut loaded = Vec::new();

        if !dir.exists() {
            return Ok(loaded);
        }

        let entries = std::fs::read_dir(dir).map_err(|e| LoadError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                if let Some(name) = super::parser::runbook_name(&path) {
                    let name = name.to_string();
                    let runbook = load_runbook_file(&path)?;
                    self.add(name.clone(), runbook);
                    loaded.push(name);
                }
            }
        }

        Ok(loaded)
    }

    /// Load and validate all runbooks in a directory, including cross-reference validation.
    ///
    /// This performs a two-pass load:
    /// 1. First pass: load all runbooks without cross-ref validation
    /// 2. Second pass: validate cross-references across all runbooks
    ///
    /// Returns Ok(loaded_names) on success, or Err with a list of (runbook_name, errors) pairs.
    pub fn load_directory_validated(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<Vec<String>, Vec<(String, Vec<super::validator::ValidationError>)>> {
        use super::parser::parse_runbook_file;
        use super::validator::{validate_cross_references, validate_runbook, ValidationError};

        let path = path.as_ref();
        let mut all_errors: Vec<(String, Vec<ValidationError>)> = Vec::new();
        let mut loaded_names = Vec::new();

        if !path.exists() {
            return Ok(loaded_names);
        }

        // First pass: load all runbooks without cross-ref validation
        let entries = match std::fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => {
                all_errors.push((
                    "directory".into(),
                    vec![ValidationError::MissingRequired {
                        item_kind: "directory",
                        item_name: path.display().to_string(),
                        field: e.to_string().leak(),
                    }],
                ));
                return Err(all_errors);
            }
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.extension().is_some_and(|e| e == "toml") {
                let name = super::parser::runbook_name(&entry_path)
                    .unwrap_or("unknown")
                    .to_string();

                // Parse the runbook
                let raw = match parse_runbook_file(&entry_path) {
                    Ok(r) => r,
                    Err(e) => {
                        all_errors.push((
                            name,
                            vec![ValidationError::MissingRequired {
                                item_kind: "runbook",
                                item_name: entry_path.display().to_string(),
                                field: e.to_string().leak(),
                            }],
                        ));
                        continue;
                    }
                };

                // Validate locally first
                match validate_runbook(&raw) {
                    Ok(validated) => {
                        // Load into registry
                        match load_runbook(&validated) {
                            Ok(runbook) => {
                                self.add(name.clone(), runbook);
                                self.add_raw(name.clone(), raw);
                                loaded_names.push(name);
                            }
                            Err(e) => {
                                all_errors.push((
                                    name,
                                    vec![ValidationError::MissingRequired {
                                        item_kind: "runbook",
                                        item_name: entry_path.display().to_string(),
                                        field: e.to_string().leak(),
                                    }],
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        all_errors.push((name, e.errors));
                    }
                }
            }
        }

        // Second pass: validate cross-references
        for name in &loaded_names {
            if let Some(raw) = self.get_raw(name) {
                let cross_errors = validate_cross_references(raw, self);
                if !cross_errors.is_empty() {
                    all_errors.push((name.clone(), cross_errors));
                }
            }
        }

        if all_errors.is_empty() {
            Ok(loaded_names)
        } else {
            Err(all_errors)
        }
    }

    /// Resolve a cross-runbook reference.
    ///
    /// Reference syntax:
    /// - Same runbook: `task.planning`, `guard.plan_exists`
    /// - Cross-runbook: `common.task.planning`, `shared.guard.file_exists`
    pub fn resolve_task(&self, runbook_name: &str, reference: &str) -> Option<&TaskDef> {
        let (rb_name, task_name) = parse_reference(runbook_name, reference)?;
        self.runbooks.get(rb_name)?.tasks.get(task_name)
    }

    /// Resolve a guard reference.
    pub fn resolve_guard(&self, runbook_name: &str, reference: &str) -> Option<&GuardDef> {
        let (rb_name, guard_name) = parse_reference(runbook_name, reference)?;
        self.runbooks.get(rb_name)?.guards.get(guard_name)
    }

    /// Resolve a strategy reference.
    pub fn resolve_strategy(&self, runbook_name: &str, reference: &str) -> Option<&StrategyDef> {
        let (rb_name, strategy_name) = parse_reference(runbook_name, reference)?;
        self.runbooks.get(rb_name)?.strategies.get(strategy_name)
    }

    /// Resolve a pipeline reference.
    pub fn resolve_pipeline(&self, runbook_name: &str, reference: &str) -> Option<&PipelineDef> {
        let (rb_name, pipeline_name) = parse_reference(runbook_name, reference)?;
        self.runbooks.get(rb_name)?.pipelines.get(pipeline_name)
    }

    /// List all runbook names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.runbooks.keys()
    }

    /// Get the number of loaded runbooks.
    pub fn len(&self) -> usize {
        self.runbooks.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.runbooks.is_empty()
    }
}

/// Parse a reference into (runbook_name, item_name).
///
/// - "task.planning" with current runbook "build" -> ("build", "planning")
/// - "common.task.planning" -> ("common", "planning")
fn parse_reference<'a>(current_runbook: &'a str, reference: &'a str) -> Option<(&'a str, &'a str)> {
    let parts: Vec<&str> = reference.split('.').collect();
    match parts.as_slice() {
        // Same runbook: "task.name" or just "name"
        [_kind, name] => Some((current_runbook, *name)),
        [name] => Some((current_runbook, *name)),
        // Cross-runbook: "runbook.kind.name"
        [runbook, _kind, name] => Some((*runbook, *name)),
        _ => None,
    }
}

/// Load a runbook from a file path.
pub fn load_runbook_file(path: &Path) -> Result<Runbook, LoadError> {
    use super::parser::parse_runbook_file;
    use super::validator::validate_runbook;

    let raw = parse_runbook_file(path)?;
    let validated = validate_runbook(&raw)?;
    load_runbook(&validated)
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;

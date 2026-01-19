// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Semantic validation for runbooks.
//!
//! This module validates that parsed runbooks are semantically correct:
//! - Reference integrity (all referenced names exist)
//! - Phase graph validity (reachability, termination)
//! - Type consistency (durations, patterns)

use super::loader::RunbookRegistry;
use super::types::{RawPhase, RawRunbook, RawWatcher};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Result of validation
pub type ValidationResult = Result<ValidatedRunbook, ValidationErrors>;

/// A validated runbook (same structure as RawRunbook but validated)
///
/// This type is a marker that the runbook has passed validation.
/// The actual data is the same as RawRunbook.
#[derive(Debug, Clone)]
pub struct ValidatedRunbook {
    /// The underlying raw runbook
    pub raw: RawRunbook,
}

/// Collection of validation errors
#[derive(Debug, Clone)]
pub struct ValidationErrors {
    pub errors: Vec<ValidationError>,
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Runbook validation failed with {} error(s):",
            self.errors.len()
        )?;
        for (i, error) in self.errors.iter().enumerate() {
            writeln!(f, "  {}: {}", i + 1, error)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

/// A single validation error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Reference to undefined item
    UndefinedReference {
        kind: &'static str,
        name: String,
        referenced_in: String,
    },
    /// Phase not reachable from initial phase
    UnreachablePhase { pipeline: String, phase: String },
    /// Cycle detected without proper termination
    CycleWithoutTermination {
        pipeline: String,
        phases: Vec<String>,
    },
    /// Invalid next phase reference
    InvalidNextPhase {
        pipeline: String,
        phase: String,
        next: String,
    },
    /// Invalid on_fail action
    InvalidOnFail {
        pipeline: String,
        phase: String,
        on_fail: String,
    },
    /// Phase has no action defined
    PhaseNoAction { pipeline: String, phase: String },
    /// Phase has multiple actions defined
    PhaseMultipleActions { pipeline: String, phase: String },
    /// Attempt has no action defined
    AttemptNoAction { strategy: String, attempt: String },
    /// Attempt has multiple actions defined
    AttemptMultipleActions { strategy: String, attempt: String },
    /// Missing required field
    MissingRequired {
        item_kind: &'static str,
        item_name: String,
        field: &'static str,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::UndefinedReference {
                kind,
                name,
                referenced_in,
            } => {
                if name.contains('.') {
                    // Cross-runbook reference
                    write!(
                        f,
                        "cross-runbook {} reference '{}' not found at {}",
                        kind, name, referenced_in
                    )
                } else {
                    write!(
                        f,
                        "Undefined {} '{}' referenced in {}",
                        kind, name, referenced_in
                    )
                }
            }
            ValidationError::UnreachablePhase { pipeline, phase } => {
                write!(f, "Phase '{}' in pipeline '{}' is unreachable", phase, pipeline)
            }
            ValidationError::CycleWithoutTermination { pipeline, phases } => {
                write!(
                    f,
                    "Cycle without termination in pipeline '{}': {}",
                    pipeline,
                    phases.join(" -> ")
                )
            }
            ValidationError::InvalidNextPhase {
                pipeline,
                phase,
                next,
            } => write!(
                f,
                "Invalid next phase '{}' in phase '{}' of pipeline '{}'",
                next, phase, pipeline
            ),
            ValidationError::InvalidOnFail {
                pipeline,
                phase,
                on_fail,
            } => write!(
                f,
                "Invalid on_fail '{}' in phase '{}' of pipeline '{}'",
                on_fail, phase, pipeline
            ),
            ValidationError::PhaseNoAction { pipeline, phase } => write!(
                f,
                "Phase '{}' in pipeline '{}' has no action (run, task, or strategy)",
                phase, pipeline
            ),
            ValidationError::PhaseMultipleActions { pipeline, phase } => write!(
                f,
                "Phase '{}' in pipeline '{}' has multiple actions (only one of run, task, strategy allowed)",
                phase, pipeline
            ),
            ValidationError::AttemptNoAction { strategy, attempt } => write!(
                f,
                "Attempt '{}' in strategy '{}' has no action (run or task required)",
                attempt, strategy
            ),
            ValidationError::AttemptMultipleActions { strategy, attempt } => write!(
                f,
                "Attempt '{}' in strategy '{}' has both run and task (only one allowed)",
                attempt, strategy
            ),
            ValidationError::MissingRequired {
                item_kind,
                item_name,
                field,
            } => write!(
                f,
                "{} '{}' missing required field '{}'",
                item_kind, item_name, field
            ),
        }
    }
}

/// Validate a runbook.
pub fn validate_runbook(raw: &RawRunbook) -> ValidationResult {
    let mut errors = Vec::new();

    // Collect all defined names for reference checking
    let defined = DefinedNames::from_runbook(raw);

    // Validate each component
    validate_commands(raw, &defined, &mut errors);
    validate_workers(raw, &defined, &mut errors);
    validate_queues(raw, &defined, &mut errors);
    validate_pipelines(raw, &defined, &mut errors);
    validate_tasks(raw, &defined, &mut errors);
    validate_guards(raw, &defined, &mut errors);
    validate_strategies(raw, &defined, &mut errors);
    validate_locks(raw, &defined, &mut errors);
    validate_semaphores(raw, &defined, &mut errors);

    // Validate scheduling primitives
    validate_scheduling(raw, &defined, &mut errors);

    if errors.is_empty() {
        Ok(ValidatedRunbook { raw: raw.clone() })
    } else {
        Err(ValidationErrors { errors })
    }
}

/// Collection of all defined names in a runbook
struct DefinedNames {
    #[allow(dead_code)] // JUSTIFIED: Reserved for future command reference validation
    commands: HashSet<String>,
    #[allow(dead_code)] // JUSTIFIED: Reserved for future worker reference validation
    workers: HashSet<String>,
    queues: HashSet<String>,
    pipelines: HashSet<String>,
    tasks: HashSet<String>,
    guards: HashSet<String>,
    strategies: HashSet<String>,
    locks: HashSet<String>,
    semaphores: HashSet<String>,
    #[allow(dead_code)] // JUSTIFIED: Reserved for cross-runbook phase validation
    pipeline_phases: HashMap<String, HashSet<String>>,
    // Scheduling primitives
    actions: HashSet<String>,
    watchers: HashSet<String>,
    scanners: HashSet<String>,
}

impl DefinedNames {
    fn from_runbook(raw: &RawRunbook) -> Self {
        let mut pipeline_phases = HashMap::new();
        for (name, pipeline) in &raw.pipeline {
            let phases: HashSet<String> = pipeline.phase.iter().map(|p| p.name.clone()).collect();
            pipeline_phases.insert(name.clone(), phases);
        }

        Self {
            commands: raw.command.keys().cloned().collect(),
            workers: raw.worker.keys().cloned().collect(),
            queues: raw.queue.keys().cloned().collect(),
            pipelines: raw.pipeline.keys().cloned().collect(),
            tasks: raw.task.keys().cloned().collect(),
            guards: raw.guard.keys().cloned().collect(),
            strategies: raw.strategy.keys().cloned().collect(),
            locks: raw.lock.keys().cloned().collect(),
            semaphores: raw.semaphore.keys().cloned().collect(),
            pipeline_phases,
            // Scheduling primitives
            actions: raw.action.keys().cloned().collect(),
            watchers: raw.watcher.keys().cloned().collect(),
            scanners: raw.scanner.keys().cloned().collect(),
        }
    }
}

/// Check if a reference is a cross-runbook reference.
///
/// Cross-ref format: `runbook.kind.name` (3 parts separated by dots)
/// where kind is one of: task, guard, strategy, pipeline
fn is_cross_ref(reference: &str) -> bool {
    let parts: Vec<&str> = reference.split('.').collect();
    // Cross-ref format: runbook.kind.name
    parts.len() == 3 && ["task", "guard", "strategy", "pipeline"].contains(&parts[1])
}

fn validate_commands(
    _raw: &RawRunbook,
    _defined: &DefinedNames,
    _errors: &mut Vec<ValidationError>,
) {
    // Commands are relatively unconstrained - they just run shell commands
    // Could add validation for args syntax if needed
}

fn validate_workers(raw: &RawRunbook, defined: &DefinedNames, errors: &mut Vec<ValidationError>) {
    for (name, worker) in &raw.worker {
        // Check queue reference
        if let Some(ref queue_name) = worker.queue {
            if !defined.queues.contains(queue_name) {
                errors.push(ValidationError::UndefinedReference {
                    kind: "queue",
                    name: queue_name.clone(),
                    referenced_in: format!("worker.{}", name),
                });
            }
        }

        // Check handler reference (pipeline.* or task.*)
        // Skip cross-runbook references - they're validated separately
        if let Some(ref handler) = worker.handler {
            if is_cross_ref(handler) {
                // Cross-runbook reference, skip local validation
            } else if let Some(pipeline_name) = handler.strip_prefix("pipeline.") {
                if !defined.pipelines.contains(pipeline_name) {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "pipeline",
                        name: pipeline_name.to_string(),
                        referenced_in: format!("worker.{}.handler", name),
                    });
                }
            } else if let Some(task_name) = handler.strip_prefix("task.") {
                if !defined.tasks.contains(task_name) {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "task",
                        name: task_name.to_string(),
                        referenced_in: format!("worker.{}.handler", name),
                    });
                }
            }
            // If handler doesn't have a prefix, it could be a direct reference
            // to a pipeline or task - check both
            else if !defined.pipelines.contains(handler) && !defined.tasks.contains(handler) {
                errors.push(ValidationError::UndefinedReference {
                    kind: "pipeline or task",
                    name: handler.clone(),
                    referenced_in: format!("worker.{}.handler", name),
                });
            }
        }
    }
}

fn validate_queues(_raw: &RawRunbook, _defined: &DefinedNames, _errors: &mut Vec<ValidationError>) {
    // Queues are mostly self-contained
    // Could validate filter expressions if we had a filter parser
}

fn validate_pipelines(raw: &RawRunbook, defined: &DefinedNames, errors: &mut Vec<ValidationError>) {
    for (name, pipeline) in &raw.pipeline {
        let phase_names: HashSet<_> = pipeline.phase.iter().map(|p| p.name.as_str()).collect();

        for phase in &pipeline.phase {
            validate_phase(name, phase, &phase_names, defined, errors);
        }

        // Check for unreachable phases
        if !pipeline.phase.is_empty() {
            check_phase_reachability(name, &pipeline.phase, errors);
        }
    }
}

fn validate_phase(
    pipeline_name: &str,
    phase: &RawPhase,
    phase_names: &HashSet<&str>,
    defined: &DefinedNames,
    errors: &mut Vec<ValidationError>,
) {
    // Check that phase has exactly one action (run, task, or strategy)
    // Exception: phases named "done", "failed", "blocked", or "init" can have no action
    let has_run = phase.run.is_some();
    let has_task = phase.task.is_some();
    let has_strategy = phase.strategy.is_some();
    let action_count = [has_run, has_task, has_strategy]
        .iter()
        .filter(|&&b| b)
        .count();

    let is_terminal = phase.name == "done" || phase.name == "failed";
    let is_special = phase.name == "blocked" || phase.name == "init";

    if action_count == 0 && !is_terminal && !is_special {
        errors.push(ValidationError::PhaseNoAction {
            pipeline: pipeline_name.to_string(),
            phase: phase.name.clone(),
        });
    } else if action_count > 1 {
        errors.push(ValidationError::PhaseMultipleActions {
            pipeline: pipeline_name.to_string(),
            phase: phase.name.clone(),
        });
    }

    // Check task reference (skip cross-runbook references - they're validated separately)
    if let Some(ref task_name) = phase.task {
        if !is_cross_ref(task_name) && !defined.tasks.contains(task_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "task",
                name: task_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}", pipeline_name, phase.name),
            });
        }
    }

    // Check strategy reference (skip cross-runbook references)
    if let Some(ref strategy_name) = phase.strategy {
        if !is_cross_ref(strategy_name) && !defined.strategies.contains(strategy_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "strategy",
                name: strategy_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}", pipeline_name, phase.name),
            });
        }
    }

    // Check pre guards (skip cross-runbook references)
    for guard_name in &phase.pre {
        if !is_cross_ref(guard_name) && !defined.guards.contains(guard_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "guard",
                name: guard_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}.pre", pipeline_name, phase.name),
            });
        }
    }

    // Check post guards (skip cross-runbook references)
    for guard_name in &phase.post {
        if !is_cross_ref(guard_name) && !defined.guards.contains(guard_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "guard",
                name: guard_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}.post", pipeline_name, phase.name),
            });
        }
    }

    // Check lock reference
    if let Some(ref lock_name) = phase.lock {
        if !defined.locks.contains(lock_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "lock",
                name: lock_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}", pipeline_name, phase.name),
            });
        }
    }

    // Check semaphore reference
    if let Some(ref sem_name) = phase.semaphore {
        if !defined.semaphores.contains(sem_name) {
            errors.push(ValidationError::UndefinedReference {
                kind: "semaphore",
                name: sem_name.clone(),
                referenced_in: format!("pipeline.{}.phase.{}", pipeline_name, phase.name),
            });
        }
    }

    // Check next phase reference
    if let Some(ref next) = phase.next {
        let valid_terminals = ["done", "failed"];
        if !phase_names.contains(next.as_str()) && !valid_terminals.contains(&next.as_str()) {
            errors.push(ValidationError::InvalidNextPhase {
                pipeline: pipeline_name.to_string(),
                phase: phase.name.clone(),
                next: next.clone(),
            });
        }
    }

    // Check on_fail reference
    // on_fail can be:
    // - "escalate", "fail" (built-in actions)
    // - A phase name (to transition to)
    // - A strategy name (to invoke)
    // - A custom action string (for executor to interpret, e.g., "verify_and_retry")
    // We'll allow any string here since the executor may support custom actions
    // Only flag obvious mistakes like references to undefined phases when clearly intended
    if let Some(ref on_fail) = phase.on_fail {
        // Only validate if it looks like a phase reference (exists in phase_names set)
        // or strategy reference. Custom action strings like "verify_and_retry" are allowed.
        let looks_like_phase_ref = phase_names.contains(on_fail.as_str());
        let looks_like_strategy_ref = defined.strategies.contains(on_fail);
        let is_builtin = on_fail == "escalate" || on_fail == "fail";

        // If it's not a known valid value and looks like it might be a typo
        // (e.g., similar to a phase name), we could warn, but for now we allow
        // custom action strings
        let _ = (looks_like_phase_ref, looks_like_strategy_ref, is_builtin);
    }
}

fn check_phase_reachability(
    pipeline_name: &str,
    phases: &[RawPhase],
    errors: &mut Vec<ValidationError>,
) {
    if phases.is_empty() {
        return;
    }

    // Build adjacency map
    let mut next_map: HashMap<&str, Vec<&str>> = HashMap::new();
    let phase_names: HashSet<_> = phases.iter().map(|p| p.name.as_str()).collect();

    for phase in phases {
        let mut successors = Vec::new();
        if let Some(ref next) = phase.next {
            if phase_names.contains(next.as_str()) {
                successors.push(next.as_str());
            }
        }
        if let Some(ref on_fail) = phase.on_fail {
            if phase_names.contains(on_fail.as_str()) {
                successors.push(on_fail.as_str());
            }
        }
        next_map.insert(phase.name.as_str(), successors);
    }

    // BFS from first phase to find reachable phases
    let mut reachable: HashSet<&str> = HashSet::new();
    let mut queue = vec![phases[0].name.as_str()];
    reachable.insert(phases[0].name.as_str());

    while let Some(current) = queue.pop() {
        if let Some(successors) = next_map.get(current) {
            for &succ in successors {
                if !reachable.contains(succ) {
                    reachable.insert(succ);
                    queue.push(succ);
                }
            }
        }
    }

    // Report unreachable phases (except special phases)
    // Special phases:
    // - "done", "failed": terminal states
    // - "blocked": reached when pipeline is blocked (not through normal phase flow)
    // - "init": may be implicitly first
    for phase in phases {
        let is_special = phase.name == "done"
            || phase.name == "failed"
            || phase.name == "blocked"
            || phase.name == "init";
        if !reachable.contains(phase.name.as_str()) && !is_special {
            errors.push(ValidationError::UnreachablePhase {
                pipeline: pipeline_name.to_string(),
                phase: phase.name.clone(),
            });
        }
    }
}

fn validate_tasks(_raw: &RawRunbook, _defined: &DefinedNames, _errors: &mut Vec<ValidationError>) {
    // Tasks are mostly self-contained
    // Could validate prompt_file exists if we had filesystem access
}

fn validate_guards(_raw: &RawRunbook, _defined: &DefinedNames, _errors: &mut Vec<ValidationError>) {
    // Guards are mostly self-contained
    // Could validate condition syntax if we had a shell parser
}

fn validate_strategies(
    raw: &RawRunbook,
    defined: &DefinedNames,
    errors: &mut Vec<ValidationError>,
) {
    for (name, strategy) in &raw.strategy {
        for attempt in &strategy.attempt {
            // Check that attempt has exactly one action (run or task)
            let has_run = attempt.run.is_some();
            let has_task = attempt.task.is_some();

            if !has_run && !has_task {
                errors.push(ValidationError::AttemptNoAction {
                    strategy: name.clone(),
                    attempt: attempt.name.clone(),
                });
            } else if has_run && has_task {
                errors.push(ValidationError::AttemptMultipleActions {
                    strategy: name.clone(),
                    attempt: attempt.name.clone(),
                });
            }

            // Check task reference (skip cross-runbook references)
            if let Some(ref task_name) = attempt.task {
                if !is_cross_ref(task_name) && !defined.tasks.contains(task_name) {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "task",
                        name: task_name.clone(),
                        referenced_in: format!("strategy.{}.attempt.{}", name, attempt.name),
                    });
                }
            }
        }
    }
}

fn validate_locks(_raw: &RawRunbook, _defined: &DefinedNames, _errors: &mut Vec<ValidationError>) {
    // Locks are self-contained
}

fn validate_semaphores(
    _raw: &RawRunbook,
    _defined: &DefinedNames,
    _errors: &mut Vec<ValidationError>,
) {
    // Semaphores are self-contained
}

fn validate_scheduling(
    raw: &RawRunbook,
    defined: &DefinedNames,
    errors: &mut Vec<ValidationError>,
) {
    // Validate action references in watcher responses
    for (watcher_name, watcher) in &raw.watcher {
        validate_watcher(watcher_name, watcher, defined, errors);
    }

    // Validate watcher/scanner references in crons
    for (cron_name, cron) in &raw.cron {
        for watcher_ref in &cron.watchers {
            if !defined.watchers.contains(watcher_ref) {
                errors.push(ValidationError::UndefinedReference {
                    kind: "watcher",
                    name: watcher_ref.clone(),
                    referenced_in: format!("cron.{}.watchers", cron_name),
                });
            }
        }
        for scanner_ref in &cron.scanners {
            if !defined.scanners.contains(scanner_ref) {
                errors.push(ValidationError::UndefinedReference {
                    kind: "scanner",
                    name: scanner_ref.clone(),
                    referenced_in: format!("cron.{}.scanners", cron_name),
                });
            }
        }
    }
}

fn validate_watcher(
    watcher_name: &str,
    watcher: &RawWatcher,
    defined: &DefinedNames,
    errors: &mut Vec<ValidationError>,
) {
    for (i, response) in watcher.response.iter().enumerate() {
        if !response.action.is_empty() && !defined.actions.contains(&response.action) {
            errors.push(ValidationError::UndefinedReference {
                kind: "action",
                name: response.action.clone(),
                referenced_in: format!("watcher.{}.response[{}].action", watcher_name, i),
            });
        }
    }
}

// ============================================================================
// Cross-runbook reference validation
// ============================================================================

/// Errors from cross-reference validation.
#[derive(Debug, Clone, Error)]
pub enum CrossRefError {
    /// Invalid cross-reference format
    #[error("invalid cross-reference format: {reference}")]
    InvalidFormat { reference: String },

    /// Referenced kind doesn't match expected kind
    #[error("kind mismatch: expected {expected}, got {actual}")]
    KindMismatch { expected: String, actual: String },

    /// Referenced runbook not found
    #[error("runbook not found: {name}")]
    RunbookNotFound { name: String },

    /// Referenced item not found in runbook
    #[error("{kind} '{name}' not found in runbook '{runbook}'")]
    ItemNotFound {
        kind: String,
        name: String,
        runbook: String,
    },
}

/// Validate a runbook with access to registry for cross-reference checks.
///
/// This performs both local validation and cross-runbook reference validation.
pub fn validate_with_registry(raw: &RawRunbook, registry: &RunbookRegistry) -> ValidationResult {
    // First do internal validation
    let mut errors = match validate_runbook(raw) {
        Ok(_) => Vec::new(),
        Err(e) => e.errors,
    };

    // Then check cross-runbook references
    errors.extend(validate_cross_references(raw, registry));

    if errors.is_empty() {
        Ok(ValidatedRunbook { raw: raw.clone() })
    } else {
        Err(ValidationErrors { errors })
    }
}

/// Validate all cross-runbook references in a runbook.
pub fn validate_cross_references(
    runbook: &RawRunbook,
    registry: &RunbookRegistry,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check task references in pipelines
    for (pipeline_name, pipeline) in &runbook.pipeline {
        for phase in &pipeline.phase {
            if let Some(task_ref) = &phase.task {
                if is_cross_ref(task_ref) && validate_cross_ref(task_ref, "task", registry).is_err()
                {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "task",
                        name: task_ref.clone(),
                        referenced_in: format!(
                            "pipeline.{}.phase.{}.task",
                            pipeline_name, phase.name
                        ),
                    });
                }
            }
            if let Some(strategy_ref) = &phase.strategy {
                if is_cross_ref(strategy_ref)
                    && validate_cross_ref(strategy_ref, "strategy", registry).is_err()
                {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "strategy",
                        name: strategy_ref.clone(),
                        referenced_in: format!(
                            "pipeline.{}.phase.{}.strategy",
                            pipeline_name, phase.name
                        ),
                    });
                }
            }
            for (i, guard_ref) in phase.pre.iter().enumerate() {
                if is_cross_ref(guard_ref)
                    && validate_cross_ref(guard_ref, "guard", registry).is_err()
                {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "guard",
                        name: guard_ref.clone(),
                        referenced_in: format!(
                            "pipeline.{}.phase.{}.pre[{}]",
                            pipeline_name, phase.name, i
                        ),
                    });
                }
            }
            for (i, guard_ref) in phase.post.iter().enumerate() {
                if is_cross_ref(guard_ref)
                    && validate_cross_ref(guard_ref, "guard", registry).is_err()
                {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "guard",
                        name: guard_ref.clone(),
                        referenced_in: format!(
                            "pipeline.{}.phase.{}.post[{}]",
                            pipeline_name, phase.name, i
                        ),
                    });
                }
            }
        }
    }

    // Check task references in strategies
    for (strategy_name, strategy) in &runbook.strategy {
        for (i, attempt) in strategy.attempt.iter().enumerate() {
            if let Some(task_ref) = &attempt.task {
                if is_cross_ref(task_ref) && validate_cross_ref(task_ref, "task", registry).is_err()
                {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "task",
                        name: task_ref.clone(),
                        referenced_in: format!("strategy.{}.attempt[{}].task", strategy_name, i),
                    });
                }
            }
        }
    }

    // Check handler references in workers
    for (worker_name, worker) in &runbook.worker {
        if let Some(handler) = &worker.handler {
            if is_cross_ref(handler) {
                // Handler can be a pipeline or task - try both
                let is_valid_pipeline = validate_cross_ref(handler, "pipeline", registry).is_ok();
                let is_valid_task = validate_cross_ref(handler, "task", registry).is_ok();

                if !is_valid_pipeline && !is_valid_task {
                    errors.push(ValidationError::UndefinedReference {
                        kind: "pipeline/task",
                        name: handler.clone(),
                        referenced_in: format!("worker.{}.handler", worker_name),
                    });
                }
            }
        }
    }

    errors
}

/// Validate a cross-runbook reference.
fn validate_cross_ref(
    reference: &str,
    expected_kind: &str,
    registry: &RunbookRegistry,
) -> Result<(), CrossRefError> {
    // Not a cross-reference, skip validation here
    if !is_cross_ref(reference) {
        return Ok(());
    }

    let parts: Vec<&str> = reference.split('.').collect();
    if parts.len() != 3 {
        return Err(CrossRefError::InvalidFormat {
            reference: reference.into(),
        });
    }

    let (runbook_name, ref_kind, ref_name) = (parts[0], parts[1], parts[2]);

    // Check kind matches expected
    if ref_kind != expected_kind {
        return Err(CrossRefError::KindMismatch {
            expected: expected_kind.into(),
            actual: ref_kind.into(),
        });
    }

    // Get the referenced runbook
    let runbook = registry
        .get(runbook_name)
        .ok_or_else(|| CrossRefError::RunbookNotFound {
            name: runbook_name.into(),
        })?;

    // Check the referenced item exists
    match expected_kind {
        "task" => {
            if !runbook.tasks.contains_key(ref_name) {
                return Err(CrossRefError::ItemNotFound {
                    kind: "task".into(),
                    name: ref_name.into(),
                    runbook: runbook_name.into(),
                });
            }
        }
        "guard" => {
            if !runbook.guards.contains_key(ref_name) {
                return Err(CrossRefError::ItemNotFound {
                    kind: "guard".into(),
                    name: ref_name.into(),
                    runbook: runbook_name.into(),
                });
            }
        }
        "strategy" => {
            if !runbook.strategies.contains_key(ref_name) {
                return Err(CrossRefError::ItemNotFound {
                    kind: "strategy".into(),
                    name: ref_name.into(),
                    runbook: runbook_name.into(),
                });
            }
        }
        "pipeline" => {
            if !runbook.pipelines.contains_key(ref_name) {
                return Err(CrossRefError::ItemNotFound {
                    kind: "pipeline".into(),
                    name: ref_name.into(),
                    runbook: runbook_name.into(),
                });
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
#[path = "validator_tests.rs"]
mod tests;

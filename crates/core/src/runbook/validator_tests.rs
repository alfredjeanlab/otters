// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::runbook::parse_runbook;

// ============================================================================
// Valid runbooks
// ============================================================================

#[test]
fn validate_empty_runbook() {
    let runbook = parse_runbook("").unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_simple_valid_runbook() {
    let toml = r#"
[command.hello]
run = "echo hello"

[queue.work]
order = "created_at"

[worker.processor]
queue = "work"
handler = "pipeline.simple"

[pipeline.simple]
inputs = ["item"]

[[pipeline.simple.phase]]
name = "process"
run = "echo processing"
next = "done"

[[pipeline.simple.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_build_example_runbook() {
    let content = include_str!("../../../../docs/10-example-runbooks/build.toml");
    let runbook = parse_runbook(content).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_bugfix_example_runbook() {
    let content = include_str!("../../../../docs/10-example-runbooks/bugfix.toml");
    let runbook = parse_runbook(content).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

// ============================================================================
// Reference integrity: undefined references
// ============================================================================

#[test]
fn validate_undefined_queue_in_worker() {
    let toml = r#"
[worker.processor]
queue = "nonexistent"
handler = "task.foo"

[task.foo]
command = "echo foo"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "queue", name, .. } if name == "nonexistent"
    )));
}

#[test]
fn validate_undefined_handler_in_worker() {
    let toml = r#"
[queue.work]
order = "created_at"

[worker.processor]
queue = "work"
handler = "pipeline.nonexistent"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "pipeline", name, .. } if name == "nonexistent"
    )));
}

#[test]
fn validate_undefined_task_in_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
task = "nonexistent"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "task", name, .. } if name == "nonexistent"
    )));
}

#[test]
fn validate_undefined_strategy_in_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "merge"
strategy = "nonexistent"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "strategy", name, .. } if name == "nonexistent"
    )));
}

#[test]
fn validate_undefined_guard_in_pre() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
pre = ["nonexistent_guard"]
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "guard", name, .. } if name == "nonexistent_guard"
    )));
}

#[test]
fn validate_undefined_guard_in_post() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
post = ["missing_guard"]
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "guard", name, .. } if name == "missing_guard"
    )));
}

#[test]
fn validate_undefined_lock_in_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "merge"
run = "git merge"
lock = "nonexistent_lock"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "lock", name, .. } if name == "nonexistent_lock"
    )));
}

#[test]
fn validate_undefined_semaphore_in_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
task = "work_task"
semaphore = "nonexistent_sem"
next = "done"

[[pipeline.test.phase]]
name = "done"

[task.work_task]
command = "echo work"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "semaphore", name, .. } if name == "nonexistent_sem"
    )));
}

#[test]
fn validate_undefined_task_in_strategy_attempt() {
    let toml = r#"
[strategy.merge]

[[strategy.merge.attempt]]
name = "agent"
task = "nonexistent_task"
timeout = "5m"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "task", name, .. } if name == "nonexistent_task"
    )));
}

// ============================================================================
// Phase graph validity
// ============================================================================

#[test]
fn validate_invalid_next_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "nonexistent_phase"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::InvalidNextPhase { next, .. } if next == "nonexistent_phase"
    )));
}

#[test]
fn validate_next_to_done_is_valid() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_next_to_failed_is_valid() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "failed"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_custom_on_fail_action_is_valid() {
    // Custom action strings like "verify_and_retry" are allowed
    // because the executor may interpret them
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "done"
on_fail = "custom_action"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    // Custom action strings are now allowed
    assert!(result.is_ok());
}

#[test]
fn validate_on_fail_escalate_is_valid() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "done"
on_fail = "escalate"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_on_fail_to_phase_is_valid() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "done"
on_fail = "retry"

[[pipeline.test.phase]]
name = "retry"
run = "echo retry"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_on_fail_to_strategy_is_valid() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "done"
on_fail = "recovery"

[[pipeline.test.phase]]
name = "done"

[strategy.recovery]

[[strategy.recovery.attempt]]
name = "retry"
run = "echo retry"
timeout = "1m"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok());
}

#[test]
fn validate_unreachable_phase() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "start"
run = "echo start"
next = "done"

[[pipeline.test.phase]]
name = "orphan"
run = "echo orphan"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UnreachablePhase { phase, .. } if phase == "orphan"
    )));
}

#[test]
fn validate_phase_reachable_via_on_fail() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "start"
run = "echo start"
next = "done"
on_fail = "recovery"

[[pipeline.test.phase]]
name = "recovery"
run = "echo recovery"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    // "recovery" is reachable via on_fail
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

// ============================================================================
// Action validation
// ============================================================================

#[test]
fn validate_phase_no_action() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "empty"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::PhaseNoAction { phase, .. } if phase == "empty"
    )));
}

#[test]
fn validate_phase_multiple_actions() {
    let toml = r#"
[task.work]
command = "echo work"

[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "confused"
run = "echo run"
task = "work"
next = "done"

[[pipeline.test.phase]]
name = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::PhaseMultipleActions { phase, .. } if phase == "confused"
    )));
}

#[test]
fn validate_special_phases_can_have_no_action() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "init"
next = "work"

[[pipeline.test.phase]]
name = "work"
run = "echo work"
next = "blocked"
on_fail = "blocked"

[[pipeline.test.phase]]
name = "blocked"
pre = ["some_guard"]
next = "done"

[[pipeline.test.phase]]
name = "done"

[[pipeline.test.phase]]
name = "failed"

[guard.some_guard]
condition = "true"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    // init, blocked, done, failed can have no action
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_attempt_no_action() {
    let toml = r#"
[strategy.test]

[[strategy.test.attempt]]
name = "empty"
timeout = "1m"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::AttemptNoAction { attempt, .. } if attempt == "empty"
    )));
}

#[test]
fn validate_attempt_multiple_actions() {
    let toml = r#"
[task.work]
command = "echo work"

[strategy.test]

[[strategy.test.attempt]]
name = "confused"
run = "echo run"
task = "work"
timeout = "1m"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::AttemptMultipleActions { attempt, .. } if attempt == "confused"
    )));
}

// ============================================================================
// Multiple errors
// ============================================================================

#[test]
fn validate_collects_multiple_errors() {
    let toml = r#"
[pipeline.test]
inputs = []

[[pipeline.test.phase]]
name = "work"
task = "nonexistent1"
semaphore = "nonexistent2"
next = "nonexistent3"
on_fail = "nonexistent4"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    // Should have multiple errors
    assert!(errors.len() >= 3);
}

// ============================================================================
// Error display
// ============================================================================

#[test]
fn validation_errors_display() {
    let errors = ValidationErrors {
        errors: vec![
            ValidationError::UndefinedReference {
                kind: "task",
                name: "foo".to_string(),
                referenced_in: "pipeline.test".to_string(),
            },
            ValidationError::UnreachablePhase {
                pipeline: "test".to_string(),
                phase: "orphan".to_string(),
            },
        ],
    };

    let display = format!("{}", errors);
    assert!(display.contains("2 error(s)"));
    assert!(display.contains("Undefined task 'foo'"));
    assert!(display.contains("unreachable"));
}

// ============================================================================
// Scheduling primitive validation
// ============================================================================

#[test]
fn validate_undefined_action_in_watcher_response() {
    let toml = r#"
[watcher.test]
check_interval = "1m"

[watcher.test.source]
type = "session"
pattern = "*"

[watcher.test.condition]
type = "idle"
threshold = "5m"

[[watcher.test.response]]
action = "nonexistent"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "action", name, .. } if name == "nonexistent"
    )));
}

#[test]
fn validate_defined_action_in_watcher_response() {
    let toml = r#"
[action.nudge]
cooldown = "30s"
command = "echo nudge"

[watcher.test]
check_interval = "1m"

[watcher.test.source]
type = "session"
pattern = "*"

[watcher.test.condition]
type = "idle"
threshold = "5m"

[[watcher.test.response]]
action = "nudge"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_undefined_watcher_in_cron() {
    let toml = r#"
[cron.daily]
interval = "24h"
enabled = true
watchers = ["nonexistent_watcher"]
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "watcher", name, .. } if name == "nonexistent_watcher"
    )));
}

#[test]
fn validate_undefined_scanner_in_cron() {
    let toml = r#"
[cron.cleanup]
interval = "1h"
enabled = true
scanners = ["nonexistent_scanner"]
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);

    assert!(result.is_err());
    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "scanner", name, .. } if name == "nonexistent_scanner"
    )));
}

#[test]
fn validate_defined_watcher_and_scanner_in_cron() {
    let toml = r#"
[watcher.agent-idle]
check_interval = "1m"

[watcher.agent-idle.source]
type = "session"

[watcher.agent-idle.condition]
type = "idle"
threshold = "5m"

[scanner.stale-locks]
interval = "10m"

[scanner.stale-locks.source]
type = "locks"

[scanner.stale-locks.condition]
type = "stale"
threshold = "1h"

[scanner.stale-locks.cleanup]
type = "release"

[cron.maintenance]
interval = "1h"
enabled = true
watchers = ["agent-idle"]
scanners = ["stale-locks"]
"#;

    let runbook = parse_runbook(toml).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_watchdog_runbook() {
    let content = include_str!("../../../../runbooks/watchdog.toml");
    let runbook = parse_runbook(content).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_janitor_runbook() {
    let content = include_str!("../../../../runbooks/janitor.toml");
    let runbook = parse_runbook(content).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_triager_runbook() {
    let content = include_str!("../../../../runbooks/triager.toml");
    let runbook = parse_runbook(content).unwrap();
    let result = validate_runbook(&runbook);
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

// ============================================================================
// Cross-runbook reference validation
// ============================================================================

use crate::runbook::loader::{load_runbook, RunbookRegistry};
use crate::runbook::validate_with_registry;

#[test]
fn valid_cross_reference_passes() {
    let mut registry = RunbookRegistry::new();

    // Add a "common" runbook with a task
    let common_toml = r#"
        [task.shared-task]
        prompt = "Do something"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    // Add a runbook that references common.task.shared-task
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.task.shared-task"
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn missing_runbook_fails() {
    let registry = RunbookRegistry::new();

    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "nonexistent.task.foo"
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "task", name, .. }
        if name == "nonexistent.task.foo"
    )));
}

#[test]
fn missing_item_in_runbook_fails() {
    let mut registry = RunbookRegistry::new();

    // Add a "common" runbook without the referenced task
    let common_toml = r#"
        [task.other-task]
        prompt = "Different task"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    // Reference a task that doesn't exist
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.task.missing-task"
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { name, .. }
        if name == "common.task.missing-task"
    )));
}

#[test]
fn kind_mismatch_detected() {
    let mut registry = RunbookRegistry::new();

    // Add common with a guard (not a task)
    let common_toml = r#"
        [guard.check-exists]
        condition = "test -f file.txt"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    // Try to use it as a task (referencing common.guard.check-exists as a task)
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.guard.check-exists"
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    // Should fail because we're referencing a guard as a task
    assert!(result.is_err());
}

#[test]
fn local_references_not_affected() {
    let registry = RunbookRegistry::new();

    // Local reference (no dots indicating cross-ref)
    let main_toml = r#"
        [task.local-task]
        prompt = "Local task"

        [[pipeline.main.phase]]
        name = "do-work"
        task = "local-task"
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    // Should pass (local references validated separately)
    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_strategy_cross_refs() {
    let mut registry = RunbookRegistry::new();

    let common_toml = r#"
        [task.fallback]
        prompt = "Fallback approach"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    let main_toml = r#"
        [strategy.retry]
        checkpoint = "save_state"

        [[strategy.retry.attempt]]
        name = "fallback"
        task = "common.task.fallback"
        timeout = "5m"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_strategy_cross_ref_missing_fails() {
    let mut registry = RunbookRegistry::new();

    let common_toml = r#"
        [task.other]
        prompt = "Other task"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    let main_toml = r#"
        [strategy.retry]
        checkpoint = "save_state"

        [[strategy.retry.attempt]]
        name = "fallback"
        task = "common.task.nonexistent"
        timeout = "5m"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "task", name, .. }
        if name == "common.task.nonexistent"
    )));
}

#[test]
fn validate_guard_cross_refs() {
    let mut registry = RunbookRegistry::new();

    let common_toml = r#"
        [guard.check-ready]
        condition = "test -f ready.txt"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        run = "echo working"
        pre = ["common.guard.check-ready"]
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn validate_guard_cross_ref_missing_fails() {
    let registry = RunbookRegistry::new();

    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        run = "echo working"
        pre = ["common.guard.nonexistent"]
        next = "done"

        [[pipeline.main.phase]]
        name = "done"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    let errors = result.unwrap_err().errors;
    assert!(errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference { kind: "guard", name, .. }
        if name == "common.guard.nonexistent"
    )));
}

#[test]
fn validate_worker_handler_cross_ref() {
    let mut registry = RunbookRegistry::new();

    let common_toml = r#"
        [pipeline.shared]
        inputs = ["item"]

        [[pipeline.shared.phase]]
        name = "work"
        run = "echo work"
        next = "done"

        [[pipeline.shared.phase]]
        name = "done"
    "#;
    let common = parse_runbook(common_toml).unwrap();
    let common_validated = validate_runbook(&common).unwrap();
    registry.add("common", load_runbook(&common_validated).unwrap());

    let main_toml = r#"
        [queue.work]
        order = "priority"

        [worker.processor]
        queue = "work"
        handler = "common.pipeline.shared"
    "#;
    let main = parse_runbook(main_toml).unwrap();

    let result = validate_with_registry(&main, &registry);

    assert!(result.is_ok(), "Expected valid, got: {:?}", result);
}

#[test]
fn cross_reference_display_format() {
    let error = ValidationError::UndefinedReference {
        kind: "task",
        name: "common.task.missing".to_string(),
        referenced_in: "pipeline.main.phase.work".to_string(),
    };

    let display = format!("{}", error);
    assert!(display.contains("cross-runbook"));
    assert!(display.contains("common.task.missing"));
}

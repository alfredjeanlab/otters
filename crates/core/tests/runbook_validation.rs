// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Integration tests for runbook validation.
//!
//! Tests that runbook validation catches errors and produces helpful messages.

use oj_core::runbook::{load_runbook_file, parse_runbook, validate_runbook, ValidationError};
use std::path::Path;

// =============================================================================
// Example Runbook Validation
// =============================================================================

#[test]
fn all_example_runbooks_are_valid() {
    let runbooks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runbooks");
    if !runbooks_dir.exists() {
        // Skip test if runbooks directory doesn't exist (CI environment)
        return;
    }

    // Test build.toml
    let build_path = runbooks_dir.join("build.toml");
    if build_path.exists() {
        let result = load_runbook_file(&build_path);
        assert!(
            result.is_ok(),
            "build.toml validation failed: {:?}",
            result.err()
        );
    }

    // Test bugfix.toml
    let bugfix_path = runbooks_dir.join("bugfix.toml");
    if bugfix_path.exists() {
        let result = load_runbook_file(&bugfix_path);
        assert!(
            result.is_ok(),
            "bugfix.toml validation failed: {:?}",
            result.err()
        );
    }
}

// =============================================================================
// Reference Validation
// =============================================================================

#[test]
fn invalid_task_reference_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        task = "nonexistent_task"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference {
            kind: "task",
            name,
            ..
        } if name == "nonexistent_task"
    )));
}

#[test]
fn invalid_guard_reference_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        run = "echo hello"
        pre = ["nonexistent_guard"]
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference {
            kind: "guard",
            name,
            ..
        } if name == "nonexistent_guard"
    )));
}

#[test]
fn invalid_strategy_reference_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        strategy = "nonexistent_strategy"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference {
            kind: "strategy",
            name,
            ..
        } if name == "nonexistent_strategy"
    )));
}

#[test]
fn invalid_lock_reference_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        run = "echo test"
        lock = "nonexistent_lock"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference {
            kind: "lock",
            name,
            ..
        } if name == "nonexistent_lock"
    )));
}

#[test]
fn invalid_semaphore_reference_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        run = "echo test"
        semaphore = "nonexistent_semaphore"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::UndefinedReference {
            kind: "semaphore",
            name,
            ..
        } if name == "nonexistent_semaphore"
    )));
}

// =============================================================================
// Phase Validation
// =============================================================================

#[test]
fn invalid_next_phase_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "work"
        run = "echo test"
        next = "nonexistent_phase"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::InvalidNextPhase {
            next,
            ..
        } if next == "nonexistent_phase"
    )));
}

#[test]
fn phase_with_no_action_produces_error() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "empty_phase"
        next = "done"

        [[pipeline.test.phase]]
        name = "done"
        run = "echo done"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::PhaseNoAction {
            phase,
            ..
        } if phase == "empty_phase"
    )));
}

#[test]
fn phase_with_multiple_actions_produces_error() {
    let toml = r#"
        [task.test_task]
        command = "echo test"

        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "multi"
        run = "echo shell"
        task = "test_task"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::PhaseMultipleActions {
            phase,
            ..
        } if phase == "multi"
    )));
}

// =============================================================================
// Strategy Validation
// =============================================================================

#[test]
fn attempt_with_no_action_produces_error() {
    let toml = r#"
        [strategy.test]
        on_exhaust = "escalate"

        [[strategy.test.attempt]]
        name = "empty_attempt"
        timeout = "60s"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::AttemptNoAction {
            attempt,
            ..
        } if attempt == "empty_attempt"
    )));
}

#[test]
fn attempt_with_multiple_actions_produces_error() {
    let toml = r#"
        [strategy.test]
        on_exhaust = "escalate"

        [[strategy.test.attempt]]
        name = "multi"
        run = "echo shell"
        task = "some_task"
        timeout = "60s"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.errors.iter().any(|e| matches!(
        e,
        ValidationError::AttemptMultipleActions {
            attempt,
            ..
        } if attempt == "multi"
    )));
}

// =============================================================================
// Valid Runbook Tests
// =============================================================================

#[test]
fn valid_simple_runbook_passes_validation() {
    let toml = r#"
        [pipeline.test]
        inputs = ["name"]

        [[pipeline.test.phase]]
        name = "work"
        run = "echo {name}"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);
    assert!(result.is_ok());
}

#[test]
fn valid_runbook_with_all_primitives_passes_validation() {
    let toml = r#"
        [command.build]
        args = "<name>"
        run = "echo building {name}"

        [guard.ready]
        condition = "test -f ready.txt"

        [task.worker]
        command = "claude"

        [strategy.merge]
        checkpoint = "git rev-parse HEAD"
        on_exhaust = "escalate"

        [[strategy.merge.attempt]]
        name = "fast"
        run = "git merge --ff-only"
        timeout = "60s"

        [lock.main]
        timeout = "30m"

        [semaphore.agents]
        max = 4

        [pipeline.build]
        inputs = ["name"]

        [[pipeline.build.phase]]
        name = "init"
        run = "echo starting"
        next = "work"

        [[pipeline.build.phase]]
        name = "work"
        task = "worker"
        pre = ["ready"]
        lock = "main"
        semaphore = "agents"
        next = "merge"

        [[pipeline.build.phase]]
        name = "merge"
        strategy = "merge"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);
    assert!(result.is_ok(), "Validation failed: {:?}", result.err());
}

#[test]
fn multiple_errors_are_collected() {
    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "phase1"
        task = "missing_task"
        pre = ["missing_guard"]
        next = "missing_phase"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    // Should collect multiple errors
    assert!(
        errors.errors.len() >= 2,
        "Expected multiple errors, got: {:?}",
        errors
    );
}

// =============================================================================
// Error Message Quality
// =============================================================================

#[test]
fn error_messages_are_descriptive() {
    let toml = r#"
        [pipeline.my_pipeline]
        inputs = []

        [[pipeline.my_pipeline.phase]]
        name = "my_phase"
        task = "missing_task"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let result = validate_runbook(&raw);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    let error_text = format!("{}", errors);

    // Error should mention the pipeline name
    assert!(
        error_text.contains("my_pipeline") || error_text.contains("my_phase"),
        "Error should reference location: {}",
        error_text
    );
    // Error should mention the missing item
    assert!(
        error_text.contains("missing_task"),
        "Error should name missing item: {}",
        error_text
    );
}

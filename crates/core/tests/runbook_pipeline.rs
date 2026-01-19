// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Integration tests for runbook-based pipelines.
//!
//! Tests the full pipeline from TOML parsing through execution.

use oj_core::clock::FakeClock;
use oj_core::pipelines::{
    create_pipeline, get_dynamic_phase, is_dynamic_pipeline, set_dynamic_phase, ContextBuilder,
};
use oj_core::runbook::{load_runbook_file, parse_runbook, validate_runbook, TemplateEngine};
use std::collections::BTreeMap;
use std::path::Path;

// =============================================================================
// Pipeline Creation from Runbook
// =============================================================================

#[test]
fn build_pipeline_from_runbook_definition() {
    let clock = FakeClock::new();

    // Create a simple pipeline definition
    let toml = r#"
        [pipeline.test]
        inputs = ["name", "prompt"]

        [pipeline.test.defaults]
        workspace = ".worktrees/test-{name}"

        [[pipeline.test.phase]]
        name = "init"
        run = "echo starting {name}"
        next = "work"

        [[pipeline.test.phase]]
        name = "work"
        run = "echo working on {prompt}"
        next = "done"

        [[pipeline.test.phase]]
        name = "done"
        run = "echo completed"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = oj_core::runbook::load_runbook(&validated).unwrap();

    // Create pipeline from definition
    let def = runbook.pipelines.get("test").unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());
    inputs.insert("prompt".to_string(), "Add authentication".to_string());

    let pipeline = create_pipeline("pipe-1", def, inputs, &clock).unwrap();

    // Verify initial state
    assert_eq!(pipeline.id.0, "pipe-1");
    assert!(is_dynamic_pipeline(&pipeline));
    assert_eq!(get_dynamic_phase(&pipeline), Some("init"));
    assert_eq!(pipeline.inputs.get("name"), Some(&"auth".to_string()));
    assert_eq!(
        pipeline.inputs.get("workspace"),
        Some(&".worktrees/test-auth".to_string())
    );
}

#[test]
fn pipeline_requires_inputs() {
    let clock = FakeClock::new();

    let toml = r#"
        [pipeline.test]
        inputs = ["required_field"]

        [[pipeline.test.phase]]
        name = "init"
        run = "echo {required_field}"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = oj_core::runbook::load_runbook(&validated).unwrap();

    let def = runbook.pipelines.get("test").unwrap();

    // Should fail without required input
    let result = create_pipeline("pipe-1", def, BTreeMap::new(), &clock);
    assert!(result.is_err());

    // Should succeed with required input
    let mut inputs = BTreeMap::new();
    inputs.insert("required_field".to_string(), "value".to_string());
    let result = create_pipeline("pipe-1", def, inputs, &clock);
    assert!(result.is_ok());
}

#[test]
fn pipeline_uses_defaults_when_input_missing() {
    let clock = FakeClock::new();

    let toml = r#"
        [pipeline.test]
        inputs = ["name"]

        [pipeline.test.defaults]
        priority = "2"
        workspace = "workspaces/{name}"

        [[pipeline.test.phase]]
        name = "init"
        run = "echo {priority}"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = oj_core::runbook::load_runbook(&validated).unwrap();

    let def = runbook.pipelines.get("test").unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());

    let pipeline = create_pipeline("pipe-1", def, inputs, &clock).unwrap();

    // Default should be applied
    assert_eq!(pipeline.inputs.get("priority"), Some(&"2".to_string()));
    // Templated default should be rendered
    assert_eq!(
        pipeline.inputs.get("workspace"),
        Some(&"workspaces/auth".to_string())
    );
}

// =============================================================================
// Template Interpolation
// =============================================================================

#[test]
fn pipeline_template_interpolation() {
    let engine = TemplateEngine::new();

    // Build context with pipeline inputs
    let context = ContextBuilder::new()
        .with_string("name", "auth")
        .with_string("branch", "feature/auth")
        .with_string("count", "42")
        .build();

    // Test various templates
    let result = engine.render_simple("workspace-{name}", &context).unwrap();
    assert_eq!(result, "workspace-auth");

    let result = engine
        .render_simple("git checkout {branch}", &context)
        .unwrap();
    assert_eq!(result, "git checkout feature/auth");

    let result = engine.render_simple("Count is {count}", &context).unwrap();
    assert_eq!(result, "Count is 42");
}

#[test]
fn pipeline_jinja2_template_features() {
    let engine = TemplateEngine::new();
    let context = ContextBuilder::new()
        .with_string("name", "auth")
        .with_string("empty", "")
        .build();

    // Jinja2 style with double braces
    let result = engine.render("Name: {{ name }}", &context).unwrap();
    assert_eq!(result, "Name: auth");

    // Default filter
    let result = engine
        .render("Value: {{ missing | default('none') }}", &context)
        .unwrap();
    assert_eq!(result, "Value: none");
}

// =============================================================================
// Phase Tracking
// =============================================================================

#[test]
fn pipeline_phase_tracking() {
    let clock = FakeClock::new();

    let toml = r#"
        [pipeline.test]
        inputs = []

        [[pipeline.test.phase]]
        name = "first"
        run = "echo first"
        next = "second"

        [[pipeline.test.phase]]
        name = "second"
        run = "echo second"
        next = "third"

        [[pipeline.test.phase]]
        name = "third"
        run = "echo done"
    "#;

    let raw = parse_runbook(toml).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = oj_core::runbook::load_runbook(&validated).unwrap();

    let def = runbook.pipelines.get("test").unwrap();
    let mut pipeline = create_pipeline("pipe-1", def, BTreeMap::new(), &clock).unwrap();

    // Initial phase
    assert_eq!(get_dynamic_phase(&pipeline), Some("first"));

    // Advance phases
    set_dynamic_phase(&mut pipeline, "second");
    assert_eq!(get_dynamic_phase(&pipeline), Some("second"));

    set_dynamic_phase(&mut pipeline, "third");
    assert_eq!(get_dynamic_phase(&pipeline), Some("third"));
}

// =============================================================================
// Example Runbook Tests
// =============================================================================

#[test]
fn load_build_runbook_from_file() {
    // This test requires the runbooks directory to exist
    let runbook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runbooks/build.toml");
    if !runbook_path.exists() {
        // Skip test if runbook doesn't exist (CI environment)
        return;
    }

    let result = load_runbook_file(&runbook_path);
    assert!(result.is_ok(), "Failed to load build.toml: {:?}", result);

    let runbook = result.unwrap();
    assert!(runbook.pipelines.contains_key("build"));
    assert!(runbook.tasks.contains_key("planning"));
    assert!(runbook.strategies.contains_key("merge"));
}

#[test]
fn load_bugfix_runbook_from_file() {
    let runbook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runbooks/bugfix.toml");
    if !runbook_path.exists() {
        return;
    }

    let result = load_runbook_file(&runbook_path);
    assert!(result.is_ok(), "Failed to load bugfix.toml: {:?}", result);
}

// =============================================================================
// Context Builder
// =============================================================================

#[test]
fn context_builder_chains() {
    let engine = TemplateEngine::new();

    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());
    inputs.insert("issue".to_string(), "42".to_string());

    let mut defaults = BTreeMap::new();
    defaults.insert("branch".to_string(), "feature/{name}".to_string());

    let context = ContextBuilder::new()
        .with_inputs(&inputs)
        .with_defaults(&defaults, &engine)
        .unwrap()
        .with_string("extra", "value")
        .build();

    // Test rendered output
    let result = engine.render_simple("{branch}", &context).unwrap();
    assert_eq!(result, "feature/auth");

    let result = engine.render_simple("{extra}", &context).unwrap();
    assert_eq!(result, "value");
}

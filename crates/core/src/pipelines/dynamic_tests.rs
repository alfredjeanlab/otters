// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::runbook::{
    load_runbook, parse_runbook, validate_runbook, FailAction, PhaseAction, PhaseDef, PhaseNext,
    TaskDef,
};
use std::collections::BTreeMap;

// ============================================================================
// Context building
// ============================================================================

#[test]
fn context_builder_with_inputs() {
    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());
    inputs.insert("prompt".to_string(), "Build auth feature".to_string());

    let context = ContextBuilder::new().with_inputs(&inputs).build();

    let engine = TemplateEngine::new();
    let result = engine.render("{{ name }}: {{ prompt }}", &context).unwrap();
    assert_eq!(result, "auth: Build auth feature");
}

#[test]
fn context_builder_with_defaults() {
    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());

    let mut defaults = BTreeMap::new();
    defaults.insert("workspace".to_string(), ".worktrees/{name}".to_string());
    defaults.insert("branch".to_string(), "feature/{name}".to_string());

    let engine = TemplateEngine::new();
    let context = ContextBuilder::new()
        .with_inputs(&inputs)
        .with_defaults(&defaults, &engine)
        .unwrap()
        .build();

    let result = engine
        .render("{{ workspace }} on {{ branch }}", &context)
        .unwrap();
    assert_eq!(result, ".worktrees/auth on feature/auth");
}

#[test]
fn context_builder_with_string() {
    let context = ContextBuilder::new()
        .with_string("phase", "plan")
        .with_string("status", "running")
        .build();

    let engine = TemplateEngine::new();
    let result = engine
        .render("{{ phase }}: {{ status }}", &context)
        .unwrap();
    assert_eq!(result, "plan: running");
}

// ============================================================================
// Pipeline creation
// ============================================================================

#[test]
fn create_pipeline_basic() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude"

[pipeline.build]
inputs = ["name", "prompt"]

[pipeline.build.defaults]
workspace = ".worktrees/{name}"

[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get("build").unwrap();

    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());
    inputs.insert("prompt".to_string(), "Build auth".to_string());

    let clock = FakeClock::new();
    let pipeline = create_pipeline("test-1", def, inputs, &clock).unwrap();

    assert_eq!(pipeline.id.0, "test-1");
    assert_eq!(pipeline.inputs.get("name"), Some(&"auth".to_string()));
    assert_eq!(
        pipeline.inputs.get("prompt"),
        Some(&"Build auth".to_string())
    );
    // Defaults should be expanded
    assert_eq!(
        pipeline.inputs.get("workspace"),
        Some(&".worktrees/auth".to_string())
    );
    // Dynamic metadata
    assert!(is_dynamic_pipeline(&pipeline));
    assert_eq!(get_dynamic_phase(&pipeline), Some("init"));
}

#[test]
fn create_pipeline_missing_required_input() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude"

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get("build").unwrap();

    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());
    // Missing "prompt"

    let clock = FakeClock::new();
    let result = create_pipeline("test-1", def, inputs, &clock);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, DynamicError::MissingInput { name } if name == "prompt"));
}

#[test]
fn create_pipeline_with_default_fills_missing() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude"

[pipeline.build]
inputs = ["name"]

[pipeline.build.defaults]
prompt = "Default prompt for {name}"

[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get("build").unwrap();

    let mut inputs = BTreeMap::new();
    inputs.insert("name".to_string(), "auth".to_string());

    let clock = FakeClock::new();
    let pipeline = create_pipeline("test-1", def, inputs, &clock).unwrap();

    assert_eq!(
        pipeline.inputs.get("prompt"),
        Some(&"Default prompt for auth".to_string())
    );
}

// ============================================================================
// Phase conversion
// ============================================================================

#[test]
fn phase_to_config_with_run() {
    let phase_def = PhaseDef {
        name: "init".to_string(),
        action: PhaseAction::Run {
            command: "echo {name}".to_string(),
        },
        pre_guards: vec![],
        post_guards: vec![],
        lock: None,
        semaphore: None,
        next: PhaseNext::Done,
        on_fail: FailAction::Escalate,
    };

    let engine = TemplateEngine::new();
    let context = ContextBuilder::new().with_string("name", "auth").build();

    let config = phase_to_config(&phase_def, None, &engine, &context).unwrap();

    assert_eq!(config.name, "init");
    assert_eq!(config.run, Some(vec!["echo auth".to_string()]));
    assert!(config.task.is_none());
    assert!(config.strategy.is_none());
    assert!(matches!(config.next, DynamicPhaseNext::Done));
}

#[test]
fn phase_to_config_with_task() {
    let phase_def = PhaseDef {
        name: "plan".to_string(),
        action: PhaseAction::Task {
            name: "planning".to_string(),
        },
        pre_guards: vec![],
        post_guards: vec!["plan_exists".to_string()],
        lock: None,
        semaphore: Some("claude".to_string()),
        next: PhaseNext::Phase("build".to_string()),
        on_fail: FailAction::Escalate,
    };

    let task_def = TaskDef {
        name: "planning".to_string(),
        command: Some("claude --print".to_string()),
        prompt: Some("Create a plan for {{ name }}".to_string()),
        prompt_file: Some("prompts/plan.md".to_string()),
        env: BTreeMap::new(),
        cwd: None,
        heartbeat: None,
        timeout: Some(Duration::from_secs(900)),
        idle_timeout: Some(Duration::from_secs(120)),
        on_stuck: vec![],
        on_timeout: None,
        checkpoint_interval: None,
        checkpoint: None,
    };

    let engine = TemplateEngine::new();
    let context = ContextBuilder::new().with_string("name", "auth").build();

    let config = phase_to_config(&phase_def, Some(&task_def), &engine, &context).unwrap();

    assert_eq!(config.name, "plan");
    assert!(config.run.is_none());

    let task = config.task.unwrap();
    assert_eq!(task.command, "claude --print");
    assert_eq!(task.prompt, Some("Create a plan for auth".to_string()));
    assert_eq!(task.prompt_file, Some("prompts/plan.md".to_string()));
    assert_eq!(task.timeout, Duration::from_secs(900));
    assert_eq!(task.idle_timeout, Duration::from_secs(120));

    assert_eq!(config.post_guards, vec!["plan_exists"]);
    assert_eq!(config.semaphore, Some("claude".to_string()));
    assert!(matches!(config.next, DynamicPhaseNext::Phase(p) if p == "build"));
}

#[test]
fn phase_to_config_with_strategy() {
    let phase_def = PhaseDef {
        name: "merge".to_string(),
        action: PhaseAction::Strategy {
            name: "merge_retry".to_string(),
        },
        pre_guards: vec![],
        post_guards: vec![],
        lock: Some("main_branch".to_string()),
        semaphore: None,
        next: PhaseNext::Done,
        on_fail: FailAction::GotoPhase("cleanup".to_string()),
    };

    let engine = TemplateEngine::new();
    let context = Context::new();

    let config = phase_to_config(&phase_def, None, &engine, &context).unwrap();

    assert_eq!(config.name, "merge");
    assert!(config.run.is_none());
    assert!(config.task.is_none());
    assert_eq!(config.strategy, Some("merge_retry".to_string()));
    assert_eq!(config.lock, Some("main_branch".to_string()));
    assert!(matches!(config.on_fail, DynamicFailAction::GotoPhase(p) if p == "cleanup"));
}

#[test]
fn phase_to_config_with_guards() {
    let phase_def = PhaseDef {
        name: "build".to_string(),
        action: PhaseAction::None,
        pre_guards: vec!["ready".to_string(), "resources_available".to_string()],
        post_guards: vec!["build_complete".to_string()],
        lock: None,
        semaphore: None,
        next: PhaseNext::Done,
        on_fail: FailAction::Escalate,
    };

    let engine = TemplateEngine::new();
    let context = Context::new();

    let config = phase_to_config(&phase_def, None, &engine, &context).unwrap();

    assert_eq!(config.pre_guards, vec!["ready", "resources_available"]);
    assert_eq!(config.post_guards, vec!["build_complete"]);
}

// ============================================================================
// Fail action conversion
// ============================================================================

#[test]
fn fail_action_conversion() {
    assert!(matches!(
        DynamicFailAction::from(&FailAction::Escalate),
        DynamicFailAction::Escalate
    ));

    assert!(matches!(
        DynamicFailAction::from(&FailAction::GotoPhase("cleanup".to_string())),
        DynamicFailAction::GotoPhase(p) if p == "cleanup"
    ));

    assert!(matches!(
        DynamicFailAction::from(&FailAction::UseStrategy("retry".to_string())),
        DynamicFailAction::UseStrategy(s) if s == "retry"
    ));

    let retry = DynamicFailAction::from(&FailAction::Retry {
        max: 3,
        interval: Duration::from_secs(60),
    });
    if let DynamicFailAction::Retry { max, interval } = retry {
        assert_eq!(max, 3);
        assert_eq!(interval, Duration::from_secs(60));
    } else {
        panic!("Expected Retry");
    }
}

// ============================================================================
// Dynamic pipeline helpers
// ============================================================================

#[test]
fn dynamic_pipeline_detection() {
    let clock = FakeClock::new();

    // Plain pipeline (from new_dynamic, no runbook metadata)
    let plain_pipeline = Pipeline::new_dynamic("plain-1", "test", BTreeMap::new());
    assert!(!is_dynamic_pipeline(&plain_pipeline));
    assert_eq!(get_dynamic_phase(&plain_pipeline), None);

    // Dynamic pipeline created from runbook has metadata
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude"

[pipeline.build]
[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get("build").unwrap();

    let dynamic_pipeline = create_pipeline("dynamic-1", def, BTreeMap::new(), &clock).unwrap();
    assert!(is_dynamic_pipeline(&dynamic_pipeline));
    assert_eq!(get_dynamic_phase(&dynamic_pipeline), Some("init"));
}

#[test]
fn set_and_get_dynamic_phase() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude"

[pipeline.build]
[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get("build").unwrap();

    let clock = FakeClock::new();
    let mut pipeline = create_pipeline("test-1", def, BTreeMap::new(), &clock).unwrap();

    assert_eq!(get_dynamic_phase(&pipeline), Some("init"));

    set_dynamic_phase(&mut pipeline, "done");
    assert_eq!(get_dynamic_phase(&pipeline), Some("done"));
}

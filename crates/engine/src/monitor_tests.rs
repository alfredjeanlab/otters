// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Tests for monitor::build_action_effects()

use super::*;
use oj_core::{PhaseStatus, Pipeline};
use oj_runbook::{ActionConfig, AgentAction, AgentDef};
use std::collections::HashMap;
use std::time::Instant;

fn test_pipeline() -> Pipeline {
    Pipeline {
        id: "test-1".to_string(),
        name: "test-feature".to_string(),
        kind: "build".to_string(),
        phase: "execute".to_string(),
        phase_status: PhaseStatus::Running,
        session_id: Some("sess-1".to_string()),
        workspace_path: Some("/tmp/test".into()),
        inputs: HashMap::new(),
        created_at: Instant::now(),
        phase_started_at: Instant::now(),
        error: None,
    }
}

fn test_agent_def() -> AgentDef {
    AgentDef {
        name: "worker".to_string(),
        run: "claude".to_string(),
        prompt: Some("Do the task.".to_string()),
        ..Default::default()
    }
}

#[test]
fn nudge_builds_send_effect() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Nudge);

    let result = build_action_effects(&pipeline, &agent, &config, "idle", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::Nudge { .. })));
}

#[test]
fn done_returns_advance_pipeline() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Done);

    let result = build_action_effects(&pipeline, &agent, &config, "idle", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::AdvancePipeline)));
}

#[test]
fn fail_returns_fail_pipeline() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Fail);

    let result = build_action_effects(&pipeline, &agent, &config, "error", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::FailPipeline { .. })));
}

#[test]
fn restart_returns_restart_effects() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Restart);

    let result = build_action_effects(&pipeline, &agent, &config, "exit", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::Restart { .. })));
}

#[test]
fn recover_returns_recover_effects() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Recover);

    let result = build_action_effects(&pipeline, &agent, &config, "exit", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::Recover { .. })));
}

#[test]
fn recover_with_message_replaces_prompt() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::with_message(AgentAction::Recover, "New prompt.");
    let inputs = [("prompt".to_string(), "Original".to_string())]
        .into_iter()
        .collect();

    let result = build_action_effects(&pipeline, &agent, &config, "exit", &inputs).unwrap();
    if let ActionEffects::Recover { inputs, .. } = result {
        assert_eq!(inputs.get("prompt"), Some(&"New prompt.".to_string()));
    } else {
        panic!("Expected Recover");
    }
}

#[test]
fn recover_with_append_appends_to_prompt() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::with_append(AgentAction::Recover, "Try again.");
    let inputs = [("prompt".to_string(), "Original".to_string())]
        .into_iter()
        .collect();

    let result = build_action_effects(&pipeline, &agent, &config, "exit", &inputs).unwrap();
    if let ActionEffects::Recover { inputs, .. } = result {
        let prompt = inputs.get("prompt").unwrap();
        assert!(prompt.contains("Original"));
        assert!(prompt.contains("Try again."));
    } else {
        panic!("Expected Recover");
    }
}

#[test]
fn escalate_returns_escalate_effects() {
    let pipeline = test_pipeline();
    let agent = test_agent_def();
    let config = ActionConfig::simple(AgentAction::Escalate);

    let result = build_action_effects(&pipeline, &agent, &config, "idle", &HashMap::new());
    assert!(matches!(result, Ok(ActionEffects::Escalate { .. })));
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

fn sample_pipeline() -> PipelineDef {
    PipelineDef {
        name: "build".to_string(),
        inputs: vec!["name".to_string(), "prompt".to_string()],
        defaults: HashMap::new(),
        phases: vec![
            PhaseDef {
                name: "init".to_string(),
                run: RunDirective::Shell("git worktree add".to_string()),
                next: None,
                on_fail: None,
            },
            PhaseDef {
                name: "plan".to_string(),
                run: RunDirective::Agent {
                    agent: "planner".to_string(),
                },
                next: None,
                on_fail: None,
            },
            PhaseDef {
                name: "execute".to_string(),
                run: RunDirective::Agent {
                    agent: "executor".to_string(),
                },
                next: Some("done".to_string()),
                on_fail: Some("failed".to_string()),
            },
            PhaseDef {
                name: "done".to_string(),
                run: RunDirective::Shell("echo done".to_string()),
                next: None,
                on_fail: None,
            },
            PhaseDef {
                name: "failed".to_string(),
                run: RunDirective::Shell("echo failed".to_string()),
                next: None,
                on_fail: None,
            },
        ],
    }
}

#[test]
fn pipeline_phase_lookup() {
    let p = sample_pipeline();
    assert!(p.get_phase("init").is_some());
    assert!(p.get_phase("nonexistent").is_none());
}

#[test]
fn pipeline_next_phase_default() {
    let p = sample_pipeline();
    let next = p.next_phase("init");
    assert_eq!(next.map(|p| &p.name), Some(&"plan".to_string()));
}

#[test]
fn pipeline_next_phase_explicit() {
    let p = sample_pipeline();
    let next = p.next_phase("execute");
    assert_eq!(next.map(|p| &p.name), Some(&"done".to_string()));
}

#[test]
fn phase_is_shell() {
    let p = sample_pipeline();
    assert!(p.get_phase("init").unwrap().is_shell());
    assert!(!p.get_phase("plan").unwrap().is_shell());
}

#[test]
fn phase_is_agent() {
    let p = sample_pipeline();
    assert!(!p.get_phase("init").unwrap().is_agent());
    assert!(p.get_phase("plan").unwrap().is_agent());
    assert_eq!(p.get_phase("plan").unwrap().agent_name(), Some("planner"));
}

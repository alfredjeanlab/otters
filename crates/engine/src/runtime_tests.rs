// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Runtime tests

use super::*;
use crate::{RuntimeConfig, RuntimeDeps};
use oj_adapters::{FakeNotifyAdapter, FakeRepoAdapter, FakeSessionAdapter};
use oj_core::{FakeClock, SequentialIdGen};
use oj_runbook::parse_runbook;
use tempfile::tempdir;

const TEST_RUNBOOK: &str = r#"
[command.build]
args = "<name> <prompt>"
run = { pipeline = "build" }

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "init"
run = "echo init"

[[pipeline.build.phase]]
name = "plan"
run = { agent = "planner" }

[[pipeline.build.phase]]
name = "execute"
run = { agent = "executor" }

[[pipeline.build.phase]]
name = "merge"
run = "echo merge"
on_fail = "cleanup"

[[pipeline.build.phase]]
name = "done"
run = "echo done"

[[pipeline.build.phase]]
name = "cleanup"
run = "echo cleanup"

[agent.planner]
run = "claude -p \"{prompt}\""
[agent.planner.env]
OJ_PIPELINE = "{pipeline_id}"

[agent.executor]
run = "claude --execute"
[agent.executor.env]
OJ_PIPELINE = "{pipeline_id}"
"#;

async fn setup(
) -> Runtime<FakeSessionAdapter, FakeRepoAdapter, FakeNotifyAdapter, FakeClock, SequentialIdGen> {
    let dir = tempdir().unwrap();
    // Keep the temp directory alive by leaking it
    let dir_path = dir.keep();
    let wal = Wal::open(&dir_path.join("test.wal")).unwrap();
    let runbook = parse_runbook(TEST_RUNBOOK).unwrap();

    // Create worktrees directory and the workspace for our test pipeline
    let worktrees = dir_path.join("worktrees");
    std::fs::create_dir_all(&worktrees).unwrap();
    std::fs::create_dir_all(worktrees.join("test-feature")).unwrap();

    Runtime::new(
        RuntimeDeps {
            sessions: FakeSessionAdapter::new(),
            repos: FakeRepoAdapter::new(),
            notify: FakeNotifyAdapter::new(),
            wal: Arc::new(Mutex::new(wal)),
            state: Arc::new(Mutex::new(MaterializedState::default())),
        },
        runbook,
        FakeClock::new(),
        SequentialIdGen::new("pipe"),
        RuntimeConfig {
            project_root: dir_path.clone(),
            worktree_root: worktrees,
        },
    )
}

async fn create_pipeline(
    runtime: &Runtime<
        FakeSessionAdapter,
        FakeRepoAdapter,
        FakeNotifyAdapter,
        FakeClock,
        SequentialIdGen,
    >,
) -> String {
    let args: HashMap<String, String> = [
        ("name".to_string(), "test-feature".to_string()),
        ("prompt".to_string(), "Add login".to_string()),
    ]
    .into_iter()
    .collect();

    runtime
        .handle_event(Event::CommandInvoked {
            command: "build".to_string(),
            args,
        })
        .await
        .unwrap();

    let pipelines = runtime.pipelines();
    pipelines.keys().next().unwrap().clone()
}

#[tokio::test]
async fn runtime_handle_command() {
    let runtime = setup().await;
    let _pipeline_id = create_pipeline(&runtime).await;

    let pipelines = runtime.pipelines();
    assert_eq!(pipelines.len(), 1);

    let pipeline = pipelines.values().next().unwrap();
    assert_eq!(pipeline.name, "test-feature");
    assert_eq!(pipeline.kind, "build");
}

#[tokio::test]
async fn shell_completion_advances_phase() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Pipeline starts at init phase (shell)
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "init");

    // Simulate shell completion
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "plan");
}

#[tokio::test]
async fn agent_done_advances_phase() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Advance to plan phase (agent)
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "plan");

    // Simulate agent completion
    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "execute");
}

#[tokio::test]
async fn shell_failure_fails_pipeline() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Simulate shell failure
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 1,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "failed");
}

#[tokio::test]
async fn agent_error_fails_pipeline() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Advance to plan phase
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    // Simulate agent error
    runtime
        .handle_event(Event::AgentError {
            pipeline_id: pipeline_id.clone(),
            error: "timeout".to_string(),
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "failed");
}

#[tokio::test]
async fn on_fail_transition_executes() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Advance to merge phase (which has on_fail = "cleanup")
    // init -> plan -> execute -> merge
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "merge");

    // Simulate merge failure - should transition to cleanup (custom phase)
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "merge".to_string(),
            exit_code: 1,
        })
        .await
        .unwrap();

    // With string-based phases, custom phases like "cleanup" now work correctly
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(
        pipeline.phase, "cleanup",
        "Expected cleanup phase, got {}",
        pipeline.phase
    );
}

#[tokio::test]
async fn final_phase_completes_pipeline() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Advance through all phases to done
    // init -> plan -> execute -> merge -> done
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "merge".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "done");
}

#[tokio::test]
async fn done_phase_run_command_executes() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Advance through all phases: init -> plan -> execute -> merge -> done
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::AgentDone {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .unwrap();

    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "merge".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    // At this point, pipeline should be in Done phase with Running status
    // (because the "done" phase's run command is executing)
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "done");
    assert_eq!(pipeline.phase_status, PhaseStatus::Running);

    // Complete the "done" phase's shell command
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "done".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    // Now pipeline should be Done with Completed status
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "done");
    assert_eq!(pipeline.phase_status, PhaseStatus::Completed);
}

#[tokio::test]
async fn wrong_phase_shell_completed_ignored() {
    let runtime = setup().await;
    let pipeline_id = create_pipeline(&runtime).await;

    // Try to complete a phase we're not in
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "merge".to_string(), // We're in init, not merge
            exit_code: 0,
        })
        .await
        .unwrap();

    // Should still be in init
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "init");
}

/// Runbook without explicit "done" phase - pipeline should still reach Done
const RUNBOOK_NO_DONE_PHASE: &str = r#"
[command.simple]
args = "<name>"
run = { pipeline = "simple" }

[pipeline.simple]
inputs = ["name"]

[[pipeline.simple.phase]]
name = "init"
run = "echo init"

[[pipeline.simple.phase]]
name = "execute"
run = "echo execute"

[[pipeline.simple.phase]]
name = "merge"
run = "echo merge"
"#;

async fn setup_no_done_phase(
) -> Runtime<FakeSessionAdapter, FakeRepoAdapter, FakeNotifyAdapter, FakeClock, SequentialIdGen> {
    let dir = tempdir().unwrap();
    let dir_path = dir.keep();
    let wal = Wal::open(&dir_path.join("test.wal")).unwrap();
    let runbook = parse_runbook(RUNBOOK_NO_DONE_PHASE).unwrap();

    let worktrees = dir_path.join("worktrees");
    std::fs::create_dir_all(&worktrees).unwrap();
    std::fs::create_dir_all(worktrees.join("test")).unwrap();

    Runtime::new(
        RuntimeDeps {
            sessions: FakeSessionAdapter::new(),
            repos: FakeRepoAdapter::new(),
            notify: FakeNotifyAdapter::new(),
            wal: Arc::new(Mutex::new(wal)),
            state: Arc::new(Mutex::new(MaterializedState::default())),
        },
        runbook,
        FakeClock::new(),
        SequentialIdGen::new("pipe"),
        RuntimeConfig {
            project_root: dir_path.clone(),
            worktree_root: worktrees,
        },
    )
}

#[tokio::test]
async fn pipeline_completes_without_explicit_done_phase() {
    let runtime = setup_no_done_phase().await;

    // Create pipeline
    let args: HashMap<String, String> = [("name".to_string(), "test".to_string())]
        .into_iter()
        .collect();

    runtime
        .handle_event(Event::CommandInvoked {
            command: "simple".to_string(),
            args,
        })
        .await
        .unwrap();

    let pipeline_id = runtime.pipelines().keys().next().unwrap().clone();

    // Advance through all phases: init -> execute -> merge
    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "init".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "execute");

    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "execute".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "merge");

    runtime
        .handle_event(Event::ShellCompleted {
            pipeline_id: pipeline_id.clone(),
            phase: "merge".to_string(),
            exit_code: 0,
        })
        .await
        .unwrap();

    // Pipeline should transition to Done even without explicit done phase in runbook
    let pipeline = runtime.get_pipeline(&pipeline_id).unwrap();
    assert_eq!(pipeline.phase, "done");
}

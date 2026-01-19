// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Integration tests for the Engine
//!
//! Tests full engine lifecycle, recovery chains, and scheduler behavior.

use oj_core::adapters::FakeAdapters;
use oj_core::clock::FakeClock;
use oj_core::engine::{Engine, ScheduledKind, Scheduler};
use oj_core::pipeline::{Pipeline, PipelineEvent, PipelineId};
use oj_core::storage::WalStore;
use oj_core::task::{Task, TaskEvent, TaskId, TaskState};
use oj_core::workspace::{Workspace, WorkspaceId};
use oj_core::{Clock, SessionAdapter};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

fn make_engine() -> (Engine<FakeAdapters, FakeClock>, FakeAdapters, FakeClock) {
    let adapters = FakeAdapters::new();
    let store = WalStore::open_temp().unwrap();
    let clock = FakeClock::new();
    let engine = Engine::new(adapters.clone(), store, clock.clone());
    (engine, adapters, clock)
}

fn make_test_pipeline(id: &str, name: &str) -> Pipeline {
    Pipeline::new_dynamic(id, name, BTreeMap::new())
}

// =============================================================================
// Engine Lifecycle Tests
// =============================================================================

#[tokio::test]
async fn engine_full_pipeline_lifecycle() {
    let (mut engine, _adapters, _clock) = make_engine();

    // Create workspace
    let workspace = Workspace::new_ready(
        "ws-test",
        "test-workspace",
        PathBuf::from("/tmp/test-workspace"),
        "feature/test",
    );
    engine.add_workspace(workspace).unwrap();

    // Create pipeline with workspace
    let pipeline = make_test_pipeline("pipe-1", "test-pipeline")
        .with_workspace(WorkspaceId("ws-test".to_string()));
    engine.add_pipeline(pipeline).unwrap();

    // Verify initial state
    let pipeline = engine
        .get_pipeline(&PipelineId("pipe-1".to_string()))
        .unwrap();
    assert_eq!(pipeline.phase.name(), "init");

    // Transition through phases
    engine
        .process_pipeline_event(
            &PipelineId("pipe-1".to_string()),
            PipelineEvent::PhaseComplete,
        )
        .await
        .unwrap();

    let pipeline = engine
        .get_pipeline(&PipelineId("pipe-1".to_string()))
        .unwrap();
    // Dynamic pipelines go straight to done
    assert_eq!(pipeline.phase.name(), "done");
}

#[tokio::test]
async fn engine_task_completion_cascades_to_pipeline() {
    let (mut engine, _adapters, clock) = make_engine();

    // Setup workspace and pipeline
    let workspace =
        Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature/test");
    engine.add_workspace(workspace).unwrap();

    let pipeline =
        make_test_pipeline("pipe-1", "test").with_workspace(WorkspaceId("ws-1".to_string()));
    engine.add_pipeline(pipeline).unwrap();

    // Create and assign a task
    let task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    engine.add_task(task).unwrap();

    // Start the task
    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Start {
                session_id: oj_core::session::SessionId("sess-1".to_string()),
            },
        )
        .await
        .unwrap();

    // Complete the task
    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Complete {
                output: Some("done".to_string()),
            },
        )
        .await
        .unwrap();

    // Verify task is completed
    let task = engine.get_task(&TaskId("task-1".to_string())).unwrap();
    assert!(task.is_terminal());
}

#[tokio::test]
async fn engine_handles_session_dead_event() {
    let (mut engine, _adapters, clock) = make_engine();

    // Setup
    let workspace =
        Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature/test");
    engine.add_workspace(workspace).unwrap();

    // Create pipeline
    let pipeline = make_test_pipeline("pipe-1", "test");
    engine.add_pipeline(pipeline).unwrap();

    // Create a running task
    let task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    engine.add_task(task).unwrap();

    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Start {
                session_id: oj_core::session::SessionId("sess-1".to_string()),
            },
        )
        .await
        .unwrap();

    // Simulate session death via process_event
    engine
        .process_event(oj_core::effect::Event::SessionDead {
            id: "sess-1".to_string(),
            reason: "terminated".to_string(),
        })
        .await
        .unwrap();

    // Task should be failed
    let task = engine.get_task(&TaskId("task-1".to_string())).unwrap();
    assert!(matches!(task.state, TaskState::Failed { .. }));
}

// =============================================================================
// Recovery Chain Tests
// =============================================================================

#[tokio::test]
async fn engine_stuck_task_triggers_nudge() {
    let (mut engine, adapters, clock) = make_engine();

    // Setup workspace and pipeline
    let workspace =
        Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature/test");
    engine.add_workspace(workspace).unwrap();

    let pipeline =
        make_test_pipeline("pipe-1", "test").with_workspace(WorkspaceId("ws-1".to_string()));
    engine.add_pipeline(pipeline).unwrap();

    // Create and start a task
    let task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    engine.add_task(task).unwrap();

    // Create a session for the task
    adapters
        .sessions()
        .spawn("sess-1", std::path::Path::new("/tmp"), "claude")
        .await
        .unwrap();

    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Start {
                session_id: oj_core::session::SessionId("sess-1".to_string()),
            },
        )
        .await
        .unwrap();

    // Advance time past stuck threshold
    clock.advance(Duration::from_secs(150));

    // Tick the task to detect stuck state
    // Pass session_idle_time to trigger stuck detection
    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Tick {
                session_idle_time: Some(Duration::from_secs(150)),
            },
        )
        .await
        .unwrap();

    // Task should be stuck
    let task = engine.get_task(&TaskId("task-1".to_string())).unwrap();
    assert!(task.is_stuck());

    // Handle stuck task - should nudge
    engine
        .handle_stuck_task(&TaskId("task-1".to_string()))
        .await
        .unwrap();

    // Verify a send was called (the nudge message)
    let calls = adapters.calls();
    let has_send = calls.iter().any(|c| {
        matches!(
            c,
            oj_core::adapters::fake::AdapterCall::SendToSession { .. }
        )
    });
    assert!(has_send, "Expected send call for nudge");
}

#[tokio::test]
async fn recovery_escalates_after_exhausted_retries() {
    use oj_core::engine::{RecoveryAction, RecoveryConfig, RecoveryState};

    let clock = FakeClock::new();
    let config = RecoveryConfig {
        max_nudges: 2,
        nudge_cooldown: Duration::from_secs(10),
        max_restarts: 1,
        restart_cooldown: Duration::from_secs(30),
        nudge_message: "test".to_string(),
    };

    // Create a stuck task
    let mut task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    task.state = TaskState::Stuck {
        since: clock.now(),
        nudge_count: 0,
    };

    let mut state = RecoveryState::default();

    // Exhaust nudges
    for _ in 0..config.max_nudges {
        let action = state.next_action(&task, &config, clock.now());
        assert_eq!(action, RecoveryAction::Nudge);
        state.record_nudge(clock.now());
        clock.advance(config.nudge_cooldown + Duration::from_secs(1));
    }

    // Should restart now
    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Restart);
    state.record_restart(clock.now());
    clock.advance(config.restart_cooldown + Duration::from_secs(1));

    // After restart, nudges reset, exhaust them again
    for _ in 0..config.max_nudges {
        let action = state.next_action(&task, &config, clock.now());
        assert_eq!(action, RecoveryAction::Nudge);
        state.record_nudge(clock.now());
        clock.advance(config.nudge_cooldown + Duration::from_secs(1));
    }

    // Should escalate now (restarts exhausted)
    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Escalate);
}

// =============================================================================
// Scheduler Tests
// =============================================================================

#[test]
fn scheduler_fires_repeating_timers() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    scheduler.schedule_repeating(
        "tick",
        clock.now() + Duration::from_secs(10),
        Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );

    // Advance and poll multiple times
    for i in 1..=5 {
        clock.advance(Duration::from_secs(10));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1, "iteration {}", i);
        assert_eq!(ready[0].id, "tick");
    }
}

#[test]
fn scheduler_cancellation_works() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    scheduler.schedule(
        "cancel-me",
        clock.now() + Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );

    scheduler.cancel("cancel-me");

    clock.advance(Duration::from_secs(15));
    let ready = scheduler.poll(clock.now());
    assert!(ready.is_empty());
}

#[test]
fn scheduler_init_defaults_creates_standard_timers() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    scheduler.init_defaults(&clock);

    // Advance past all default timers
    clock.advance(Duration::from_secs(30));
    let ready = scheduler.poll(clock.now());

    // Should have multiple timers fire
    assert!(
        ready.len() >= 2,
        "Expected at least 2 timers, got {}",
        ready.len()
    );
}

// =============================================================================
// Signal Handling Tests
// =============================================================================

#[tokio::test]
async fn engine_signal_done_completes_task() {
    let (mut engine, _adapters, clock) = make_engine();

    // Setup workspace
    let workspace =
        Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature/test");
    engine.add_workspace(workspace).unwrap();

    // Create pipeline with workspace
    let pipeline = make_test_pipeline("pipe-1", "test")
        .with_workspace(WorkspaceId("ws-1".to_string()))
        .with_current_task(TaskId("task-1".to_string()));
    engine.add_pipeline(pipeline).unwrap();

    // Create and start task
    let task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    engine.add_task(task).unwrap();

    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Start {
                session_id: oj_core::session::SessionId("sess-1".to_string()),
            },
        )
        .await
        .unwrap();

    // Signal done
    engine
        .signal_done(&WorkspaceId("ws-1".to_string()), None)
        .await
        .unwrap();

    // Task should be completed
    let task = engine.get_task(&TaskId("task-1".to_string())).unwrap();
    assert!(task.is_terminal());
    assert!(matches!(task.state, TaskState::Done { .. }));
}

#[tokio::test]
async fn engine_signal_done_with_error_fails_task() {
    let (mut engine, _adapters, clock) = make_engine();

    // Setup workspace
    let workspace =
        Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature/test");
    engine.add_workspace(workspace).unwrap();

    // Create pipeline with workspace
    let pipeline = make_test_pipeline("pipe-1", "test")
        .with_workspace(WorkspaceId("ws-1".to_string()))
        .with_current_task(TaskId("task-1".to_string()));
    engine.add_pipeline(pipeline).unwrap();

    // Create and start task
    let task = Task::new(
        "task-1",
        PipelineId("pipe-1".to_string()),
        "init",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    engine.add_task(task).unwrap();

    engine
        .process_task_event(
            &TaskId("task-1".to_string()),
            TaskEvent::Start {
                session_id: oj_core::session::SessionId("sess-1".to_string()),
            },
        )
        .await
        .unwrap();

    // Signal done with error
    engine
        .signal_done(
            &WorkspaceId("ws-1".to_string()),
            Some("something went wrong".to_string()),
        )
        .await
        .unwrap();

    // Task should be failed
    let task = engine.get_task(&TaskId("task-1".to_string())).unwrap();
    assert!(task.is_terminal());
    assert!(matches!(task.state, TaskState::Failed { .. }));
}

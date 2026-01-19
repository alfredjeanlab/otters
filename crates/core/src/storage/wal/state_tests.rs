// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::collections::BTreeMap;

fn ts() -> i64 {
    1705123456789000
}

fn ts_u64() -> u64 {
    1705123456789000
}

#[test]
fn apply_pipeline_create() {
    let mut state = MaterializedState::new();

    let op = Operation::PipelineCreate(PipelineCreateOp {
        id: "pipe-1".to_string(),
        kind: "Dynamic".to_string(),
        name: "Test Pipeline".to_string(),
        workspace_id: Some("ws-1".to_string()),
        inputs: {
            let mut m = BTreeMap::new();
            m.insert("input1".to_string(), "value1".to_string());
            m
        },
        outputs: BTreeMap::new(),
        created_at_micros: ts(),
    });

    state.apply(&op, ts_u64()).unwrap();

    let pipeline = state.pipeline(&PipelineId("pipe-1".to_string())).unwrap();
    assert_eq!(pipeline.name, "Test Pipeline");
    assert_eq!(pipeline.workspace_id, Some(WorkspaceId("ws-1".to_string())));
    assert_eq!(pipeline.inputs.get("input1"), Some(&"value1".to_string()));
}

#[test]
fn apply_pipeline_create_duplicate_fails() {
    let mut state = MaterializedState::new();

    let op = Operation::PipelineCreate(PipelineCreateOp {
        id: "pipe-1".to_string(),
        kind: "Dynamic".to_string(),
        name: "Test".to_string(),
        workspace_id: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        created_at_micros: ts(),
    });

    state.apply(&op, ts_u64()).unwrap();
    let result = state.apply(&op, ts_u64());

    assert!(matches!(result, Err(ApplyError::AlreadyExists { .. })));
}

#[test]
fn apply_pipeline_transition() {
    let mut state = MaterializedState::new();

    // Create pipeline first
    state
        .apply(
            &Operation::PipelineCreate(PipelineCreateOp {
                id: "pipe-1".to_string(),
                kind: "Dynamic".to_string(),
                name: "Test".to_string(),
                workspace_id: None,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                created_at_micros: ts(),
            }),
            ts_u64(),
        )
        .unwrap();

    // Transition
    state
        .apply(
            &Operation::PipelineTransition(PipelineTransitionOp {
                id: "pipe-1".to_string(),
                from_phase: "init".to_string(),
                to_phase: "plan".to_string(),
                workspace_id: None,
                outputs: Some({
                    let mut m = BTreeMap::new();
                    m.insert("result".to_string(), "success".to_string());
                    m
                }),
                current_task_id: Some("task-1".to_string()),
                failed_reason: None,
                blocked_waiting_on: None,
                blocked_guard_id: None,
            }),
            ts_u64(),
        )
        .unwrap();

    let pipeline = state.pipeline(&PipelineId("pipe-1".to_string())).unwrap();
    assert!(matches!(pipeline.phase, Phase::Plan));
    assert_eq!(pipeline.outputs.get("result"), Some(&"success".to_string()));
    assert_eq!(pipeline.current_task_id, Some(TaskId("task-1".to_string())));
}

#[test]
fn apply_pipeline_transition_to_failed() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::PipelineCreate(PipelineCreateOp {
                id: "pipe-1".to_string(),
                kind: "Dynamic".to_string(),
                name: "Test".to_string(),
                workspace_id: None,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                created_at_micros: ts(),
            }),
            ts_u64(),
        )
        .unwrap();

    state
        .apply(
            &Operation::PipelineTransition(PipelineTransitionOp {
                id: "pipe-1".to_string(),
                from_phase: "init".to_string(),
                to_phase: "failed".to_string(),
                workspace_id: None,
                outputs: None,
                current_task_id: None,
                failed_reason: Some("Something went wrong".to_string()),
                blocked_waiting_on: None,
                blocked_guard_id: None,
            }),
            ts_u64(),
        )
        .unwrap();

    let pipeline = state.pipeline(&PipelineId("pipe-1".to_string())).unwrap();
    assert!(
        matches!(&pipeline.phase, Phase::Failed { reason } if reason == "Something went wrong")
    );
}

#[test]
fn apply_pipeline_delete() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::PipelineCreate(PipelineCreateOp {
                id: "pipe-1".to_string(),
                kind: "Dynamic".to_string(),
                name: "Test".to_string(),
                workspace_id: None,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                created_at_micros: ts(),
            }),
            ts_u64(),
        )
        .unwrap();

    state
        .apply(
            &Operation::PipelineDelete(PipelineDeleteOp {
                id: "pipe-1".to_string(),
            }),
            ts_u64(),
        )
        .unwrap();

    assert!(state.pipeline(&PipelineId("pipe-1".to_string())).is_none());
}

#[test]
fn apply_task_create() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::TaskCreate(TaskCreateOp {
                id: "task-1".to_string(),
                pipeline_id: "pipe-1".to_string(),
                phase: "plan".to_string(),
                heartbeat_interval_secs: 30,
                stuck_threshold_secs: 300,
            }),
            ts_u64(),
        )
        .unwrap();

    let task = state.task(&TaskId("task-1".to_string())).unwrap();
    assert_eq!(task.pipeline_id, PipelineId("pipe-1".to_string()));
    assert_eq!(task.phase, "plan");
    assert!(matches!(task.state, TaskState::Pending));
}

#[test]
fn apply_task_transition_to_running() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::TaskCreate(TaskCreateOp {
                id: "task-1".to_string(),
                pipeline_id: "pipe-1".to_string(),
                phase: "plan".to_string(),
                heartbeat_interval_secs: 30,
                stuck_threshold_secs: 300,
            }),
            ts_u64(),
        )
        .unwrap();

    state
        .apply(
            &Operation::TaskTransition(TaskTransitionOp {
                id: "task-1".to_string(),
                from_state: "pending".to_string(),
                to_state: "running".to_string(),
                session_id: Some("session-1".to_string()),
                output: None,
                failed_reason: None,
                nudge_count: None,
            }),
            ts_u64(),
        )
        .unwrap();

    let task = state.task(&TaskId("task-1".to_string())).unwrap();
    assert!(matches!(task.state, TaskState::Running));
    assert_eq!(
        task.session_id,
        Some(crate::session::SessionId("session-1".to_string()))
    );
}

#[test]
fn apply_workspace_create() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::WorkspaceCreate(WorkspaceCreateOp {
                id: "ws-1".to_string(),
                name: "feature-branch".to_string(),
                path: "/tmp/worktree/feature".to_string(),
                branch: "feature/test".to_string(),
                state: "ready".to_string(),
                created_at_micros: ts(),
            }),
            ts_u64(),
        )
        .unwrap();

    let workspace = state.workspace(&WorkspaceId("ws-1".to_string())).unwrap();
    assert_eq!(workspace.name, "feature-branch");
    assert_eq!(workspace.branch, "feature/test");
    assert!(matches!(workspace.state, WorkspaceState::Ready));
}

#[test]
fn apply_queue_push() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::QueuePush(QueuePushOp {
                queue_name: "tasks".to_string(),
                item_id: "item-1".to_string(),
                data: {
                    let mut m = BTreeMap::new();
                    m.insert("task".to_string(), "do something".to_string());
                    m
                },
                priority: 5,
                max_attempts: 3,
            }),
            ts_u64(),
        )
        .unwrap();

    let queue = state.queue("tasks").unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.items[0].id, "item-1");
    assert_eq!(queue.items[0].priority, 5);
}

#[test]
fn apply_lock_acquire_and_release() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::LockAcquire(LockAcquireOp {
                lock_name: "my-lock".to_string(),
                holder_id: "worker-1".to_string(),
                metadata: Some("working on it".to_string()),
            }),
            ts_u64(),
        )
        .unwrap();

    assert!(state.coordination.get_lock("my-lock").is_some());

    state
        .apply(
            &Operation::LockRelease(LockReleaseOp {
                lock_name: "my-lock".to_string(),
                holder_id: "worker-1".to_string(),
            }),
            ts_u64(),
        )
        .unwrap();
}

#[test]
fn apply_event_emit() {
    let mut state = MaterializedState::new();

    state
        .apply(
            &Operation::EventEmit(EventEmitOp {
                event_type: "pipeline.completed".to_string(),
                payload: serde_json::json!({"id": "pipe-1"}),
            }),
            ts_u64(),
        )
        .unwrap();

    let events = state.recent_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "pipeline.completed");
}

#[test]
fn apply_snapshot_taken_is_noop() {
    let mut state = MaterializedState::new();

    let result = state.apply(
        &Operation::SnapshotTaken {
            snapshot_id: "snap-1".to_string(),
        },
        ts_u64(),
    );

    assert!(result.is_ok());
}

#[test]
fn apply_transition_nonexistent_pipeline_fails() {
    let mut state = MaterializedState::new();

    let result = state.apply(
        &Operation::PipelineTransition(PipelineTransitionOp {
            id: "nonexistent".to_string(),
            from_phase: "init".to_string(),
            to_phase: "plan".to_string(),
            workspace_id: None,
            outputs: None,
            current_task_id: None,
            failed_reason: None,
            blocked_waiting_on: None,
            blocked_guard_id: None,
        }),
        ts_u64(),
    );

    assert!(matches!(result, Err(ApplyError::NotFound { .. })));
}

#[test]
fn state_is_deterministic() {
    let ops = vec![
        Operation::PipelineCreate(PipelineCreateOp {
            id: "pipe-1".to_string(),
            kind: "Dynamic".to_string(),
            name: "Test".to_string(),
            workspace_id: None,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            created_at_micros: ts(),
        }),
        Operation::TaskCreate(TaskCreateOp {
            id: "task-1".to_string(),
            pipeline_id: "pipe-1".to_string(),
            phase: "plan".to_string(),
            heartbeat_interval_secs: 30,
            stuck_threshold_secs: 300,
        }),
        Operation::PipelineTransition(PipelineTransitionOp {
            id: "pipe-1".to_string(),
            from_phase: "init".to_string(),
            to_phase: "done".to_string(),
            workspace_id: None,
            outputs: None,
            current_task_id: None,
            failed_reason: None,
            blocked_waiting_on: None,
            blocked_guard_id: None,
        }),
    ];

    let mut state1 = MaterializedState::new();
    let mut state2 = MaterializedState::new();

    for op in &ops {
        state1.apply(op, ts_u64()).unwrap();
    }

    for op in &ops {
        state2.apply(op, ts_u64()).unwrap();
    }

    // Same operations produce same state
    assert_eq!(state1.pipelines.len(), state2.pipelines.len());
    assert_eq!(state1.tasks.len(), state2.tasks.len());

    let p1 = state1.pipeline(&PipelineId("pipe-1".to_string())).unwrap();
    let p2 = state2.pipeline(&PipelineId("pipe-1".to_string())).unwrap();
    assert_eq!(p1.name, p2.name);
    assert!(matches!(p1.phase, Phase::Done));
    assert!(matches!(p2.phase, Phase::Done));
}

#[test]
fn events_ring_buffer_limits_size() {
    let mut state = MaterializedState::new();

    // Add more than MAX_EVENTS
    for i in 0..1100 {
        state
            .apply(
                &Operation::EventEmit(EventEmitOp {
                    event_type: format!("event.{}", i),
                    payload: serde_json::json!({}),
                }),
                ts_u64(),
            )
            .unwrap();
    }

    // Should only keep MAX_EVENTS (1000)
    assert_eq!(state.recent_events().len(), 1000);

    // First event should be #100 (0-99 were dropped)
    assert_eq!(state.recent_events()[0].event_type, "event.100");
}

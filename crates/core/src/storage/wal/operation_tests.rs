// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::collections::BTreeMap;

#[test]
fn pipeline_create_roundtrip() {
    let op = Operation::PipelineCreate(PipelineCreateOp {
        id: "pipe-1".to_string(),
        kind: "Dynamic".to_string(),
        name: "test pipeline".to_string(),
        workspace_id: Some("ws-1".to_string()),
        inputs: {
            let mut m = BTreeMap::new();
            m.insert("input1".to_string(), "value1".to_string());
            m
        },
        outputs: BTreeMap::new(),
        created_at_micros: 1705123456789000,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains(r#""type":"pipeline_create""#));
}

#[test]
fn pipeline_transition_roundtrip() {
    let op = Operation::PipelineTransition(PipelineTransitionOp {
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
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn task_create_roundtrip() {
    let op = Operation::TaskCreate(TaskCreateOp {
        id: "task-1".to_string(),
        pipeline_id: "pipe-1".to_string(),
        phase: "plan".to_string(),
        heartbeat_interval_secs: 30,
        stuck_threshold_secs: 300,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn queue_push_roundtrip() {
    let op = Operation::QueuePush(QueuePushOp {
        queue_name: "tasks".to_string(),
        item_id: "item-1".to_string(),
        data: {
            let mut m = BTreeMap::new();
            m.insert("task".to_string(), "do something".to_string());
            m
        },
        priority: 5,
        max_attempts: 3,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn lock_acquire_roundtrip() {
    let op = Operation::LockAcquire(LockAcquireOp {
        lock_name: "my-lock".to_string(),
        holder_id: "worker-1".to_string(),
        metadata: Some("important work".to_string()),
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn event_emit_roundtrip() {
    let op = Operation::EventEmit(EventEmitOp {
        event_type: "pipeline.completed".to_string(),
        payload: serde_json::json!({
            "pipeline_id": "pipe-1",
            "duration_ms": 12345
        }),
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn snapshot_taken_roundtrip() {
    let op = Operation::SnapshotTaken {
        snapshot_id: "00000042-1705123456".to_string(),
    };

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
}

#[test]
fn session_heartbeat_roundtrip() {
    let op = Operation::SessionHeartbeat(SessionHeartbeatOp {
        id: "session-123".to_string(),
        timestamp_micros: 1705123456789000,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains(r#""type":"session_heartbeat""#));
}

#[test]
fn optional_fields_omitted_when_none() {
    let op = Operation::PipelineTransition(PipelineTransitionOp {
        id: "pipe-1".to_string(),
        from_phase: "init".to_string(),
        to_phase: "done".to_string(),
        workspace_id: None,
        outputs: None,
        current_task_id: None,
        failed_reason: None,
        blocked_waiting_on: None,
        blocked_guard_id: None,
    });

    let json = serde_json::to_string(&op).unwrap();

    // Optional fields should be omitted
    assert!(!json.contains("outputs"));
    assert!(!json.contains("current_task_id"));
    assert!(!json.contains("failed_reason"));

    let parsed: Operation = serde_json::from_str(&json).unwrap();
    assert_eq!(op, parsed);
}

#[test]
fn backward_compat_with_defaults() {
    // Simulate an old JSON without optional fields
    let json =
        r#"{"type":"pipeline_transition","id":"pipe-1","from_phase":"init","to_phase":"done"}"#;

    let parsed: Operation = serde_json::from_str(json).unwrap();

    if let Operation::PipelineTransition(op) = parsed {
        assert_eq!(op.id, "pipe-1");
        assert_eq!(op.from_phase, "init");
        assert_eq!(op.to_phase, "done");
        assert!(op.outputs.is_none());
        assert!(op.current_task_id.is_none());
    } else {
        panic!("expected PipelineTransition");
    }
}

#[test]
fn all_operation_types_have_stable_names() {
    // This test ensures the serialized type names are stable
    let ops: Vec<(&str, Operation)> = vec![
        (
            "pipeline_create",
            Operation::PipelineCreate(PipelineCreateOp {
                id: "x".to_string(),
                kind: "Dynamic".to_string(),
                name: "x".to_string(),
                workspace_id: None,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                created_at_micros: 0,
            }),
        ),
        (
            "pipeline_transition",
            Operation::PipelineTransition(PipelineTransitionOp {
                id: "x".to_string(),
                from_phase: "a".to_string(),
                to_phase: "b".to_string(),
                workspace_id: None,
                outputs: None,
                current_task_id: None,
                failed_reason: None,
                blocked_waiting_on: None,
                blocked_guard_id: None,
            }),
        ),
        (
            "pipeline_delete",
            Operation::PipelineDelete(PipelineDeleteOp {
                id: "x".to_string(),
            }),
        ),
        (
            "task_create",
            Operation::TaskCreate(TaskCreateOp {
                id: "x".to_string(),
                pipeline_id: "x".to_string(),
                phase: "x".to_string(),
                heartbeat_interval_secs: 0,
                stuck_threshold_secs: 0,
            }),
        ),
        (
            "task_transition",
            Operation::TaskTransition(TaskTransitionOp {
                id: "x".to_string(),
                from_state: "a".to_string(),
                to_state: "b".to_string(),
                session_id: None,
                output: None,
                failed_reason: None,
                nudge_count: None,
            }),
        ),
        (
            "task_delete",
            Operation::TaskDelete(TaskDeleteOp {
                id: "x".to_string(),
            }),
        ),
        (
            "lock_acquire",
            Operation::LockAcquire(LockAcquireOp {
                lock_name: "x".to_string(),
                holder_id: "x".to_string(),
                metadata: None,
            }),
        ),
        (
            "lock_release",
            Operation::LockRelease(LockReleaseOp {
                lock_name: "x".to_string(),
                holder_id: "x".to_string(),
            }),
        ),
        (
            "snapshot_taken",
            Operation::SnapshotTaken {
                snapshot_id: "x".to_string(),
            },
        ),
        (
            "action_execution_started",
            Operation::ActionExecutionStarted(ActionExecutionStartedOp {
                action_id: "x".to_string(),
                source: "x".to_string(),
                execution_type: "x".to_string(),
                started_at: 0,
            }),
        ),
        (
            "action_execution_completed",
            Operation::ActionExecutionCompleted(ActionExecutionCompletedOp {
                action_id: "x".to_string(),
                success: true,
                output: None,
                error: None,
                duration_ms: 0,
                completed_at: 0,
            }),
        ),
        (
            "cleanup_executed",
            Operation::CleanupExecuted(CleanupExecutedOp {
                scanner_id: "x".to_string(),
                resource_id: "x".to_string(),
                action: "x".to_string(),
                success: true,
                error: None,
                executed_at: 0,
            }),
        ),
    ];

    for (expected_type, op) in ops {
        let json = serde_json::to_string(&op).unwrap();
        assert!(
            json.contains(&format!(r#""type":"{}""#, expected_type)),
            "expected type {} in {}",
            expected_type,
            json
        );
    }
}

#[test]
fn action_execution_started_roundtrip() {
    let op = Operation::ActionExecutionStarted(ActionExecutionStartedOp {
        action_id: "notify-action".to_string(),
        source: "watcher:idle-check".to_string(),
        execution_type: "command".to_string(),
        started_at: 1705123456789,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains(r#""type":"action_execution_started""#));
}

#[test]
fn action_execution_completed_roundtrip() {
    let op = Operation::ActionExecutionCompleted(ActionExecutionCompletedOp {
        action_id: "notify-action".to_string(),
        success: true,
        output: Some("notification sent".to_string()),
        error: None,
        duration_ms: 150,
        completed_at: 1705123456939,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains(r#""type":"action_execution_completed""#));
}

#[test]
fn action_execution_completed_with_error_roundtrip() {
    let op = Operation::ActionExecutionCompleted(ActionExecutionCompletedOp {
        action_id: "deploy-action".to_string(),
        success: false,
        output: None,
        error: Some("connection refused".to_string()),
        duration_ms: 5000,
        completed_at: 1705123461789,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);

    // Verify optional fields behavior
    assert!(!json.contains("output")); // None should be omitted
    assert!(json.contains("error"));
}

#[test]
fn cleanup_executed_roundtrip() {
    let op = Operation::CleanupExecuted(CleanupExecutedOp {
        scanner_id: "stale-locks".to_string(),
        resource_id: "lock:deploy".to_string(),
        action: "release".to_string(),
        success: true,
        error: None,
        executed_at: 1705123456789,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains(r#""type":"cleanup_executed""#));
}

#[test]
fn cleanup_executed_with_error_roundtrip() {
    let op = Operation::CleanupExecuted(CleanupExecutedOp {
        scanner_id: "stale-sessions".to_string(),
        resource_id: "session:abc123".to_string(),
        action: "terminate".to_string(),
        success: false,
        error: Some("session not found".to_string()),
        executed_at: 1705123456789,
    });

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(op, parsed);
    assert!(json.contains("error"));
}

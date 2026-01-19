// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::storage::wal::operation::*;
use crate::storage::wal::state::MaterializedState;
use std::collections::BTreeMap;
use tempfile::TempDir;

fn temp_snapshots_dir() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("snapshots");
    (dir, path)
}

fn create_test_state() -> MaterializedState {
    let mut state = MaterializedState::new();

    // Add a pipeline
    state
        .apply(
            &Operation::PipelineCreate(PipelineCreateOp {
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
                created_at_micros: 1705123456789000,
            }),
            1705123456789000,
        )
        .unwrap();

    // Add a task
    state
        .apply(
            &Operation::TaskCreate(TaskCreateOp {
                id: "task-1".to_string(),
                pipeline_id: "pipe-1".to_string(),
                phase: "plan".to_string(),
                heartbeat_interval_secs: 30,
                stuck_threshold_secs: 300,
            }),
            1705123456789000,
        )
        .unwrap();

    // Add a workspace
    state
        .apply(
            &Operation::WorkspaceCreate(WorkspaceCreateOp {
                id: "ws-1".to_string(),
                name: "feature-branch".to_string(),
                path: "/tmp/worktree".to_string(),
                branch: "feature/test".to_string(),
                state: "ready".to_string(),
                created_at_micros: 1705123456789000,
            }),
            1705123456789000,
        )
        .unwrap();

    // Add a queue item
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
            1705123456789000,
        )
        .unwrap();

    state
}

#[test]
fn storable_state_from_materialized() {
    let state = create_test_state();
    let storable = StorableState::from_materialized(&state, 42);

    assert_eq!(storable.version, StorableState::CURRENT_VERSION);
    assert_eq!(storable.sequence_at_snapshot, 42);
    assert_eq!(storable.pipelines.len(), 1);
    assert_eq!(storable.tasks.len(), 1);
    assert_eq!(storable.workspaces.len(), 1);
    assert_eq!(storable.queues.len(), 1);

    assert_eq!(storable.pipelines[0].id, "pipe-1");
    assert_eq!(storable.tasks[0].id, "task-1");
    assert_eq!(storable.workspaces[0].id, "ws-1");
    assert_eq!(storable.queues[0].name, "tasks");
}

#[test]
fn storable_state_roundtrip() {
    let state = create_test_state();
    let storable = StorableState::from_materialized(&state, 42);

    let json = serde_json::to_string(&storable).unwrap();
    let parsed: StorableState = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.version, storable.version);
    assert_eq!(parsed.sequence_at_snapshot, storable.sequence_at_snapshot);
    assert_eq!(parsed.pipelines.len(), storable.pipelines.len());
    assert_eq!(parsed.tasks.len(), storable.tasks.len());
}

#[test]
fn storable_state_to_materialized() {
    let state = create_test_state();
    let storable = StorableState::from_materialized(&state, 42);
    let restored = storable.to_materialized();

    assert_eq!(restored.pipelines.len(), 1);
    assert_eq!(restored.tasks.len(), 1);
    assert_eq!(restored.workspaces.len(), 1);
    assert_eq!(restored.queues.len(), 1);

    let pipeline = restored
        .pipeline(&crate::pipeline::PipelineId("pipe-1".to_string()))
        .unwrap();
    assert_eq!(pipeline.name, "Test Pipeline");
}

#[test]
fn snapshot_manager_create_and_load() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let state = create_test_state();
    let meta = manager.create_snapshot(&state, 100).unwrap();

    assert_eq!(meta.sequence, 100);
    assert!(meta.size_bytes > 0);

    let loaded = manager.load_snapshot(&meta.id).unwrap();

    assert_eq!(loaded.sequence_at_snapshot, 100);
    assert_eq!(loaded.pipelines.len(), 1);
}

#[test]
fn snapshot_manager_list_snapshots() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let state = create_test_state();

    manager.create_snapshot(&state, 10).unwrap();
    manager.create_snapshot(&state, 20).unwrap();
    manager.create_snapshot(&state, 30).unwrap();

    let list = manager.list_snapshots().unwrap();

    assert_eq!(list.len(), 3);
    // Should be sorted newest first
    assert_eq!(list[0].sequence, 30);
    assert_eq!(list[1].sequence, 20);
    assert_eq!(list[2].sequence, 10);
}

#[test]
fn snapshot_manager_latest_snapshot() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let state = create_test_state();

    manager.create_snapshot(&state, 10).unwrap();
    manager.create_snapshot(&state, 30).unwrap();
    manager.create_snapshot(&state, 20).unwrap();

    let latest = manager.latest_snapshot().unwrap().unwrap();

    assert_eq!(latest.sequence, 30);
}

#[test]
fn snapshot_manager_latest_snapshot_empty() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let latest = manager.latest_snapshot().unwrap();

    assert!(latest.is_none());
}

#[test]
fn snapshot_manager_delete_snapshot() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let state = create_test_state();
    let meta = manager.create_snapshot(&state, 100).unwrap();

    manager.delete_snapshot(&meta.id).unwrap();

    let result = manager.load_snapshot(&meta.id);
    assert!(matches!(result, Err(SnapshotError::NotFound(_))));
}

#[test]
fn snapshot_manager_cleanup_old_snapshots() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let state = create_test_state();

    manager.create_snapshot(&state, 10).unwrap();
    manager.create_snapshot(&state, 20).unwrap();
    manager.create_snapshot(&state, 30).unwrap();
    manager.create_snapshot(&state, 40).unwrap();
    manager.create_snapshot(&state, 50).unwrap();

    // Keep snapshots at or after seq 40, plus 1 older one
    let deleted = manager.cleanup_old_snapshots(40, 1).unwrap();

    // Should delete seq 10 and 20, keeping 30 as the one old snapshot
    assert_eq!(deleted.len(), 2);

    let remaining = manager.list_snapshots().unwrap();
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0].sequence, 50);
    assert_eq!(remaining[1].sequence, 40);
    assert_eq!(remaining[2].sequence, 30);
}

#[test]
fn snapshot_manager_load_nonexistent_fails() {
    let (_dir, path) = temp_snapshots_dir();
    let manager = SnapshotManager::new(&path);

    let result = manager.load_snapshot("nonexistent");

    assert!(matches!(result, Err(SnapshotError::NotFound(_))));
}

#[test]
fn generate_id_format() {
    let timestamp = chrono::DateTime::parse_from_rfc3339("2024-01-13T12:34:56Z")
        .unwrap()
        .with_timezone(&Utc);

    let id = SnapshotManager::generate_id(42, timestamp);

    assert_eq!(id, "00000042-20240113123456");
}

#[test]
fn storable_pipeline_preserves_data() {
    let state = create_test_state();
    let pipeline = state
        .pipeline(&crate::pipeline::PipelineId("pipe-1".to_string()))
        .unwrap();

    let storable = StorablePipeline::from(pipeline);

    assert_eq!(storable.id, "pipe-1");
    assert_eq!(storable.name, "Test Pipeline");
    assert_eq!(storable.workspace_id, Some("ws-1".to_string()));
    assert_eq!(storable.inputs.get("input1"), Some(&"value1".to_string()));
}

#[test]
fn storable_queue_preserves_items() {
    let state = create_test_state();
    let queue = state.queue("tasks").unwrap();

    let storable = StorableQueue::from_queue("tasks", queue);

    assert_eq!(storable.name, "tasks");
    assert_eq!(storable.items.len(), 1);
    assert_eq!(storable.items[0].id, "item-1");
    assert_eq!(storable.items[0].priority, 5);
}

#[test]
fn session_heartbeat_survives_snapshot() {
    let mut state = MaterializedState::new();

    // Create a session
    state
        .apply(
            &Operation::SessionCreate(SessionCreateOp {
                id: "session-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                idle_threshold_secs: 300,
            }),
            1705123456789000,
        )
        .unwrap();

    // Record a heartbeat
    state
        .apply(
            &Operation::SessionHeartbeat(SessionHeartbeatOp {
                id: "session-1".to_string(),
                timestamp_micros: 1705123456789000,
            }),
            1705123456789000,
        )
        .unwrap();

    // Verify heartbeat is set
    let session = state
        .session(&crate::session::SessionId("session-1".into()))
        .unwrap();
    assert!(session.last_heartbeat.is_some());

    // Convert to storable
    let storable = StorableState::from_materialized(&state, 42);
    assert_eq!(storable.sessions.len(), 1);
    assert!(storable.sessions[0].last_heartbeat_age_micros.is_some());

    // Roundtrip through JSON
    let json = serde_json::to_string(&storable).unwrap();
    let parsed: StorableState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.sessions.len(), 1);
    assert!(parsed.sessions[0].last_heartbeat_age_micros.is_some());

    // Restore to materialized
    let restored = parsed.to_materialized();
    let restored_session = restored
        .session(&crate::session::SessionId("session-1".into()))
        .unwrap();
    assert!(restored_session.last_heartbeat.is_some());
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::pipeline::Pipeline;
use crate::task::Task;
use crate::workspace::Workspace;
use std::collections::BTreeMap;
use std::time::Duration;
use tempfile::TempDir;

fn temp_store_dir() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn test_config() -> WalStoreConfig {
    WalStoreConfig {
        snapshot_interval: 5, // Snapshot every 5 operations
        keep_old_snapshots: 1,
        compaction_threshold: 100,
        machine_id: "test-machine".to_string(),
    }
}

#[test]
fn store_creates_directory() {
    let (_dir, path) = temp_store_dir();
    let subdir = path.join("nested");

    let _store = WalStore::open(&subdir, test_config()).unwrap();

    assert!(subdir.exists());
    assert!(subdir.join("wal.jsonl").exists());
}

#[test]
fn store_save_and_load_pipeline() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let pipeline = Pipeline::new_dynamic("pipe-1", "Test Pipeline", {
        let mut m = BTreeMap::new();
        m.insert("input".to_string(), "value".to_string());
        m
    });

    store.save_pipeline(&pipeline).unwrap();

    let loaded = store
        .load_pipeline(&PipelineId("pipe-1".to_string()))
        .unwrap();
    assert_eq!(loaded.name, "Test Pipeline");
    assert_eq!(loaded.inputs.get("input"), Some(&"value".to_string()));
}

#[test]
fn store_save_and_load_task() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let task = Task {
        id: TaskId("task-1".to_string()),
        pipeline_id: PipelineId("pipe-1".to_string()),
        phase: "plan".to_string(),
        state: TaskState::Pending,
        session_id: None,
        heartbeat_interval: Duration::from_secs(30),
        stuck_threshold: Duration::from_secs(300),
        last_heartbeat: None,
        created_at: std::time::Instant::now(),
        started_at: None,
        completed_at: None,
    };

    store.save_task(&task).unwrap();

    let loaded = store.load_task(&TaskId("task-1".to_string())).unwrap();
    assert_eq!(loaded.phase, "plan");
    assert!(matches!(loaded.state, TaskState::Pending));
}

#[test]
fn store_save_and_load_workspace() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let workspace = Workspace::new_ready(
        "ws-1",
        "feature-branch",
        PathBuf::from("/tmp/worktree"),
        "feature/test",
    );

    store.save_workspace(&workspace).unwrap();

    let loaded = store
        .load_workspace(&WorkspaceId("ws-1".to_string()))
        .unwrap();
    assert_eq!(loaded.name, "feature-branch");
    assert_eq!(loaded.branch, "feature/test");
}

#[test]
fn store_list_entities() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    for i in 0..3 {
        let id = format!("pipe-{}", i);
        let pipeline = Pipeline::new_dynamic(&id, "Test", BTreeMap::new());
        store.save_pipeline(&pipeline).unwrap();
    }

    let ids = store.list_pipelines().unwrap();
    assert_eq!(ids.len(), 3);
}

#[test]
fn store_delete_pipeline() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let pipeline = Pipeline::new_dynamic("pipe-1", "Test", BTreeMap::new());
    store.save_pipeline(&pipeline).unwrap();

    store
        .delete_pipeline(&PipelineId("pipe-1".to_string()))
        .unwrap();

    let result = store.load_pipeline(&PipelineId("pipe-1".to_string()));
    assert!(matches!(result, Err(WalStoreError::NotFound { .. })));
}

#[test]
fn store_recovers_from_wal() {
    let (_dir, path) = temp_store_dir();

    // Write some data
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();

        let pipeline = Pipeline::new_dynamic("pipe-1", "Test Pipeline", BTreeMap::new());
        store.save_pipeline(&pipeline).unwrap();

        let task = Task {
            id: TaskId("task-1".to_string()),
            pipeline_id: PipelineId("pipe-1".to_string()),
            phase: "plan".to_string(),
            state: TaskState::Pending,
            session_id: None,
            heartbeat_interval: Duration::from_secs(30),
            stuck_threshold: Duration::from_secs(300),
            last_heartbeat: None,
            created_at: std::time::Instant::now(),
            started_at: None,
            completed_at: None,
        };
        store.save_task(&task).unwrap();
    }

    // Reopen - should recover from WAL
    {
        let store = WalStore::open(&path, test_config()).unwrap();

        let pipeline = store
            .load_pipeline(&PipelineId("pipe-1".to_string()))
            .unwrap();
        assert_eq!(pipeline.name, "Test Pipeline");

        let task = store.load_task(&TaskId("task-1".to_string())).unwrap();
        assert_eq!(task.phase, "plan");
    }
}

#[test]
fn store_recovers_from_snapshot_plus_wal() {
    let (_dir, path) = temp_store_dir();

    // Create snapshot after some operations
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();

        let pipeline = Pipeline::new_dynamic("pipe-1", "Test", BTreeMap::new());
        store.save_pipeline(&pipeline).unwrap();

        // Create snapshot
        store.create_snapshot().unwrap();

        // Write more after snapshot
        let workspace = Workspace::new_ready("ws-1", "feature", PathBuf::from("/tmp"), "main");
        store.save_workspace(&workspace).unwrap();
    }

    // Reopen - should recover from snapshot + WAL
    {
        let store = WalStore::open(&path, test_config()).unwrap();

        // Data from before snapshot
        let pipeline = store
            .load_pipeline(&PipelineId("pipe-1".to_string()))
            .unwrap();
        assert_eq!(pipeline.name, "Test");

        // Data from after snapshot (recovered from WAL)
        let workspace = store
            .load_workspace(&WorkspaceId("ws-1".to_string()))
            .unwrap();
        assert_eq!(workspace.name, "feature");
    }
}

#[test]
fn store_auto_snapshot() {
    let (_dir, path) = temp_store_dir();
    let config = WalStoreConfig {
        snapshot_interval: 3, // Snapshot every 3 operations
        keep_old_snapshots: 1,
        compaction_threshold: 100,
        machine_id: "test".to_string(),
    };

    let mut store = WalStore::open(&path, config).unwrap();

    // First two operations - no snapshot
    let pipeline = Pipeline::new_dynamic("pipe-1", "Test", BTreeMap::new());
    store.save_pipeline(&pipeline).unwrap();
    assert_eq!(store.ops_since_snapshot, 1);

    let result = store.maybe_snapshot().unwrap();
    assert!(result.is_none());

    store
        .save_pipeline(&Pipeline::new_dynamic("pipe-2", "Test2", BTreeMap::new()))
        .unwrap();
    store
        .save_pipeline(&Pipeline::new_dynamic("pipe-3", "Test3", BTreeMap::new()))
        .unwrap();

    // Third operation - should trigger snapshot
    let result = store.maybe_snapshot().unwrap();
    assert!(result.is_some());
    assert_eq!(store.ops_since_snapshot, 0);
}

#[test]
fn store_update_pipeline() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    // Create
    let mut pipeline = Pipeline::new_dynamic("pipe-1", "Test", BTreeMap::new());
    store.save_pipeline(&pipeline).unwrap();

    // Update
    pipeline
        .outputs
        .insert("result".to_string(), "success".to_string());
    store.save_pipeline(&pipeline).unwrap();

    let loaded = store
        .load_pipeline(&PipelineId("pipe-1".to_string()))
        .unwrap();
    assert_eq!(loaded.outputs.get("result"), Some(&"success".to_string()));
}

#[test]
fn store_update_task_state() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    // Create
    let mut task = Task {
        id: TaskId("task-1".to_string()),
        pipeline_id: PipelineId("pipe-1".to_string()),
        phase: "plan".to_string(),
        state: TaskState::Pending,
        session_id: None,
        heartbeat_interval: Duration::from_secs(30),
        stuck_threshold: Duration::from_secs(300),
        last_heartbeat: None,
        created_at: std::time::Instant::now(),
        started_at: None,
        completed_at: None,
    };
    store.save_task(&task).unwrap();

    // Update state
    task.state = TaskState::Running;
    task.session_id = Some(crate::session::SessionId("session-1".to_string()));
    store.save_task(&task).unwrap();

    let loaded = store.load_task(&TaskId("task-1".to_string())).unwrap();
    assert!(matches!(loaded.state, TaskState::Running));
    assert_eq!(
        loaded.session_id,
        Some(crate::session::SessionId("session-1".to_string()))
    );
}

#[test]
fn store_queue_push() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let mut data = BTreeMap::new();
    data.insert("task".to_string(), "do something".to_string());

    store.queue_push("tasks", "item-1", data, 5, 3).unwrap();

    let queue = store.queue("tasks").unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.items[0].priority, 5);
}

#[test]
fn store_delete_nonexistent_fails() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    let result = store.delete_pipeline(&PipelineId("nonexistent".to_string()));
    assert!(matches!(result, Err(WalStoreError::NotFound { .. })));
}

#[test]
fn store_sequence_increases() {
    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    assert_eq!(store.sequence(), 0);

    store
        .save_pipeline(&Pipeline::new_dynamic("p1", "t1", BTreeMap::new()))
        .unwrap();
    assert_eq!(store.sequence(), 1);

    store
        .save_pipeline(&Pipeline::new_dynamic("p2", "t2", BTreeMap::new()))
        .unwrap();
    assert_eq!(store.sequence(), 2);
}

#[test]
fn store_compact_cleans_old_snapshots() {
    let (_dir, path) = temp_store_dir();
    let config = WalStoreConfig {
        snapshot_interval: 2,
        keep_old_snapshots: 1,
        compaction_threshold: 100,
        machine_id: "test".to_string(),
    };

    let mut store = WalStore::open(&path, config).unwrap();

    // Create some snapshots
    for i in 0..5 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
        store.create_snapshot().unwrap();
    }

    store.compact().unwrap();

    // Should have kept only keep_old_snapshots + 1 (the latest)
    let snapshots = store.snapshots.list_snapshots().unwrap();
    assert!(snapshots.len() <= 2);
}

#[test]
fn store_temp_creates_unique_directories() {
    let store1 = WalStore::open_temp().unwrap();
    let store2 = WalStore::open_temp().unwrap();

    assert_ne!(store1.base_dir(), store2.base_dir());
}

#[test]
fn session_heartbeat_survives_recovery() {
    use crate::session::SessionId;
    use crate::storage::wal::operation::{Operation, SessionCreateOp};

    let (_dir, path) = temp_store_dir();

    // Create store, add session, heartbeat it
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();

        // First create a session via the WAL
        store
            .append_operation(Operation::SessionCreate(SessionCreateOp {
                id: "session-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                idle_threshold_secs: 300,
            }))
            .unwrap();

        // Record heartbeat
        store.session_heartbeat("session-1").unwrap();
    }

    // Reopen and verify heartbeat was restored
    {
        let store = WalStore::open(&path, test_config()).unwrap();
        let session = store
            .state()
            .session(&SessionId("session-1".into()))
            .unwrap();
        assert!(session.last_heartbeat.is_some());
    }
}

#[test]
fn record_action_execution() {
    use crate::scheduling::ActionId;

    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();
    let action_id = ActionId::new("notify");

    store
        .action_execution_started(&action_id, "watcher:idle-check", "command")
        .unwrap();
    store
        .action_execution_completed(
            &action_id,
            true,
            Some("notified".into()),
            None,
            Duration::from_millis(150),
        )
        .unwrap();

    let recent = store.recent_action_executions(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].action_id, "notify");
    assert!(recent[0].success);
    assert_eq!(recent[0].duration_ms, 150);
    assert_eq!(recent[0].source, "watcher:idle-check");
    assert_eq!(recent[0].execution_type, "command");
}

#[test]
fn record_failed_action() {
    use crate::scheduling::ActionId;

    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();
    let action_id = ActionId::new("deploy");

    store
        .action_execution_started(&action_id, "cron:nightly", "command")
        .unwrap();
    store
        .action_execution_completed(
            &action_id,
            false,
            None,
            Some("connection refused".into()),
            Duration::from_millis(5000),
        )
        .unwrap();

    let stats = store.action_stats(&action_id);
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.failures, 1);
    assert_eq!(stats.successes, 0);

    let recent = store.recent_action_executions(10);
    assert!(!recent[0].success);
    assert_eq!(recent[0].error, Some("connection refused".to_string()));
}

#[test]
fn record_cleanup_operation() {
    use crate::scheduling::ScannerId;

    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();
    let scanner_id = ScannerId::new("stale-locks");

    store
        .cleanup_executed(&scanner_id, "lock:deploy", "release", true, None)
        .unwrap();

    let recent = store.recent_cleanup_operations(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].scanner_id, "stale-locks");
    assert_eq!(recent[0].resource_id, "lock:deploy");
    assert_eq!(recent[0].action, "release");
    assert!(recent[0].success);
}

#[test]
fn execution_history_survives_replay() {
    use crate::scheduling::ActionId;

    let (_dir, path) = temp_store_dir();

    // Write operations
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();
        let action_id = ActionId::new("test");
        store
            .action_execution_started(&action_id, "test-source", "command")
            .unwrap();
        store
            .action_execution_completed(&action_id, true, None, None, Duration::from_millis(100))
            .unwrap();
    }

    // Reopen and verify replay
    {
        let store = WalStore::open(&path, test_config()).unwrap();
        let recent = store.recent_action_executions(10);
        assert_eq!(recent.len(), 1);
        assert!(recent[0].success);
        assert_eq!(recent[0].duration_ms, 100);
    }
}

#[test]
fn action_stats_aggregates_correctly() {
    use crate::scheduling::ActionId;

    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();
    let action_id = ActionId::new("notify");

    // Record multiple executions
    for i in 0..5 {
        store
            .action_execution_started(&action_id, "watcher:check", "command")
            .unwrap();
        let success = i % 2 == 0; // Alternate success/failure
        let duration = Duration::from_millis(100 + i * 50);
        store
            .action_execution_completed(
                &action_id,
                success,
                None,
                if success { None } else { Some("error".into()) },
                duration,
            )
            .unwrap();
    }

    let stats = store.action_stats(&action_id);
    assert_eq!(stats.total_executions, 5);
    assert_eq!(stats.successes, 3); // i=0, 2, 4
    assert_eq!(stats.failures, 2); // i=1, 3
                                   // Average: (100 + 150 + 200 + 250 + 300) / 5 = 200
    assert_eq!(stats.avg_duration_ms, 200);
}

#[test]
fn cleanup_operations_survive_replay() {
    use crate::scheduling::ScannerId;

    let (_dir, path) = temp_store_dir();

    // Write operations
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();
        let scanner_id = ScannerId::new("stale-sessions");

        store
            .cleanup_executed(&scanner_id, "session:abc", "terminate", true, None)
            .unwrap();
        store
            .cleanup_executed(
                &scanner_id,
                "session:def",
                "terminate",
                false,
                Some("not found".into()),
            )
            .unwrap();
    }

    // Reopen and verify replay
    {
        let store = WalStore::open(&path, test_config()).unwrap();
        let recent = store.recent_cleanup_operations(10);
        assert_eq!(recent.len(), 2);
        // Most recent first
        assert_eq!(recent[0].resource_id, "session:def");
        assert!(!recent[0].success);
        assert_eq!(recent[1].resource_id, "session:abc");
        assert!(recent[1].success);
    }
}

#[test]
fn recent_executions_limited_correctly() {
    use crate::scheduling::ActionId;

    let (_dir, path) = temp_store_dir();
    let mut store = WalStore::open(&path, test_config()).unwrap();

    // Add 10 executions
    for i in 0..10 {
        let action_id = ActionId::new(format!("action-{}", i));
        store
            .action_execution_completed(&action_id, true, None, None, Duration::from_millis(100))
            .unwrap();
    }

    // Request limited number
    let recent = store.recent_action_executions(5);
    assert_eq!(recent.len(), 5);

    // Should get most recent ones
    assert_eq!(recent[0].action_id, "action-9"); // Most recent
    assert_eq!(recent[4].action_id, "action-5");
}

#[test]
fn compaction_removes_old_entries() {
    let (_dir, path) = temp_store_dir();
    let config = WalStoreConfig {
        snapshot_interval: 1000,
        keep_old_snapshots: 1,
        compaction_threshold: 10,
        machine_id: "test".to_string(),
    };

    let mut store = WalStore::open(&path, config).unwrap();

    // Write 100 entries
    for i in 0..100 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }

    // Take snapshot at current sequence (after 100 entries)
    store.create_snapshot().unwrap();

    // Write 50 more entries after snapshot
    for i in 100..150 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }

    // Get file size before compaction
    let wal_path = path.join("wal.jsonl");
    let old_size = std::fs::metadata(&wal_path).unwrap().len();

    // Compact
    let result = store.compact().unwrap();

    // Should have removed entries 0-99 (before snapshot)
    assert_eq!(result.entries_removed, 100);
    // Should have kept 50 entries + 1 snapshot marker
    assert_eq!(result.entries_kept, 51);
    assert!(result.bytes_reclaimed > 0);

    // New file should be smaller
    let new_size = std::fs::metadata(&wal_path).unwrap().len();
    assert!(new_size < old_size);
}

#[test]
fn compaction_is_atomic() {
    let (_dir, path) = temp_store_dir();
    let config = WalStoreConfig {
        snapshot_interval: 1000,
        keep_old_snapshots: 1,
        compaction_threshold: 10,
        machine_id: "test".to_string(),
    };

    let mut store = WalStore::open(&path, config.clone()).unwrap();

    // Write entries
    for i in 0..50 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }
    store.create_snapshot().unwrap();
    for i in 50..100 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }

    // Compact
    store.compact().unwrap();

    // Reopen and verify integrity
    let store2 = WalStore::open(&path, config).unwrap();

    // Should still have all 100 pipelines (state is preserved)
    let pipelines = store2.list_pipelines().unwrap();
    assert_eq!(pipelines.len(), 100);
}

#[test]
fn recovery_truncates_corrupted_wal() {
    use std::io::Write;

    let (_dir, path) = temp_store_dir();

    // Write valid entries
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();
        for i in 0..10 {
            let id = format!("p{}", i);
            store
                .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
                .unwrap();
        }
    }

    // Get file size before corruption
    let wal_path = path.join("wal.jsonl");
    let valid_size = std::fs::metadata(&wal_path).unwrap().len();

    // Corrupt the file by appending garbage
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        file.write_all(b"GARBAGE_DATA_HERE").unwrap();
    }

    // Verify file is now larger (corrupted)
    let corrupted_size = std::fs::metadata(&wal_path).unwrap().len();
    assert!(corrupted_size > valid_size);

    // Explicitly repair the WAL (simulating crash recovery)
    let bytes_removed = WalStore::repair_wal(&path).unwrap();
    assert!(bytes_removed > 0);

    // Now open should work with clean WAL
    let store = WalStore::open(&path, test_config()).unwrap();

    // All valid entries should be recovered
    let pipelines = store.list_pipelines().unwrap();
    assert_eq!(pipelines.len(), 10);

    // File should be truncated back to valid size
    let recovered_size = std::fs::metadata(&wal_path).unwrap().len();
    assert_eq!(recovered_size, valid_size);
}

#[test]
fn recovery_handles_partial_write() {
    let (_dir, path) = temp_store_dir();

    // Write valid entries
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();
        for i in 0..5 {
            let id = format!("p{}", i);
            store
                .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
                .unwrap();
        }
    }

    // Simulate partial write by truncating mid-entry
    {
        let wal_path = path.join("wal.jsonl");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&wal_path)
            .unwrap();
        let size = file.metadata().unwrap().len();
        file.set_len(size - 10).unwrap(); // Remove last 10 bytes (partial entry)
    }

    // Recovery should handle gracefully - should recover 4 of 5 entries
    let store = WalStore::open(&path, test_config()).unwrap();
    let pipelines = store.list_pipelines().unwrap();
    // Might recover 4 (if last entry was truncated) or 5 (if truncation was in newline)
    assert!(pipelines.len() >= 4);
}

#[test]
fn should_compact_respects_threshold() {
    let (_dir, path) = temp_store_dir();
    let config = WalStoreConfig {
        snapshot_interval: 1000,
        keep_old_snapshots: 1,
        compaction_threshold: 50,
        machine_id: "test".to_string(),
    };

    let mut store = WalStore::open(&path, config).unwrap();

    // No snapshot, no compaction
    assert!(!store.should_compact());

    // Write entries and snapshot
    for i in 0..30 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }
    store.create_snapshot().unwrap();

    // Below threshold (30 < 50)
    assert!(!store.should_compact());

    // Write more entries
    for i in 30..100 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }
    store.create_snapshot().unwrap();

    // Now above threshold (100 entries > 50 threshold)
    assert!(store.should_compact());
}

#[test]
fn writes_work_after_truncation() {
    use std::io::Write;

    let (_dir, path) = temp_store_dir();

    // Write, corrupt, recover
    {
        let mut store = WalStore::open(&path, test_config()).unwrap();
        for i in 0..5 {
            let id = format!("p{}", i);
            store
                .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
                .unwrap();
        }
    }
    {
        let wal_path = path.join("wal.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        file.write_all(b"CORRUPT").unwrap();
    }

    // Explicitly repair the WAL
    WalStore::repair_wal(&path).unwrap();

    // Recover and continue writing
    let mut store = WalStore::open(&path, test_config()).unwrap();
    for i in 5..10 {
        let id = format!("p{}", i);
        store
            .save_pipeline(&Pipeline::new_dynamic(&id, "t", BTreeMap::new()))
            .unwrap();
    }

    // Verify all entries (5 recovered + 5 new)
    let pipelines = store.list_pipelines().unwrap();
    assert_eq!(pipelines.len(), 10);

    // Reopen to verify persistence
    drop(store);
    let store2 = WalStore::open(&path, test_config()).unwrap();
    let pipelines2 = store2.list_pipelines().unwrap();
    assert_eq!(pipelines2.len(), 10);
}

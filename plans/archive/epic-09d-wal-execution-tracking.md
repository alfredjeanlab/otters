# Epic 9d: WAL Execution Tracking

**Root Feature:** `otters-4b49`

**Depends on**: None (can be done in parallel with 9a, 9c, 9e)
**Blocks**: Epic 9b (Engine Integration)

## Problem Statement

The Epic 8b plan specified WAL operations for execution tracking but they were not implemented:
- `ActionExecutionStarted` - Record when an action begins execution
- `ActionExecutionCompleted` - Record action outcome with duration
- `CleanupExecuted` - Record scanner cleanup operations

Without these, there's no audit trail for automated actions.

## Goal

Add WAL operations for tracking action execution and cleanup operations, enabling audit, debugging, and replay.

## Implementation

### 1. Add Operations to `crates/core/src/storage/wal/operation.rs`

```rust
// Add to Operation enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    // ... existing variants ...

    /// Action execution started
    ActionExecutionStarted(ActionExecutionStartedOp),
    /// Action execution completed
    ActionExecutionCompleted(ActionExecutionCompletedOp),
    /// Cleanup operation executed
    CleanupExecuted(CleanupExecutedOp),
}

/// Records the start of an action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionStartedOp {
    /// ID of the action being executed
    pub action_id: String,
    /// What triggered this execution (watcher ID, manual, etc.)
    pub source: String,
    /// Type of execution (command, task, rules)
    pub execution_type: String,
    /// Timestamp when execution started (epoch millis)
    pub started_at: u64,
}

/// Records the completion of an action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionCompletedOp {
    /// ID of the action that was executed
    pub action_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Command/task output if any
    pub output: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Timestamp when execution completed (epoch millis)
    pub completed_at: u64,
}

/// Records a cleanup operation from a scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupExecutedOp {
    /// ID of the scanner that triggered cleanup
    pub scanner_id: String,
    /// ID of the resource that was cleaned up
    pub resource_id: String,
    /// Type of cleanup action (delete, release, archive, etc.)
    pub action: String,
    /// Whether cleanup succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp when cleanup was executed (epoch millis)
    pub executed_at: u64,
}
```

### 2. Add State Tracking (Optional) to `crates/core/src/storage/wal/state.rs`

```rust
/// Execution history for auditing (ring buffer)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionHistory {
    /// Recent action executions (last N)
    pub action_executions: VecDeque<ActionExecutionRecord>,
    /// Recent cleanup operations (last N)
    pub cleanup_operations: VecDeque<CleanupRecord>,
    /// Maximum entries to keep
    #[serde(skip)]
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionRecord {
    pub action_id: String,
    pub source: String,
    pub execution_type: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRecord {
    pub scanner_id: String,
    pub resource_id: String,
    pub action: String,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: u64,
}

impl WalState {
    pub fn apply(&mut self, op: &Operation) {
        match op {
            // ... existing handlers ...
            Operation::ActionExecutionStarted(op) => { /* Track in in_flight map */ }
            Operation::ActionExecutionCompleted(op) => { /* Remove from in_flight, push to action_executions, trim */ }
            Operation::CleanupExecuted(op) => { /* Push to cleanup_operations, trim */ }
            // ... rest of match ...
        }
    }
}
```

### 3. Add Store Methods to `crates/core/src/storage/store.rs`

```rust
use crate::scheduling::{ActionId, ScannerId};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

impl WalStore {
    pub fn action_execution_started(&mut self, action_id: &ActionId, source: &str, execution_type: &str) -> Result<(), WalStoreError> {
        // Append ActionExecutionStarted operation with current timestamp
    }

    pub fn action_execution_completed(&mut self, action_id: &ActionId, success: bool, output: Option<String>, error: Option<String>, duration: Duration) -> Result<(), WalStoreError> {
        // Append ActionExecutionCompleted operation with current timestamp
    }

    pub fn cleanup_executed(&mut self, scanner_id: &ScannerId, resource_id: &str, action: &str, success: bool, error: Option<String>) -> Result<(), WalStoreError> {
        // Append CleanupExecuted operation with current timestamp
    }

    pub fn recent_action_executions(&self, limit: usize) -> Vec<&ActionExecutionRecord> {
        // Return last N from execution_history.action_executions
    }

    pub fn recent_cleanup_operations(&self, limit: usize) -> Vec<&CleanupRecord> {
        // Return last N from execution_history.cleanup_operations
    }

    pub fn action_stats(&self, action_id: &ActionId) -> ActionStats {
        // Filter executions by action_id, compute total/successes/failures/avg_duration
    }
}

#[derive(Debug, Clone)]
pub struct ActionStats {
    pub total_executions: usize,
    pub successes: usize,
    pub failures: usize,
    pub avg_duration_ms: u64,
}
```

### 4. Add CLI Reporting (Optional Enhancement)

```rust
// In crates/cli/src/commands/stats.rs
pub fn show_execution_stats(store: &WalStore) {
    // Print recent action executions with status, duration, errors
    // Print recent cleanup operations with status, resource, action
}
```

## Files

- `crates/core/src/storage/wal/operation.rs` - Add operation types
- `crates/core/src/storage/wal/state.rs` - Add state tracking (optional)
- `crates/core/src/storage/store.rs` - Add store methods
- `crates/core/src/storage/wal/operation_tests.rs` - Test serialization
- `crates/core/src/storage/store_tests.rs` - Test recording and querying

## Tests

```rust
#[test]
fn record_action_execution() {
    let mut store = WalStore::new_in_memory();
    let action_id = ActionId::new("notify");

    store.action_execution_started(&action_id, "watcher:idle-check", "command").unwrap();
    store.action_execution_completed(
        &action_id,
        true,
        Some("notified".into()),
        None,
        Duration::from_millis(150),
    ).unwrap();

    let recent = store.recent_action_executions(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].action_id, "notify");
    assert!(recent[0].success);
    assert_eq!(recent[0].duration_ms, 150);
}

#[test]
fn record_failed_action() {
    let mut store = WalStore::new_in_memory();
    let action_id = ActionId::new("deploy");

    store.action_execution_started(&action_id, "cron:nightly", "command").unwrap();
    store.action_execution_completed(
        &action_id,
        false,
        None,
        Some("connection refused".into()),
        Duration::from_millis(5000),
    ).unwrap();

    let stats = store.action_stats(&action_id);
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.failures, 1);
}

#[test]
fn record_cleanup_operation() {
    let mut store = WalStore::new_in_memory();
    let scanner_id = ScannerId::new("stale-locks");

    store.cleanup_executed(
        &scanner_id,
        "lock:deploy",
        "release",
        true,
        None,
    ).unwrap();

    let recent = store.recent_cleanup_operations(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].scanner_id, "stale-locks");
    assert_eq!(recent[0].resource_id, "lock:deploy");
    assert_eq!(recent[0].action, "release");
}

#[test]
fn operations_survive_replay() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write operations
    {
        let mut store = WalStore::open(&path).unwrap();
        let action_id = ActionId::new("test");
        store.action_execution_started(&action_id, "test", "command").unwrap();
        store.action_execution_completed(&action_id, true, None, None, Duration::from_millis(100)).unwrap();
    }

    // Reopen and verify replay
    {
        let store = WalStore::open(&path).unwrap();
        let recent = store.recent_action_executions(10);
        assert_eq!(recent.len(), 1);
        assert!(recent[0].success);
    }
}

#[test]
fn execution_history_bounded() {
    let mut store = WalStore::new_in_memory();
    store.state.execution_history.max_entries = 5;

    // Add more than max entries
    for i in 0..10 {
        let action_id = ActionId::new(&format!("action-{}", i));
        store.action_execution_completed(
            &action_id,
            true,
            None,
            None,
            Duration::from_millis(100),
        ).unwrap();
    }

    // Should only keep last 5
    let recent = store.recent_action_executions(100);
    assert_eq!(recent.len(), 5);
    assert_eq!(recent[0].action_id, "action-9"); // Most recent
}
```

## Landing Checklist

- [ ] `ActionExecutionStarted` operation serializes/deserializes correctly
- [ ] `ActionExecutionCompleted` operation serializes/deserializes correctly
- [ ] `CleanupExecuted` operation serializes/deserializes correctly
- [ ] Store methods record operations to WAL
- [ ] Operations replay correctly after reopen
- [ ] Execution history is bounded (doesn't grow unbounded)
- [ ] Query methods return correct data
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

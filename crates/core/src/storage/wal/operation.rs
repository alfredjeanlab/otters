// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WAL operation types
//!
//! All state-changing operations in the system are represented as typed operations.
//! These operations form the source of truth for the WAL.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// All state-changing operations in the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    // Pipeline operations
    PipelineCreate(PipelineCreateOp),
    PipelineTransition(PipelineTransitionOp),
    PipelineDelete(PipelineDeleteOp),

    // Task operations
    TaskCreate(TaskCreateOp),
    TaskTransition(TaskTransitionOp),
    TaskDelete(TaskDeleteOp),

    // Workspace operations
    WorkspaceCreate(WorkspaceCreateOp),
    WorkspaceTransition(WorkspaceTransitionOp),
    WorkspaceDelete(WorkspaceDeleteOp),

    // Queue operations
    QueuePush(QueuePushOp),
    QueueClaim(QueueClaimOp),
    QueueComplete(QueueCompleteOp),
    QueueFail(QueueFailOp),
    QueueRelease(QueueReleaseOp),
    QueueDelete(QueueDeleteOp),
    /// Tick queue to handle visibility timeouts
    QueueTick(QueueTickOp),

    // Lock operations
    LockAcquire(LockAcquireOp),
    LockRelease(LockReleaseOp),
    LockHeartbeat(LockHeartbeatOp),

    // Semaphore operations
    SemaphoreAcquire(SemaphoreAcquireOp),
    SemaphoreRelease(SemaphoreReleaseOp),
    SemaphoreHeartbeat(SemaphoreHeartbeatOp),

    // Session operations
    SessionCreate(SessionCreateOp),
    SessionTransition(SessionTransitionOp),
    SessionHeartbeat(SessionHeartbeatOp),
    SessionDelete(SessionDeleteOp),

    // Event operations (events now durable)
    EventEmit(EventEmitOp),

    // Cron operations
    CronCreate(CronCreateOp),
    CronTransition(CronTransitionOp),
    CronDelete(CronDeleteOp),

    // Action operations
    ActionCreate(ActionCreateOp),
    ActionTransition(ActionTransitionOp),
    ActionDelete(ActionDeleteOp),

    // Watcher operations
    WatcherCreate(WatcherCreateOp),
    WatcherTransition(WatcherTransitionOp),
    WatcherDelete(WatcherDeleteOp),

    // Scanner operations
    ScannerCreate(ScannerCreateOp),
    ScannerTransition(ScannerTransitionOp),
    ScannerDelete(ScannerDeleteOp),

    // Execution tracking operations
    ActionExecutionStarted(ActionExecutionStartedOp),
    ActionExecutionCompleted(ActionExecutionCompletedOp),
    CleanupExecuted(CleanupExecutedOp),

    // Snapshot marker (for compaction)
    SnapshotTaken {
        snapshot_id: String,
    },
}

// Pipeline operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineCreateOp {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub workspace_id: Option<String>,
    pub inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub created_at_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineTransitionOp {
    pub id: String,
    pub from_phase: String,
    pub to_phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_waiting_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_guard_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineDeleteOp {
    pub id: String,
}

// Task operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCreateOp {
    pub id: String,
    pub pipeline_id: String,
    pub phase: String,
    pub heartbeat_interval_secs: u64,
    pub stuck_threshold_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudge_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDeleteOp {
    pub id: String,
}

// Workspace operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCreateOp {
    pub id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub state: String,
    pub created_at_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDeleteOp {
    pub id: String,
}

// Queue operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuePushOp {
    pub queue_name: String,
    pub item_id: String,
    pub data: BTreeMap<String, String>,
    pub priority: i32,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueClaimOp {
    pub queue_name: String,
    pub item_id: String,
    pub claim_id: String,
    pub visibility_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueCompleteOp {
    pub queue_name: String,
    pub claim_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueFailOp {
    pub queue_name: String,
    pub claim_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueReleaseOp {
    pub queue_name: String,
    pub claim_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueDeleteOp {
    pub queue_name: String,
}

/// Tick queue to handle visibility timeouts (bulk state change from tick)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueTickOp {
    pub queue_name: String,
    /// JSON-serialized result of the tick (expired items, dead-lettered items)
    pub tick_result_json: String,
}

// Lock operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockAcquireOp {
    pub lock_name: String,
    pub holder_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockReleaseOp {
    pub lock_name: String,
    pub holder_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockHeartbeatOp {
    pub lock_name: String,
    pub holder_id: String,
}

// Semaphore operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemaphoreAcquireOp {
    pub semaphore_name: String,
    pub holder_id: String,
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemaphoreReleaseOp {
    pub semaphore_name: String,
    pub holder_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemaphoreHeartbeatOp {
    pub semaphore_name: String,
    pub holder_id: String,
}

// Session operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCreateOp {
    pub id: String,
    pub workspace_id: String,
    pub idle_threshold_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHeartbeatOp {
    pub id: String,
    pub timestamp_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDeleteOp {
    pub id: String,
}

// Event operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEmitOp {
    pub event_type: String,
    pub payload: serde_json::Value,
}

// Cron operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronCreateOp {
    pub id: String,
    pub name: String,
    pub interval_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronDeleteOp {
    pub id: String,
}

// Action operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCreateOp {
    pub id: String,
    pub name: String,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDeleteOp {
    pub id: String,
}

// Watcher operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherCreateOp {
    pub id: String,
    pub name: String,
    pub source_json: String,
    pub condition_json: String,
    pub response_chain_json: String,
    pub check_interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consecutive_triggers: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherDeleteOp {
    pub id: String,
}

// Scanner operations

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannerCreateOp {
    pub id: String,
    pub name: String,
    pub source_json: String,
    pub condition_json: String,
    pub cleanup_action_json: String,
    pub scan_interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannerTransitionOp {
    pub id: String,
    pub from_state: String,
    pub to_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cleaned: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannerDeleteOp {
    pub id: String,
}

// Execution tracking operations

/// Records the start of an action execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionExecutionCompletedOp {
    /// ID of the action that was executed
    pub action_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Command/task output if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message if failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Timestamp when execution completed (epoch millis)
    pub completed_at: u64,
}

/// Records a cleanup operation from a scanner
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Timestamp when cleanup was executed (epoch millis)
    pub executed_at: u64,
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;

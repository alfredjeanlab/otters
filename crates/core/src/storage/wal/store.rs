// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WAL-based storage with crash recovery
//!
//! WalStore provides durable state persistence using a write-ahead log
//! with periodic snapshots for fast recovery.

use super::operation::*;
use super::reader::WalReader;
use super::snapshot::{SnapshotError, SnapshotManager, SnapshotMeta};
use super::state::{ActionExecutionRecord, ApplyError, CleanupRecord, MaterializedState};
use super::writer::WalWriter;
use crate::pipeline::{Phase, Pipeline, PipelineId};
use crate::queue::Queue;
use crate::scheduling::{ActionId, ScannerId};
use crate::task::{Task, TaskId, TaskState};
use crate::workspace::{Workspace, WorkspaceId, WorkspaceState};
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Errors from WalStore operations
#[derive(Debug, Error)]
pub enum WalStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("WAL read error: {0}")]
    WalRead(#[from] super::reader::WalReadError),
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
    #[error("apply error: {0}")]
    Apply(#[from] ApplyError),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("entity not found: {kind} {id}")]
    NotFound { kind: &'static str, id: String },
}

/// Configuration for WalStore
#[derive(Debug, Clone)]
pub struct WalStoreConfig {
    /// Number of operations between automatic snapshots
    pub snapshot_interval: u64,
    /// Number of old snapshots to keep after compaction
    pub keep_old_snapshots: usize,
    /// Number of entries before snapshot to trigger compaction
    pub compaction_threshold: u64,
    /// Machine ID for WAL entries
    pub machine_id: String,
}

impl Default for WalStoreConfig {
    fn default() -> Self {
        Self {
            snapshot_interval: 1000,
            keep_old_snapshots: 2,
            compaction_threshold: 10_000,
            machine_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// Execution statistics for an action
#[derive(Debug, Clone)]
pub struct ActionStats {
    pub total_executions: usize,
    pub successes: usize,
    pub failures: usize,
    pub avg_duration_ms: u64,
}

/// WAL-based storage with automatic recovery and snapshots
pub struct WalStore {
    config: WalStoreConfig,
    base_dir: PathBuf,
    wal_path: PathBuf,
    writer: WalWriter,
    snapshots: SnapshotManager,
    state: MaterializedState,
    last_snapshot_sequence: Option<u64>,
    first_sequence: u64,
    ops_since_snapshot: u64,
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Number of entries removed
    pub entries_removed: usize,
    /// Number of entries kept
    pub entries_kept: usize,
    /// Bytes reclaimed from disk
    pub bytes_reclaimed: u64,
}

impl WalStore {
    /// Open or create a WalStore at the given directory
    pub fn open(base_dir: &Path, config: WalStoreConfig) -> Result<Self, WalStoreError> {
        std::fs::create_dir_all(base_dir)?;

        let wal_path = base_dir.join("wal.jsonl");
        let snapshots_dir = base_dir.join("snapshots");

        let snapshots = SnapshotManager::new(&snapshots_dir);
        let mut state = MaterializedState::new();
        let mut last_snapshot_sequence: Option<u64> = None;
        let mut first_sequence: u64 = 0;

        // Try to load from latest snapshot
        if let Ok(Some(snapshot_meta)) = snapshots.latest_snapshot() {
            if let Ok(storable) = snapshots.load_snapshot(&snapshot_meta.id) {
                state = storable.to_materialized();
                last_snapshot_sequence = Some(storable.sequence_at_snapshot);
            }
        }

        // Replay WAL entries after the snapshot (or from beginning if no snapshot)
        let reader = WalReader::open_or_empty(&wal_path)?;
        let start_sequence = last_snapshot_sequence.map(|s| s + 1).unwrap_or(0);
        let mut iter = reader.entries_from(start_sequence)?;
        let mut first_entry_seen = false;
        let mut last_valid_position: u64 = 0;
        let mut had_corruption = false;

        while let Some(entry_result) = iter.next() {
            match entry_result {
                Ok(entry) => {
                    if !first_entry_seen {
                        first_sequence = entry.sequence;
                        first_entry_seen = true;
                    }
                    // Ignore apply errors during recovery - they indicate
                    // corrupted state that will be overwritten
                    let _ = state.apply(&entry.operation, entry.timestamp_micros);
                    last_valid_position = iter.last_valid_position();
                }
                Err(e) => {
                    // Stop at first read error (truncated entry, corruption)
                    tracing::warn!(?e, "stopping WAL replay due to read error");
                    had_corruption = true;
                    break;
                }
            }
        }

        // Note: We intentionally do NOT automatically truncate the WAL here.
        // Automatic truncation is dangerous in concurrent access scenarios.
        // Instead, we log the corruption and let the caller decide whether to
        // call repair_wal() explicitly for crash recovery.
        if had_corruption {
            tracing::warn!(
                last_valid_position,
                "WAL corruption detected but not auto-truncated; call repair_wal() for recovery"
            );
        }

        let writer = WalWriter::open(&wal_path, &config.machine_id)?;

        Ok(Self {
            config,
            base_dir: base_dir.to_path_buf(),
            wal_path,
            writer,
            snapshots,
            state,
            last_snapshot_sequence,
            first_sequence,
            ops_since_snapshot: 0,
        })
    }

    /// Truncate WAL file at the given position
    fn truncate_wal_file(wal_path: &Path, position: u64) -> Result<(), WalStoreError> {
        use std::fs::OpenOptions;

        let file = OpenOptions::new()
            .write(true)
            .open(wal_path)
            .map_err(WalStoreError::Io)?;

        file.set_len(position).map_err(WalStoreError::Io)?;

        file.sync_all().map_err(WalStoreError::Io)?;

        tracing::info!(position, "WAL truncated at corruption point");

        Ok(())
    }

    /// Repair a WAL file by truncating at the first corruption point.
    ///
    /// This should be called during explicit crash recovery, not during normal
    /// operation. It scans the WAL, finds the last valid entry, and truncates
    /// any corrupted data after it.
    ///
    /// Returns the number of bytes removed, or 0 if no corruption was found.
    pub fn repair_wal(base_dir: &Path) -> Result<u64, WalStoreError> {
        let wal_path = base_dir.join("wal.jsonl");

        if !wal_path.exists() {
            return Ok(0);
        }

        let reader = WalReader::open_or_empty(&wal_path)?;
        let mut iter = reader.entries_from(0)?;
        let mut last_valid_position: u64 = 0;
        let mut had_corruption = false;

        while let Some(entry_result) = iter.next() {
            match entry_result {
                Ok(_entry) => {
                    last_valid_position = iter.last_valid_position();
                }
                Err(e) => {
                    tracing::warn!(?e, "WAL corruption detected during repair");
                    had_corruption = true;
                    break;
                }
            }
        }

        if !had_corruption {
            return Ok(0);
        }

        let old_size = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);

        if last_valid_position > 0 && last_valid_position < old_size {
            Self::truncate_wal_file(&wal_path, last_valid_position)?;
            Ok(old_size - last_valid_position)
        } else if last_valid_position == 0 {
            // All entries are corrupt, truncate to empty
            Self::truncate_wal_file(&wal_path, 0)?;
            Ok(old_size)
        } else {
            Ok(0)
        }
    }

    /// Open a WalStore with default configuration
    pub fn open_default(base_dir: &Path) -> Result<Self, WalStoreError> {
        Self::open(base_dir, WalStoreConfig::default())
    }

    /// Create a WalStore in a temporary directory (for testing)
    pub fn open_temp() -> Result<Self, WalStoreError> {
        let temp_dir =
            std::env::temp_dir().join(format!("oj-walstore-test-{}", uuid::Uuid::new_v4()));
        Self::open_default(&temp_dir)
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get the current sequence number
    pub fn sequence(&self) -> u64 {
        self.writer.sequence()
    }

    // === Pipeline Operations ===

    /// Save a pipeline (create or update)
    pub fn save_pipeline(&mut self, pipeline: &Pipeline) -> Result<(), WalStoreError> {
        if let Some(old) = self.state.pipeline(&pipeline.id) {
            // Transition
            let old = old.clone();
            let op = Operation::PipelineTransition(PipelineTransitionOp {
                id: pipeline.id.0.clone(),
                from_phase: format!("{:?}", old.phase),
                to_phase: phase_to_string(&pipeline.phase),
                workspace_id: if pipeline.workspace_id != old.workspace_id {
                    pipeline.workspace_id.as_ref().map(|id| id.0.clone())
                } else {
                    None
                },
                outputs: if pipeline.outputs != old.outputs {
                    Some(pipeline.outputs.clone())
                } else {
                    None
                },
                current_task_id: pipeline.current_task_id.as_ref().map(|id| id.0.clone()),
                failed_reason: if let Phase::Failed { reason } = &pipeline.phase {
                    Some(reason.clone())
                } else {
                    None
                },
                blocked_waiting_on: if let Phase::Blocked { waiting_on, .. } = &pipeline.phase {
                    Some(waiting_on.clone())
                } else {
                    None
                },
                blocked_guard_id: if let Phase::Blocked { guard_id, .. } = &pipeline.phase {
                    guard_id.clone()
                } else {
                    None
                },
            });
            self.append_operation(op)?;
        } else {
            // Create
            let op = Operation::PipelineCreate(PipelineCreateOp {
                id: pipeline.id.0.clone(),
                kind: format!("{:?}", pipeline.kind),
                name: pipeline.name.clone(),
                workspace_id: pipeline.workspace_id.as_ref().map(|id| id.0.clone()),
                inputs: pipeline.inputs.clone(),
                outputs: pipeline.outputs.clone(),
                created_at_micros: pipeline.created_at.timestamp_micros(),
            });
            self.append_operation(op)?;
        }

        Ok(())
    }

    /// Load a pipeline by ID
    pub fn load_pipeline(&self, id: &PipelineId) -> Result<Pipeline, WalStoreError> {
        self.state
            .pipeline(id)
            .cloned()
            .ok_or_else(|| WalStoreError::NotFound {
                kind: "pipeline",
                id: id.0.clone(),
            })
    }

    /// List all pipeline IDs
    pub fn list_pipelines(&self) -> Result<Vec<PipelineId>, WalStoreError> {
        Ok(self.state.pipelines.keys().cloned().collect())
    }

    /// Delete a pipeline
    pub fn delete_pipeline(&mut self, id: &PipelineId) -> Result<(), WalStoreError> {
        if self.state.pipeline(id).is_none() {
            return Err(WalStoreError::NotFound {
                kind: "pipeline",
                id: id.0.clone(),
            });
        }

        let op = Operation::PipelineDelete(PipelineDeleteOp { id: id.0.clone() });
        self.append_operation(op)?;
        Ok(())
    }

    // === Task Operations ===

    /// Save a task (create or update)
    pub fn save_task(&mut self, task: &Task) -> Result<(), WalStoreError> {
        if let Some(old) = self.state.task(&task.id) {
            // Transition
            let old = old.clone();
            let op = Operation::TaskTransition(TaskTransitionOp {
                id: task.id.0.clone(),
                from_state: task_state_to_string(&old.state),
                to_state: task_state_to_string(&task.state),
                session_id: task.session_id.as_ref().map(|id| id.0.clone()),
                output: if let TaskState::Done { output } = &task.state {
                    output.clone()
                } else {
                    None
                },
                failed_reason: if let TaskState::Failed { reason } = &task.state {
                    Some(reason.clone())
                } else {
                    None
                },
                nudge_count: if let TaskState::Stuck { nudge_count, .. } = &task.state {
                    Some(*nudge_count)
                } else {
                    None
                },
            });
            self.append_operation(op)?;
        } else {
            // Create
            let op = Operation::TaskCreate(TaskCreateOp {
                id: task.id.0.clone(),
                pipeline_id: task.pipeline_id.0.clone(),
                phase: task.phase.clone(),
                heartbeat_interval_secs: task.heartbeat_interval.as_secs(),
                stuck_threshold_secs: task.stuck_threshold.as_secs(),
            });
            self.append_operation(op)?;
        }

        Ok(())
    }

    /// Load a task by ID
    pub fn load_task(&self, id: &TaskId) -> Result<Task, WalStoreError> {
        self.state
            .task(id)
            .cloned()
            .ok_or_else(|| WalStoreError::NotFound {
                kind: "task",
                id: id.0.clone(),
            })
    }

    /// List all task IDs
    pub fn list_tasks(&self) -> Result<Vec<TaskId>, WalStoreError> {
        Ok(self.state.tasks.keys().cloned().collect())
    }

    /// Delete a task
    pub fn delete_task(&mut self, id: &TaskId) -> Result<(), WalStoreError> {
        if self.state.task(id).is_none() {
            return Err(WalStoreError::NotFound {
                kind: "task",
                id: id.0.clone(),
            });
        }

        let op = Operation::TaskDelete(TaskDeleteOp { id: id.0.clone() });
        self.append_operation(op)?;
        Ok(())
    }

    // === Workspace Operations ===

    /// Save a workspace (create or update)
    pub fn save_workspace(&mut self, workspace: &Workspace) -> Result<(), WalStoreError> {
        if let Some(old) = self.state.workspace(&workspace.id) {
            // Transition
            let old = old.clone();
            let op = Operation::WorkspaceTransition(WorkspaceTransitionOp {
                id: workspace.id.0.clone(),
                from_state: workspace_state_to_string(&old.state),
                to_state: workspace_state_to_string(&workspace.state),
                session_id: if let WorkspaceState::InUse { session_id } = &workspace.state {
                    Some(session_id.clone())
                } else {
                    None
                },
            });
            self.append_operation(op)?;
        } else {
            // Create
            let op = Operation::WorkspaceCreate(WorkspaceCreateOp {
                id: workspace.id.0.clone(),
                name: workspace.name.clone(),
                path: workspace.path.to_string_lossy().to_string(),
                branch: workspace.branch.clone(),
                state: workspace_state_to_string(&workspace.state),
                created_at_micros: workspace.created_at.timestamp_micros(),
            });
            self.append_operation(op)?;
        }

        Ok(())
    }

    /// Load a workspace by ID
    pub fn load_workspace(&self, id: &WorkspaceId) -> Result<Workspace, WalStoreError> {
        self.state
            .workspace(id)
            .cloned()
            .ok_or_else(|| WalStoreError::NotFound {
                kind: "workspace",
                id: id.0.clone(),
            })
    }

    /// List all workspace IDs
    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceId>, WalStoreError> {
        Ok(self.state.workspaces.keys().cloned().collect())
    }

    /// Delete a workspace
    pub fn delete_workspace(&mut self, id: &WorkspaceId) -> Result<(), WalStoreError> {
        if self.state.workspace(id).is_none() {
            return Err(WalStoreError::NotFound {
                kind: "workspace",
                id: id.0.clone(),
            });
        }

        let op = Operation::WorkspaceDelete(WorkspaceDeleteOp { id: id.0.clone() });
        self.append_operation(op)?;
        Ok(())
    }

    // === Queue Operations ===

    /// Get a queue by name
    pub fn queue(&self, name: &str) -> Option<&Queue> {
        self.state.queue(name)
    }

    /// Load a queue by name (creates empty queue if not found)
    pub fn load_queue(&self, name: &str) -> Result<Queue, WalStoreError> {
        Ok(self
            .state
            .queue(name)
            .cloned()
            .unwrap_or_else(|| Queue::new(name)))
    }

    /// Push an item to a queue
    pub fn queue_push(
        &mut self,
        queue_name: &str,
        item_id: &str,
        data: BTreeMap<String, String>,
        priority: i32,
        max_attempts: u32,
    ) -> Result<(), WalStoreError> {
        let op = Operation::QueuePush(QueuePushOp {
            queue_name: queue_name.to_string(),
            item_id: item_id.to_string(),
            data,
            priority,
            max_attempts,
        });
        self.append_operation(op)?;
        Ok(())
    }

    /// Claim an item from a queue for processing
    pub fn queue_claim(
        &mut self,
        queue_name: &str,
        item_id: &str,
        claim_id: &str,
        visibility_timeout_secs: u64,
    ) -> Result<(), WalStoreError> {
        let op = Operation::QueueClaim(QueueClaimOp {
            queue_name: queue_name.to_string(),
            item_id: item_id.to_string(),
            claim_id: claim_id.to_string(),
            visibility_timeout_secs,
        });
        self.append_operation(op)?;
        Ok(())
    }

    /// Complete a claimed item (remove from queue successfully)
    pub fn queue_complete(
        &mut self,
        queue_name: &str,
        claim_id: &str,
    ) -> Result<(), WalStoreError> {
        let op = Operation::QueueComplete(QueueCompleteOp {
            queue_name: queue_name.to_string(),
            claim_id: claim_id.to_string(),
        });
        self.append_operation(op)?;
        Ok(())
    }

    /// Fail a claimed item (requeue or dead-letter based on attempts)
    pub fn queue_fail(
        &mut self,
        queue_name: &str,
        claim_id: &str,
        reason: &str,
    ) -> Result<(), WalStoreError> {
        let op = Operation::QueueFail(QueueFailOp {
            queue_name: queue_name.to_string(),
            claim_id: claim_id.to_string(),
            reason: reason.to_string(),
        });
        self.append_operation(op)?;
        Ok(())
    }

    /// Release a claimed item back to the queue without processing
    pub fn queue_release(&mut self, queue_name: &str, claim_id: &str) -> Result<(), WalStoreError> {
        let op = Operation::QueueRelease(QueueReleaseOp {
            queue_name: queue_name.to_string(),
            claim_id: claim_id.to_string(),
        });
        self.append_operation(op)?;
        Ok(())
    }

    /// Tick a queue to handle visibility timeouts
    ///
    /// This applies the Tick event to the queue and persists the resulting state.
    /// Returns the effects generated by the tick (e.g., expired items, dead-lettered items).
    pub fn queue_tick(
        &mut self,
        queue_name: &str,
        clock: &impl crate::clock::Clock,
    ) -> Result<Vec<crate::effect::Effect>, WalStoreError> {
        let queue = self.load_queue(queue_name)?;
        let (new_queue, effects) = queue.transition(crate::queue::QueueEvent::Tick, clock);

        // Only persist if there were changes
        if !effects.is_empty() {
            let tick_result_json = serde_json::to_string(&new_queue)?;
            let op = Operation::QueueTick(QueueTickOp {
                queue_name: queue_name.to_string(),
                tick_result_json,
            });
            self.append_operation(op)?;
        }

        Ok(effects)
    }

    // === Session Operations ===

    /// Record a session heartbeat
    pub fn session_heartbeat(&mut self, session_id: &str) -> Result<(), WalStoreError> {
        let timestamp_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);

        self.append_operation(Operation::SessionHeartbeat(SessionHeartbeatOp {
            id: session_id.to_string(),
            timestamp_micros,
        }))?;

        Ok(())
    }

    // === Execution Tracking Operations ===

    /// Record the start of an action execution
    pub fn action_execution_started(
        &mut self,
        action_id: &ActionId,
        source: &str,
        execution_type: &str,
    ) -> Result<(), WalStoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.append_operation(Operation::ActionExecutionStarted(
            ActionExecutionStartedOp {
                action_id: action_id.0.clone(),
                source: source.to_string(),
                execution_type: execution_type.to_string(),
                started_at: now,
            },
        ))?;

        Ok(())
    }

    /// Record the completion of an action execution
    pub fn action_execution_completed(
        &mut self,
        action_id: &ActionId,
        success: bool,
        output: Option<String>,
        error: Option<String>,
        duration: Duration,
    ) -> Result<(), WalStoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.append_operation(Operation::ActionExecutionCompleted(
            ActionExecutionCompletedOp {
                action_id: action_id.0.clone(),
                success,
                output,
                error,
                duration_ms: duration.as_millis() as u64,
                completed_at: now,
            },
        ))?;

        Ok(())
    }

    /// Record a cleanup operation
    pub fn cleanup_executed(
        &mut self,
        scanner_id: &ScannerId,
        resource_id: &str,
        action: &str,
        success: bool,
        error: Option<String>,
    ) -> Result<(), WalStoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.append_operation(Operation::CleanupExecuted(CleanupExecutedOp {
            scanner_id: scanner_id.0.clone(),
            resource_id: resource_id.to_string(),
            action: action.to_string(),
            success,
            error,
            executed_at: now,
        }))?;

        Ok(())
    }

    /// Get recent action executions
    pub fn recent_action_executions(&self, limit: usize) -> Vec<&ActionExecutionRecord> {
        self.state
            .execution_history()
            .action_executions
            .iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Get recent cleanup operations
    pub fn recent_cleanup_operations(&self, limit: usize) -> Vec<&CleanupRecord> {
        self.state
            .execution_history()
            .cleanup_operations
            .iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Get execution stats for an action
    pub fn action_stats(&self, action_id: &ActionId) -> ActionStats {
        let executions: Vec<_> = self
            .state
            .execution_history()
            .action_executions
            .iter()
            .filter(|e| e.action_id == action_id.0)
            .collect();

        let total = executions.len();
        let successes = executions.iter().filter(|e| e.success).count();
        let failures = total - successes;
        let avg_duration = if total > 0 {
            executions.iter().map(|e| e.duration_ms).sum::<u64>() / total as u64
        } else {
            0
        };

        ActionStats {
            total_executions: total,
            successes,
            failures,
            avg_duration_ms: avg_duration,
        }
    }

    // === Snapshot Operations ===

    /// Create a snapshot of current state
    pub fn create_snapshot(&mut self) -> Result<SnapshotMeta, WalStoreError> {
        let sequence = self.writer.last_sequence().unwrap_or(0);
        let meta = self.snapshots.create_snapshot(&self.state, sequence)?;

        // Record snapshot in WAL
        let op = Operation::SnapshotTaken {
            snapshot_id: meta.id.clone(),
        };
        self.writer.append(op)?;

        self.last_snapshot_sequence = Some(sequence);
        self.ops_since_snapshot = 0;

        Ok(meta)
    }

    /// Create snapshot if needed based on config interval
    pub fn maybe_snapshot(&mut self) -> Result<Option<SnapshotMeta>, WalStoreError> {
        if self.ops_since_snapshot >= self.config.snapshot_interval {
            Ok(Some(self.create_snapshot()?))
        } else {
            Ok(None)
        }
    }

    /// Compact the WAL by removing entries before the last snapshot.
    ///
    /// This rewrites the WAL file to contain only entries after the most
    /// recent snapshot, then atomically replaces the old file.
    pub fn compact(&mut self) -> Result<CompactionResult, WalStoreError> {
        let Some(snapshot_seq) = self.last_snapshot_sequence else {
            // No snapshot to compact from
            return Ok(CompactionResult {
                entries_removed: 0,
                entries_kept: 0,
                bytes_reclaimed: 0,
            });
        };

        // Get current file size before compaction
        let old_size = std::fs::metadata(&self.wal_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Collect entries to keep (after snapshot)
        let reader = WalReader::open_or_empty(&self.wal_path)?;
        let entries_to_keep: Vec<_> = reader
            .entries_from(snapshot_seq + 1)?
            .filter_map(|r| r.ok())
            .collect();

        // entries_removed = entries from first_sequence through snapshot_seq (inclusive)
        let entries_removed = (snapshot_seq - self.first_sequence + 1) as usize;

        if entries_to_keep.is_empty() && entries_removed == 0 {
            // Nothing to compact
            return Ok(CompactionResult {
                entries_removed: 0,
                entries_kept: 0,
                bytes_reclaimed: 0,
            });
        }

        // Write to temporary file, preserving original sequence numbers
        let temp_path = self.wal_path.with_extension("wal.compact.tmp");
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&temp_path)?;

            for entry in &entries_to_keep {
                // Write the original entry directly to preserve sequence numbers
                let line = entry.to_line()?;
                file.write_all(line.as_bytes())?;
                file.write_all(b"\n")?;
            }
            file.sync_all()?;
        }

        // Atomic replace (rename is atomic on POSIX)
        std::fs::rename(&temp_path, &self.wal_path)?;

        // Reopen writer
        self.writer = WalWriter::open(&self.wal_path, &self.config.machine_id)?;

        // Update first sequence
        if let Some(first) = entries_to_keep.first() {
            self.first_sequence = first.sequence;
        }

        // Get new file size
        let new_size = std::fs::metadata(&self.wal_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let bytes_reclaimed = old_size.saturating_sub(new_size);

        // Clean old snapshots
        self.snapshots
            .cleanup_old_snapshots(snapshot_seq, self.config.keep_old_snapshots)?;

        tracing::info!(
            entries_removed,
            entries_kept = entries_to_keep.len(),
            bytes_reclaimed,
            "WAL compacted"
        );

        Ok(CompactionResult {
            entries_removed,
            entries_kept: entries_to_keep.len(),
            bytes_reclaimed,
        })
    }

    /// Check if compaction is recommended
    pub fn should_compact(&self) -> bool {
        let Some(snapshot_seq) = self.last_snapshot_sequence else {
            return false;
        };

        // Compact if we have more than threshold entries before snapshot
        let entries_before_snapshot = snapshot_seq.saturating_sub(self.first_sequence);
        entries_before_snapshot > self.config.compaction_threshold
    }

    /// Compact if recommended
    pub fn maybe_compact(&mut self) -> Result<Option<CompactionResult>, WalStoreError> {
        if self.should_compact() {
            Ok(Some(self.compact()?))
        } else {
            Ok(None)
        }
    }

    /// Get materialized state (for read operations)
    pub fn state(&self) -> &MaterializedState {
        &self.state
    }

    // === Internal Operations ===

    /// Append an operation to the WAL and apply to state
    fn append_operation(&mut self, op: Operation) -> Result<u64, WalStoreError> {
        let sequence = self.writer.append(op.clone())?;

        let timestamp_micros = Utc::now().timestamp_micros() as u64;
        self.state.apply(&op, timestamp_micros)?;

        self.ops_since_snapshot += 1;

        Ok(sequence)
    }
}

// Helper functions

fn phase_to_string(phase: &Phase) -> String {
    match phase {
        Phase::Init => "init".to_string(),
        Phase::Plan => "plan".to_string(),
        Phase::Decompose => "decompose".to_string(),
        Phase::Execute => "execute".to_string(),
        Phase::Fix => "fix".to_string(),
        Phase::Verify => "verify".to_string(),
        Phase::Merge => "merge".to_string(),
        Phase::Cleanup => "cleanup".to_string(),
        Phase::Done => "done".to_string(),
        Phase::Failed { .. } => "failed".to_string(),
        Phase::Blocked { .. } => "blocked".to_string(),
    }
}

fn task_state_to_string(state: &TaskState) -> String {
    match state {
        TaskState::Pending => "pending".to_string(),
        TaskState::Running => "running".to_string(),
        TaskState::Stuck { .. } => "stuck".to_string(),
        TaskState::Done { .. } => "done".to_string(),
        TaskState::Failed { .. } => "failed".to_string(),
    }
}

fn workspace_state_to_string(state: &WorkspaceState) -> String {
    match state {
        WorkspaceState::Creating => "creating".to_string(),
        WorkspaceState::Ready => "ready".to_string(),
        WorkspaceState::InUse { .. } => "in_use".to_string(),
        WorkspaceState::Dirty => "dirty".to_string(),
        WorkspaceState::Stale => "stale".to_string(),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

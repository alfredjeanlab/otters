// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Snapshot management for WAL compaction
//!
//! Snapshots provide point-in-time state captures that enable WAL truncation
//! and fast recovery.

use super::state::{MaterializedState, StoredEvent};
use crate::clock::{Clock, SystemClock};
use crate::coordination::StorableCoordinationState;
use crate::pipeline::Pipeline;
use crate::queue::{DeadLetter, Queue, QueueItem};
use crate::session::{DeathReason, Session, SessionId, SessionState};
use crate::task::Task;
use crate::workspace::{Workspace, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during snapshot operations
#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("snapshot not found: {0}")]
    NotFound(String),
    #[error("invalid snapshot format: {0}")]
    InvalidFormat(String),
}

/// Serializable version of the full system state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableState {
    pub version: u32,
    pub sequence_at_snapshot: u64,
    pub timestamp: DateTime<Utc>,
    pub pipelines: Vec<StorablePipeline>,
    pub tasks: Vec<StorableTask>,
    pub workspaces: Vec<StorableWorkspace>,
    pub queues: Vec<StorableQueue>,
    pub coordination: StorableCoordinationState,
    pub events: Vec<StorableEvent>,
    #[serde(default)]
    pub sessions: Vec<StorableSession>,
}

impl StorableState {
    /// Current version of the snapshot format
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a storable state from a materialized state
    pub fn from_materialized(state: &MaterializedState, sequence: u64) -> Self {
        let clock = SystemClock;

        Self {
            version: Self::CURRENT_VERSION,
            sequence_at_snapshot: sequence,
            timestamp: Utc::now(),
            pipelines: state.all_pipelines().map(StorablePipeline::from).collect(),
            tasks: state.all_tasks().map(StorableTask::from).collect(),
            workspaces: state
                .all_workspaces()
                .map(StorableWorkspace::from)
                .collect(),
            queues: state
                .all_queues()
                .map(|(name, q)| StorableQueue::from_queue(name, q))
                .collect(),
            coordination: StorableCoordinationState::from_manager(state.coordination(), &clock),
            events: state
                .recent_events()
                .iter()
                .map(StorableEvent::from)
                .collect(),
            sessions: state
                .sessions
                .values()
                .map(StorableSession::from_session)
                .collect(),
        }
    }

    /// Convert to a materialized state
    pub fn to_materialized(&self) -> MaterializedState {
        let mut state = MaterializedState::new();

        // Restore pipelines
        for sp in &self.pipelines {
            state
                .pipelines
                .insert(crate::pipeline::PipelineId(sp.id.clone()), sp.to_pipeline());
        }

        // Restore tasks
        for st in &self.tasks {
            state
                .tasks
                .insert(crate::task::TaskId(st.id.clone()), st.to_task());
        }

        // Restore workspaces
        for sw in &self.workspaces {
            state.workspaces.insert(
                crate::workspace::WorkspaceId(sw.id.clone()),
                sw.to_workspace(),
            );
        }

        // Restore queues
        for sq in &self.queues {
            state.queues.insert(sq.name.clone(), sq.to_queue());
        }

        // Restore sessions
        for ss in &self.sessions {
            state
                .sessions
                .insert(SessionId(ss.id.clone()), ss.to_session());
        }

        // Note: Coordination state and events are not fully restored here
        // because they require Clock for proper reconstruction

        state
    }
}

/// Serializable pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorablePipeline {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub phase: String,
    pub workspace_id: Option<String>,
    pub current_task_id: Option<String>,
    pub inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
}

impl From<&Pipeline> for StorablePipeline {
    fn from(p: &Pipeline) -> Self {
        Self {
            id: p.id.0.clone(),
            kind: format!("{:?}", p.kind),
            name: p.name.clone(),
            phase: format!("{:?}", p.phase),
            workspace_id: p.workspace_id.as_ref().map(|id| id.0.clone()),
            current_task_id: p.current_task_id.as_ref().map(|id| id.0.clone()),
            inputs: p.inputs.clone(),
            outputs: p.outputs.clone(),
            created_at: p.created_at,
        }
    }
}

impl StorablePipeline {
    fn to_pipeline(&self) -> Pipeline {
        let mut pipeline = Pipeline::new_dynamic(&self.id, &self.name, self.inputs.clone());
        pipeline.outputs = self.outputs.clone();
        pipeline.workspace_id = self
            .workspace_id
            .as_ref()
            .map(|id| crate::workspace::WorkspaceId(id.clone()));
        pipeline.current_task_id = self
            .current_task_id
            .as_ref()
            .map(|id| crate::task::TaskId(id.clone()));
        pipeline.created_at = self.created_at;
        pipeline
    }
}

/// Serializable task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableTask {
    pub id: String,
    pub pipeline_id: String,
    pub phase: String,
    pub state: String,
    pub session_id: Option<String>,
    pub heartbeat_interval_secs: u64,
    pub stuck_threshold_secs: u64,
}

impl From<&Task> for StorableTask {
    fn from(t: &Task) -> Self {
        Self {
            id: t.id.0.clone(),
            pipeline_id: t.pipeline_id.0.clone(),
            phase: t.phase.clone(),
            state: format!("{:?}", t.state),
            session_id: t.session_id.as_ref().map(|id| id.0.clone()),
            heartbeat_interval_secs: t.heartbeat_interval.as_secs(),
            stuck_threshold_secs: t.stuck_threshold.as_secs(),
        }
    }
}

impl StorableTask {
    fn to_task(&self) -> Task {
        Task {
            id: crate::task::TaskId(self.id.clone()),
            pipeline_id: crate::pipeline::PipelineId(self.pipeline_id.clone()),
            phase: self.phase.clone(),
            state: crate::task::TaskState::Pending, // Simplified - state parsing would need more work
            session_id: self
                .session_id
                .as_ref()
                .map(|id| crate::session::SessionId(id.clone())),
            heartbeat_interval: Duration::from_secs(self.heartbeat_interval_secs),
            stuck_threshold: Duration::from_secs(self.stuck_threshold_secs),
            last_heartbeat: None,
            created_at: std::time::Instant::now(),
            started_at: None,
            completed_at: None,
        }
    }
}

/// Serializable workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableWorkspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

impl From<&Workspace> for StorableWorkspace {
    fn from(w: &Workspace) -> Self {
        Self {
            id: w.id.0.clone(),
            name: w.name.clone(),
            path: w.path.to_string_lossy().to_string(),
            branch: w.branch.clone(),
            state: format!("{:?}", w.state),
            created_at: w.created_at,
        }
    }
}

impl StorableWorkspace {
    fn to_workspace(&self) -> Workspace {
        Workspace::new_ready(
            &self.id,
            &self.name,
            PathBuf::from(&self.path),
            &self.branch,
        )
    }
}

/// Serializable session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableSession {
    pub id: String,
    pub workspace_id: String,
    pub state: String,
    pub idle_threshold_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_reason: Option<String>,
    /// Heartbeat age for reconstruction (microseconds since last heartbeat)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_age_micros: Option<u64>,
}

impl StorableSession {
    fn from_session(session: &Session) -> Self {
        let clock = SystemClock;
        Self {
            id: session.id.0.clone(),
            workspace_id: session.workspace_id.0.clone(),
            state: session_state_to_string(&session.state),
            idle_threshold_secs: session.idle_threshold.as_secs(),
            death_reason: extract_death_reason(&session.state),
            last_heartbeat_age_micros: session
                .last_heartbeat
                .map(|hb| clock.now().duration_since(hb).as_micros() as u64),
        }
    }

    fn to_session(&self) -> Session {
        let clock = SystemClock;
        Session {
            id: SessionId(self.id.clone()),
            workspace_id: WorkspaceId(self.workspace_id.clone()),
            state: session_state_from_string(&self.state, &self.death_reason),
            last_output: None,
            last_output_hash: None,
            idle_threshold: Duration::from_secs(self.idle_threshold_secs),
            created_at: std::time::Instant::now(),
            last_heartbeat: self
                .last_heartbeat_age_micros
                .map(|age| clock.now() - Duration::from_micros(age)),
        }
    }
}

fn session_state_to_string(state: &SessionState) -> String {
    match state {
        SessionState::Starting => "starting".to_string(),
        SessionState::Running => "running".to_string(),
        SessionState::Idle { .. } => "idle".to_string(),
        SessionState::Dead { .. } => "dead".to_string(),
    }
}

fn extract_death_reason(state: &SessionState) -> Option<String> {
    match state {
        SessionState::Dead { reason } => Some(format!("{:?}", reason)),
        _ => None,
    }
}

fn session_state_from_string(s: &str, death_reason: &Option<String>) -> SessionState {
    match s {
        "starting" => SessionState::Starting,
        "running" => SessionState::Running,
        "idle" => SessionState::Idle {
            since: std::time::Instant::now(),
        },
        "dead" => SessionState::Dead {
            reason: death_reason
                .as_ref()
                .map(|r| match r.as_str() {
                    "Completed" => DeathReason::Completed,
                    "Killed" => DeathReason::Killed,
                    "Timeout" => DeathReason::Timeout,
                    _ => DeathReason::Error(r.clone()),
                })
                .unwrap_or(DeathReason::Completed),
        },
        _ => SessionState::Starting,
    }
}

/// Serializable queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableQueue {
    pub name: String,
    pub items: Vec<StorableQueueItem>,
    pub dead_letters: Vec<StorableDeadLetter>,
    pub default_visibility_timeout_secs: u64,
}

impl StorableQueue {
    fn from_queue(name: &str, q: &Queue) -> Self {
        Self {
            name: name.to_string(),
            items: q.items.iter().map(StorableQueueItem::from).collect(),
            dead_letters: q
                .dead_letters
                .iter()
                .map(StorableDeadLetter::from)
                .collect(),
            default_visibility_timeout_secs: q.default_visibility_timeout.as_secs(),
        }
    }

    fn to_queue(&self) -> Queue {
        let mut queue = Queue::with_visibility_timeout(
            &self.name,
            Duration::from_secs(self.default_visibility_timeout_secs),
        );
        for item in &self.items {
            queue = queue.push(item.to_queue_item());
        }
        // Note: dead letters would need separate restoration
        queue
    }
}

/// Serializable queue item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableQueueItem {
    pub id: String,
    pub data: BTreeMap<String, String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
    pub max_attempts: u32,
}

impl From<&QueueItem> for StorableQueueItem {
    fn from(i: &QueueItem) -> Self {
        Self {
            id: i.id.clone(),
            data: i.data.clone(),
            priority: i.priority,
            created_at: i.created_at,
            attempts: i.attempts,
            max_attempts: i.max_attempts,
        }
    }
}

impl StorableQueueItem {
    fn to_queue_item(&self) -> QueueItem {
        QueueItem {
            id: self.id.clone(),
            data: self.data.clone(),
            priority: self.priority,
            created_at: self.created_at,
            attempts: self.attempts,
            max_attempts: self.max_attempts,
        }
    }
}

/// Serializable dead letter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableDeadLetter {
    pub item: StorableQueueItem,
    pub reason: String,
    pub dead_at: DateTime<Utc>,
}

impl From<&DeadLetter> for StorableDeadLetter {
    fn from(d: &DeadLetter) -> Self {
        Self {
            item: StorableQueueItem::from(&d.item),
            reason: d.reason.clone(),
            dead_at: d.dead_at,
        }
    }
}

/// Serializable event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp_micros: u64,
}

impl From<&StoredEvent> for StorableEvent {
    fn from(e: &StoredEvent) -> Self {
        Self {
            event_type: e.event_type.clone(),
            payload: e.payload.clone(),
            timestamp_micros: e.timestamp_micros,
        }
    }
}

/// Snapshot metadata (stored separately from full snapshot)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub size_bytes: u64,
}

/// Manages snapshot creation, discovery, and cleanup
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager for the given directory
    pub fn new(snapshots_dir: &Path) -> Self {
        Self {
            snapshots_dir: snapshots_dir.to_path_buf(),
        }
    }

    /// Ensure the snapshots directory exists
    pub fn ensure_dir(&self) -> Result<(), SnapshotError> {
        fs::create_dir_all(&self.snapshots_dir)?;
        Ok(())
    }

    /// Generate a snapshot ID from sequence number and timestamp
    pub fn generate_id(sequence: u64, timestamp: DateTime<Utc>) -> String {
        format!("{:08}-{}", sequence, timestamp.format("%Y%m%d%H%M%S"))
    }

    /// Create a snapshot from the current state
    pub fn create_snapshot(
        &self,
        state: &MaterializedState,
        sequence: u64,
    ) -> Result<SnapshotMeta, SnapshotError> {
        self.ensure_dir()?;

        let timestamp = Utc::now();
        let id = Self::generate_id(sequence, timestamp);
        let storable = StorableState::from_materialized(state, sequence);

        let path = self.snapshot_path(&id);
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &storable)?;

        let size_bytes = fs::metadata(&path)?.len();

        Ok(SnapshotMeta {
            id,
            sequence,
            timestamp,
            size_bytes,
        })
    }

    /// Load a snapshot by ID
    pub fn load_snapshot(&self, id: &str) -> Result<StorableState, SnapshotError> {
        let path = self.snapshot_path(id);
        if !path.exists() {
            return Err(SnapshotError::NotFound(id.to_string()));
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let state: StorableState = serde_json::from_reader(reader)?;

        if state.version != StorableState::CURRENT_VERSION {
            return Err(SnapshotError::InvalidFormat(format!(
                "unsupported version: {} (expected {})",
                state.version,
                StorableState::CURRENT_VERSION
            )));
        }

        Ok(state)
    }

    /// List all available snapshots, ordered by sequence (newest first)
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>, SnapshotError> {
        if !self.snapshots_dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Parse ID to extract sequence
                    if let Some((seq_str, _)) = stem.split_once('-') {
                        if let Ok(sequence) = seq_str.parse::<u64>() {
                            let metadata = fs::metadata(&path)?;
                            snapshots.push(SnapshotMeta {
                                id: stem.to_string(),
                                sequence,
                                timestamp: DateTime::from(metadata.modified()?),
                                size_bytes: metadata.len(),
                            });
                        }
                    }
                }
            }
        }

        // Sort by sequence descending (newest first)
        snapshots.sort_by(|a, b| b.sequence.cmp(&a.sequence));

        Ok(snapshots)
    }

    /// Get the latest snapshot
    pub fn latest_snapshot(&self) -> Result<Option<SnapshotMeta>, SnapshotError> {
        Ok(self.list_snapshots()?.into_iter().next())
    }

    /// Delete a snapshot by ID
    pub fn delete_snapshot(&self, id: &str) -> Result<(), SnapshotError> {
        let path = self.snapshot_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Delete all snapshots older than the given one (keep the given one)
    pub fn cleanup_old_snapshots(
        &self,
        keep_sequence: u64,
        keep_count: usize,
    ) -> Result<Vec<String>, SnapshotError> {
        let snapshots = self.list_snapshots()?;
        let mut deleted = Vec::new();
        let mut old_kept = 0;

        for snapshot in snapshots.iter() {
            // Keep snapshots at or after keep_sequence
            if snapshot.sequence >= keep_sequence {
                continue;
            }

            // Keep up to keep_count old snapshots
            if old_kept < keep_count {
                old_kept += 1;
                continue;
            }

            self.delete_snapshot(&snapshot.id)?;
            deleted.push(snapshot.id.clone());
        }

        Ok(deleted)
    }

    fn snapshot_path(&self, id: &str) -> PathBuf {
        self.snapshots_dir.join(format!("{}.json", id))
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;

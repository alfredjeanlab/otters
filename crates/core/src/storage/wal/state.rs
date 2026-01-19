// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Materialized state from WAL replay
//!
//! The MaterializedState is the in-memory representation of all system state,
//! reconstructed by replaying WAL operations from a snapshot.

use super::operation::*;
use crate::coordination::{CoordinationManager, HolderId, LockConfig, SemaphoreConfig};
use crate::pipeline::{Phase, Pipeline, PipelineId};
use crate::queue::{Queue, QueueItem};
use crate::scheduling::{
    Action, ActionConfig, ActionEvent, ActionId, CleanupAction, Cron, CronConfig, CronEvent,
    CronId, Scanner, ScannerCondition, ScannerConfig, ScannerId, ScannerSource, ScannerState,
    Watcher, WatcherCondition, WatcherConfig, WatcherId, WatcherResponse, WatcherSource,
    WatcherState,
};
use crate::session::{DeathReason, Session, SessionId, SessionState};
use crate::task::{Task, TaskId, TaskState};
use crate::workspace::{Workspace, WorkspaceId, WorkspaceState};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Maximum events to keep in the ring buffer
const MAX_EVENTS: usize = 1000;

/// Maximum execution history entries to keep
const MAX_EXECUTION_HISTORY: usize = 1000;

/// Error applying an operation to state
#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("entity not found: {kind} {id}")]
    NotFound { kind: &'static str, id: String },
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
    #[error("entity already exists: {kind} {id}")]
    AlreadyExists { kind: &'static str, id: String },
}

/// A stored event for audit purposes
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp_micros: u64,
}

/// Record of an action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionRecord {
    pub action_id: String,
    pub source: String,
    pub execution_type: String,
    pub success: bool,
    pub output: Option<String>,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub timestamp: u64,
}

/// Record of a cleanup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRecord {
    pub scanner_id: String,
    pub resource_id: String,
    pub action: String,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: u64,
}

/// Execution history for auditing (ring buffer)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionHistory {
    /// Recent action executions (last N)
    pub action_executions: VecDeque<ActionExecutionRecord>,
    /// Recent cleanup operations (last N)
    pub cleanup_operations: VecDeque<CleanupRecord>,
    /// In-flight executions (action_id -> started_at, source, execution_type)
    #[serde(skip)]
    pub in_flight: HashMap<String, (u64, String, String)>,
}

/// Full system state materialized from WAL
#[derive(Debug, Default)]
pub struct MaterializedState {
    pub pipelines: HashMap<PipelineId, Pipeline>,
    pub tasks: HashMap<TaskId, Task>,
    pub workspaces: HashMap<WorkspaceId, Workspace>,
    pub sessions: HashMap<SessionId, Session>,
    pub queues: HashMap<String, Queue>,
    pub coordination: CoordinationManager,
    pub crons: HashMap<CronId, Cron>,
    pub actions: HashMap<ActionId, Action>,
    pub watchers: HashMap<WatcherId, Watcher>,
    pub scanners: HashMap<ScannerId, Scanner>,
    events: Vec<StoredEvent>,
    pub execution_history: ExecutionHistory,
}

impl Clone for MaterializedState {
    fn clone(&self) -> Self {
        Self {
            pipelines: self.pipelines.clone(),
            tasks: self.tasks.clone(),
            workspaces: self.workspaces.clone(),
            sessions: self.sessions.clone(),
            queues: self.queues.clone(),
            coordination: CoordinationManager::new(), // Fresh manager
            crons: self.crons.clone(),
            actions: self.actions.clone(),
            watchers: self.watchers.clone(),
            scanners: self.scanners.clone(),
            events: self.events.clone(),
            execution_history: self.execution_history.clone(),
        }
    }
}

impl MaterializedState {
    /// Create a new empty state
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a single operation to the state
    pub fn apply(&mut self, op: &Operation, timestamp_micros: u64) -> Result<(), ApplyError> {
        match op {
            // Pipeline operations
            Operation::PipelineCreate(create) => {
                let id = PipelineId(create.id.clone());
                if self.pipelines.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "pipeline",
                        id: create.id.clone(),
                    });
                }

                let mut pipeline =
                    Pipeline::new_dynamic(&create.id, &create.name, create.inputs.clone());
                pipeline.outputs = create.outputs.clone();
                pipeline.workspace_id = create
                    .workspace_id
                    .as_ref()
                    .map(|id| WorkspaceId(id.clone()));
                pipeline.created_at = micros_to_datetime(create.created_at_micros);

                self.pipelines.insert(id, pipeline);
            }

            Operation::PipelineTransition(trans) => {
                let id = PipelineId(trans.id.clone());
                let pipeline = self
                    .pipelines
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "pipeline",
                        id: trans.id.clone(),
                    })?;

                pipeline.phase = phase_from_string(&trans.to_phase, trans);
                if let Some(workspace_id) = &trans.workspace_id {
                    pipeline.workspace_id = Some(WorkspaceId(workspace_id.clone()));
                }
                if let Some(outputs) = &trans.outputs {
                    pipeline.outputs.extend(outputs.clone());
                }
                pipeline.current_task_id =
                    trans.current_task_id.as_ref().map(|id| TaskId(id.clone()));
            }

            Operation::PipelineDelete(del) => {
                let id = PipelineId(del.id.clone());
                self.pipelines.remove(&id);
            }

            // Task operations
            Operation::TaskCreate(create) => {
                let id = TaskId(create.id.clone());
                if self.tasks.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "task",
                        id: create.id.clone(),
                    });
                }

                let task = Task {
                    id: id.clone(),
                    pipeline_id: PipelineId(create.pipeline_id.clone()),
                    phase: create.phase.clone(),
                    state: TaskState::Pending,
                    session_id: None,
                    heartbeat_interval: Duration::from_secs(create.heartbeat_interval_secs),
                    stuck_threshold: Duration::from_secs(create.stuck_threshold_secs),
                    last_heartbeat: None,
                    created_at: Instant::now(),
                    started_at: None,
                    completed_at: None,
                };

                self.tasks.insert(id, task);
            }

            Operation::TaskTransition(trans) => {
                let id = TaskId(trans.id.clone());
                let task = self
                    .tasks
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "task",
                        id: trans.id.clone(),
                    })?;

                task.state = task_state_from_string(&trans.to_state, trans);
                if let Some(session_id) = &trans.session_id {
                    task.session_id = Some(crate::session::SessionId(session_id.clone()));
                }
            }

            Operation::TaskDelete(del) => {
                let id = TaskId(del.id.clone());
                self.tasks.remove(&id);
            }

            // Workspace operations
            Operation::WorkspaceCreate(create) => {
                let id = WorkspaceId(create.id.clone());
                if self.workspaces.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "workspace",
                        id: create.id.clone(),
                    });
                }

                let mut workspace = Workspace::new(
                    &create.id,
                    &create.name,
                    PathBuf::from(&create.path),
                    &create.branch,
                );
                workspace.state = workspace_state_from_string(&create.state);
                workspace.created_at = micros_to_datetime(create.created_at_micros);

                self.workspaces.insert(id, workspace);
            }

            Operation::WorkspaceTransition(trans) => {
                let id = WorkspaceId(trans.id.clone());
                let workspace =
                    self.workspaces
                        .get_mut(&id)
                        .ok_or_else(|| ApplyError::NotFound {
                            kind: "workspace",
                            id: trans.id.clone(),
                        })?;

                workspace.state =
                    workspace_state_from_string_with_session(&trans.to_state, &trans.session_id);
            }

            Operation::WorkspaceDelete(del) => {
                let id = WorkspaceId(del.id.clone());
                self.workspaces.remove(&id);
            }

            // Queue operations
            Operation::QueuePush(push) => {
                let queue = self
                    .queues
                    .entry(push.queue_name.clone())
                    .or_insert_with(|| Queue::new(&push.queue_name));

                let item =
                    QueueItem::with_priority(&push.item_id, push.data.clone(), push.priority)
                        .with_max_attempts(push.max_attempts);

                *queue = queue.push(item);
            }

            Operation::QueueClaim(claim) => {
                if let Some(queue) = self.queues.get_mut(&claim.queue_name) {
                    let (new_queue, _) = queue.transition(
                        crate::queue::QueueEvent::Claim {
                            claim_id: claim.claim_id.clone(),
                            visibility_timeout: Some(Duration::from_secs(
                                claim.visibility_timeout_secs,
                            )),
                        },
                        &crate::clock::SystemClock,
                    );
                    *queue = new_queue;
                }
            }

            Operation::QueueComplete(complete) => {
                if let Some(queue) = self.queues.get_mut(&complete.queue_name) {
                    let (new_queue, _) = queue.transition(
                        crate::queue::QueueEvent::Complete {
                            claim_id: complete.claim_id.clone(),
                        },
                        &crate::clock::SystemClock,
                    );
                    *queue = new_queue;
                }
            }

            Operation::QueueFail(fail) => {
                if let Some(queue) = self.queues.get_mut(&fail.queue_name) {
                    let (new_queue, _) = queue.transition(
                        crate::queue::QueueEvent::Fail {
                            claim_id: fail.claim_id.clone(),
                            reason: fail.reason.clone(),
                        },
                        &crate::clock::SystemClock,
                    );
                    *queue = new_queue;
                }
            }

            Operation::QueueRelease(release) => {
                if let Some(queue) = self.queues.get_mut(&release.queue_name) {
                    let (new_queue, _) = queue.transition(
                        crate::queue::QueueEvent::Release {
                            claim_id: release.claim_id.clone(),
                        },
                        &crate::clock::SystemClock,
                    );
                    *queue = new_queue;
                }
            }

            Operation::QueueDelete(del) => {
                self.queues.remove(&del.queue_name);
            }

            Operation::QueueTick(tick) => {
                // Apply tick result - the tick_result_json contains the new queue state
                // after processing visibility timeouts
                if let Ok(queue) = serde_json::from_str::<Queue>(&tick.tick_result_json) {
                    self.queues.insert(tick.queue_name.clone(), queue);
                } else {
                    tracing::warn!(
                        queue_name = %tick.queue_name,
                        "failed to deserialize queue state in QueueTick operation"
                    );
                }
            }

            // Lock operations
            Operation::LockAcquire(acquire) => {
                self.coordination
                    .ensure_lock(LockConfig::new(&acquire.lock_name));
                self.coordination.acquire_lock(
                    &acquire.lock_name,
                    HolderId::new(&acquire.holder_id),
                    acquire.metadata.clone(),
                    &crate::clock::SystemClock,
                );
            }

            Operation::LockRelease(release) => {
                self.coordination.release_lock(
                    &release.lock_name,
                    HolderId::new(&release.holder_id),
                    &crate::clock::SystemClock,
                );
            }

            Operation::LockHeartbeat(heartbeat) => {
                self.coordination.heartbeat_lock(
                    &heartbeat.lock_name,
                    HolderId::new(&heartbeat.holder_id),
                    &crate::clock::SystemClock,
                );
            }

            // Semaphore operations
            Operation::SemaphoreAcquire(acquire) => {
                self.coordination.ensure_semaphore(
                    SemaphoreConfig::new(&acquire.semaphore_name, 10), // Default capacity
                );
                self.coordination.acquire_semaphore(
                    &acquire.semaphore_name,
                    acquire.holder_id.clone(),
                    acquire.weight,
                    acquire.metadata.clone(),
                    &crate::clock::SystemClock,
                );
            }

            Operation::SemaphoreRelease(release) => {
                self.coordination.release_semaphore(
                    &release.semaphore_name,
                    release.holder_id.clone(),
                    &crate::clock::SystemClock,
                );
            }

            Operation::SemaphoreHeartbeat(heartbeat) => {
                self.coordination.heartbeat_semaphore(
                    &heartbeat.semaphore_name,
                    heartbeat.holder_id.clone(),
                    &crate::clock::SystemClock,
                );
            }

            // Session operations
            Operation::SessionCreate(create) => {
                let id = SessionId(create.id.clone());
                if self.sessions.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "session",
                        id: create.id.clone(),
                    });
                }

                let session = Session::new(
                    &create.id,
                    WorkspaceId(create.workspace_id.clone()),
                    Duration::from_secs(create.idle_threshold_secs),
                    &crate::clock::SystemClock,
                );

                self.sessions.insert(id, session);
            }

            Operation::SessionTransition(trans) => {
                let id = SessionId(trans.id.clone());
                let session = self
                    .sessions
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "session",
                        id: trans.id.clone(),
                    })?;

                session.state = session_state_from_string(&trans.to_state, &trans.death_reason);
            }

            Operation::SessionHeartbeat(hb) => {
                let id = SessionId(hb.id.clone());
                if let Some(session) = self.sessions.get_mut(&id) {
                    // Reconstruct Instant using current time
                    // The actual time is stored for WAL replay ordering,
                    // but we use current Instant for in-memory state
                    session.last_heartbeat = Some(std::time::Instant::now());
                }
            }

            Operation::SessionDelete(del) => {
                let id = SessionId(del.id.clone());
                self.sessions.remove(&id);
            }

            // Event operations
            Operation::EventEmit(emit) => {
                let event = StoredEvent {
                    event_type: emit.event_type.clone(),
                    payload: emit.payload.clone(),
                    timestamp_micros,
                };

                self.events.push(event);

                // Keep only the last MAX_EVENTS
                if self.events.len() > MAX_EVENTS {
                    self.events.remove(0);
                }
            }

            // Cron operations
            Operation::CronCreate(create) => {
                let id = CronId::new(&create.id);
                if self.crons.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "cron",
                        id: create.id.clone(),
                    });
                }

                let config =
                    CronConfig::new(&create.name, Duration::from_secs(create.interval_secs));
                let config = if create.enabled {
                    config.enabled()
                } else {
                    config
                };
                let cron = Cron::new(id.clone(), config, &crate::clock::SystemClock);

                self.crons.insert(id, cron);
            }

            Operation::CronTransition(trans) => {
                let id = CronId::new(&trans.id);
                let cron = self
                    .crons
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "cron",
                        id: trans.id.clone(),
                    })?;

                // Apply the transition based on to_state
                let event = cron_event_from_string(&trans.to_state, &trans.error);
                let (new_cron, _) = cron.transition(event, &crate::clock::SystemClock);
                *cron = new_cron;

                // Update run_count if provided
                if let Some(run_count) = trans.run_count {
                    cron.run_count = run_count;
                }
            }

            Operation::CronDelete(del) => {
                let id = CronId::new(&del.id);
                self.crons.remove(&id);
            }

            // Action operations
            Operation::ActionCreate(create) => {
                let id = ActionId::new(&create.id);
                if self.actions.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "action",
                        id: create.id.clone(),
                    });
                }

                let config =
                    ActionConfig::new(&create.name, Duration::from_secs(create.cooldown_secs));
                let action = Action::new(id.clone(), config);

                self.actions.insert(id, action);
            }

            Operation::ActionTransition(trans) => {
                let id = ActionId::new(&trans.id);
                let action = self
                    .actions
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "action",
                        id: trans.id.clone(),
                    })?;

                // Apply the transition based on to_state
                let event = action_event_from_string(&trans.to_state, &trans.source, &trans.error);
                let (new_action, _) = action.transition(event, &crate::clock::SystemClock);
                *action = new_action;

                // Update execution_count if provided
                if let Some(execution_count) = trans.execution_count {
                    action.execution_count = execution_count;
                }
            }

            Operation::ActionDelete(del) => {
                let id = ActionId::new(&del.id);
                self.actions.remove(&id);
            }

            // Watcher operations
            Operation::WatcherCreate(create) => {
                let id = WatcherId::new(&create.id);
                if self.watchers.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "watcher",
                        id: create.id.clone(),
                    });
                }

                // Deserialize nested structures
                let source: WatcherSource = serde_json::from_str(&create.source_json)
                    .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;
                let condition: WatcherCondition = serde_json::from_str(&create.condition_json)
                    .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;
                let response_chain: Vec<WatcherResponse> =
                    serde_json::from_str(&create.response_chain_json)
                        .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;

                let config = WatcherConfig::new(
                    &create.name,
                    source,
                    condition,
                    Duration::from_secs(create.check_interval_secs),
                )
                .with_responses(response_chain);

                let watcher = Watcher::new(id.clone(), config);
                self.watchers.insert(id, watcher);
            }

            Operation::WatcherTransition(trans) => {
                let id = WatcherId::new(&trans.id);
                let watcher = self
                    .watchers
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "watcher",
                        id: trans.id.clone(),
                    })?;

                // Update state directly
                watcher.state = WatcherState::from_string(&trans.to_state);

                // Update consecutive_triggers if provided
                if let Some(consecutive) = trans.consecutive_triggers {
                    watcher.consecutive_triggers = consecutive;
                }
            }

            Operation::WatcherDelete(del) => {
                let id = WatcherId::new(&del.id);
                self.watchers.remove(&id);
            }

            // Scanner operations
            Operation::ScannerCreate(create) => {
                let id = ScannerId::new(&create.id);
                if self.scanners.contains_key(&id) {
                    return Err(ApplyError::AlreadyExists {
                        kind: "scanner",
                        id: create.id.clone(),
                    });
                }

                // Deserialize nested structures
                let source: ScannerSource = serde_json::from_str(&create.source_json)
                    .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;
                let condition: ScannerCondition = serde_json::from_str(&create.condition_json)
                    .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;
                let cleanup_action: CleanupAction =
                    serde_json::from_str(&create.cleanup_action_json)
                        .map_err(|e| ApplyError::InvalidTransition(e.to_string()))?;

                let config = ScannerConfig::new(
                    &create.name,
                    source,
                    condition,
                    cleanup_action,
                    Duration::from_secs(create.scan_interval_secs),
                );

                let scanner = Scanner::new(id.clone(), config);
                self.scanners.insert(id, scanner);
            }

            Operation::ScannerTransition(trans) => {
                let id = ScannerId::new(&trans.id);
                let scanner = self
                    .scanners
                    .get_mut(&id)
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "scanner",
                        id: trans.id.clone(),
                    })?;

                // Update state directly
                scanner.state = ScannerState::from_string(&trans.to_state);

                // Update total_cleaned if provided
                if let Some(total_cleaned) = trans.total_cleaned {
                    scanner.total_cleaned = total_cleaned;
                }
            }

            Operation::ScannerDelete(del) => {
                let id = ScannerId::new(&del.id);
                self.scanners.remove(&id);
            }

            // Execution tracking operations
            Operation::ActionExecutionStarted(op) => {
                // Track in-flight executions
                self.execution_history.in_flight.insert(
                    op.action_id.clone(),
                    (op.started_at, op.source.clone(), op.execution_type.clone()),
                );
            }

            Operation::ActionExecutionCompleted(op) => {
                // Get source and execution_type from in-flight tracking
                let (source, execution_type) = self
                    .execution_history
                    .in_flight
                    .remove(&op.action_id)
                    .map(|(_, source, exec_type)| (source, exec_type))
                    .unwrap_or_default();

                self.execution_history
                    .action_executions
                    .push_back(ActionExecutionRecord {
                        action_id: op.action_id.clone(),
                        source,
                        execution_type,
                        success: op.success,
                        output: op.output.clone(),
                        duration_ms: op.duration_ms,
                        error: op.error.clone(),
                        timestamp: op.completed_at,
                    });

                // Trim to max size
                while self.execution_history.action_executions.len() > MAX_EXECUTION_HISTORY {
                    self.execution_history.action_executions.pop_front();
                }
            }

            Operation::CleanupExecuted(op) => {
                self.execution_history
                    .cleanup_operations
                    .push_back(CleanupRecord {
                        scanner_id: op.scanner_id.clone(),
                        resource_id: op.resource_id.clone(),
                        action: op.action.clone(),
                        success: op.success,
                        error: op.error.clone(),
                        timestamp: op.executed_at,
                    });

                // Trim to max size
                while self.execution_history.cleanup_operations.len() > MAX_EXECUTION_HISTORY {
                    self.execution_history.cleanup_operations.pop_front();
                }
            }

            // Snapshot marker - no state change
            Operation::SnapshotTaken { .. } => {}
        }

        Ok(())
    }

    // Accessor methods

    /// Get pipeline by ID
    pub fn pipeline(&self, id: &PipelineId) -> Option<&Pipeline> {
        self.pipelines.get(id)
    }

    /// Get all pipelines
    pub fn all_pipelines(&self) -> impl Iterator<Item = &Pipeline> {
        self.pipelines.values()
    }

    /// Get task by ID
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Get all tasks
    pub fn all_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    /// Get workspace by ID
    pub fn workspace(&self, id: &WorkspaceId) -> Option<&Workspace> {
        self.workspaces.get(id)
    }

    /// Get all workspaces
    pub fn all_workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.values()
    }

    /// Get session by ID
    pub fn session(&self, id: &SessionId) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// Get queue by name
    pub fn queue(&self, name: &str) -> Option<&Queue> {
        self.queues.get(name)
    }

    /// Get all queues
    pub fn all_queues(&self) -> impl Iterator<Item = (&String, &Queue)> {
        self.queues.iter()
    }

    /// Get recent events
    pub fn recent_events(&self) -> &[StoredEvent] {
        &self.events
    }

    /// Get coordination manager reference
    pub fn coordination(&self) -> &CoordinationManager {
        &self.coordination
    }

    /// Get cron by ID
    pub fn cron(&self, id: &CronId) -> Option<&Cron> {
        self.crons.get(id)
    }

    /// Get all crons
    pub fn all_crons(&self) -> impl Iterator<Item = &Cron> {
        self.crons.values()
    }

    /// Get action by ID
    pub fn action(&self, id: &ActionId) -> Option<&Action> {
        self.actions.get(id)
    }

    /// Get all actions
    pub fn all_actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    /// Get watcher by ID
    pub fn watcher(&self, id: &WatcherId) -> Option<&Watcher> {
        self.watchers.get(id)
    }

    /// Get all watchers
    pub fn all_watchers(&self) -> impl Iterator<Item = &Watcher> {
        self.watchers.values()
    }

    /// Get scanner by ID
    pub fn scanner(&self, id: &ScannerId) -> Option<&Scanner> {
        self.scanners.get(id)
    }

    /// Get all scanners
    pub fn all_scanners(&self) -> impl Iterator<Item = &Scanner> {
        self.scanners.values()
    }

    /// Get execution history
    pub fn execution_history(&self) -> &ExecutionHistory {
        &self.execution_history
    }
}

// Helper functions for converting strings to types

fn micros_to_datetime(micros: i64) -> DateTime<Utc> {
    Utc.timestamp_micros(micros)
        .single()
        .unwrap_or_else(Utc::now)
}

fn phase_from_string(s: &str, trans: &PipelineTransitionOp) -> Phase {
    match s {
        "init" => Phase::Init,
        "plan" => Phase::Plan,
        "decompose" => Phase::Decompose,
        "execute" => Phase::Execute,
        "fix" => Phase::Fix,
        "verify" => Phase::Verify,
        "merge" => Phase::Merge,
        "cleanup" => Phase::Cleanup,
        "done" => Phase::Done,
        "failed" => Phase::Failed {
            reason: trans.failed_reason.clone().unwrap_or_default(),
        },
        "blocked" => Phase::Blocked {
            waiting_on: trans.blocked_waiting_on.clone().unwrap_or_default(),
            guard_id: trans.blocked_guard_id.clone(),
        },
        _ => Phase::Done,
    }
}

fn task_state_from_string(s: &str, trans: &TaskTransitionOp) -> TaskState {
    match s {
        "pending" => TaskState::Pending,
        "running" => TaskState::Running,
        "stuck" => TaskState::Stuck {
            since: Instant::now(),
            nudge_count: trans.nudge_count.unwrap_or(0),
        },
        "done" => TaskState::Done {
            output: trans.output.clone(),
        },
        "failed" => TaskState::Failed {
            reason: trans.failed_reason.clone().unwrap_or_default(),
        },
        _ => TaskState::Pending,
    }
}

fn workspace_state_from_string(s: &str) -> WorkspaceState {
    match s {
        "creating" => WorkspaceState::Creating,
        "ready" => WorkspaceState::Ready,
        "dirty" => WorkspaceState::Dirty,
        "stale" => WorkspaceState::Stale,
        _ => WorkspaceState::Creating,
    }
}

fn workspace_state_from_string_with_session(
    s: &str,
    session_id: &Option<String>,
) -> WorkspaceState {
    match s {
        "creating" => WorkspaceState::Creating,
        "ready" => WorkspaceState::Ready,
        "in_use" => WorkspaceState::InUse {
            session_id: session_id.clone().unwrap_or_default(),
        },
        "dirty" => WorkspaceState::Dirty,
        "stale" => WorkspaceState::Stale,
        _ => WorkspaceState::Creating,
    }
}

fn session_state_from_string(s: &str, death_reason: &Option<String>) -> SessionState {
    match s {
        "starting" => SessionState::Starting,
        "running" => SessionState::Running,
        "idle" => SessionState::Idle {
            since: Instant::now(),
        },
        "dead" => SessionState::Dead {
            reason: death_reason
                .as_ref()
                .map(|r| match r.as_str() {
                    "completed" => DeathReason::Completed,
                    "killed" => DeathReason::Killed,
                    "timeout" => DeathReason::Timeout,
                    _ => DeathReason::Error(r.clone()),
                })
                .unwrap_or(DeathReason::Completed),
        },
        _ => SessionState::Starting,
    }
}

fn cron_event_from_string(to_state: &str, error: &Option<String>) -> CronEvent {
    match to_state {
        "enabled" => CronEvent::Enable,
        "disabled" => CronEvent::Disable,
        "running" => CronEvent::Tick,
        "completed" => CronEvent::Complete,
        "failed" => CronEvent::Fail {
            error: error.clone().unwrap_or_default(),
        },
        _ => CronEvent::Disable, // Default to disable for unknown states
    }
}

fn action_event_from_string(
    to_state: &str,
    source: &Option<String>,
    error: &Option<String>,
) -> ActionEvent {
    match to_state {
        "executing" => ActionEvent::Trigger {
            source: source.clone().unwrap_or_default(),
        },
        "cooling" | "completed" => ActionEvent::Complete,
        "failed" => ActionEvent::Fail {
            error: error.clone().unwrap_or_default(),
        },
        "ready" => ActionEvent::CooldownExpired,
        _ => ActionEvent::CooldownExpired, // Default to ready state
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

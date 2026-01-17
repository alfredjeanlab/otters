//! Effects and events for state machine orchestration

use crate::pipeline::PipelineId;
use crate::session::SessionId;
use crate::task::TaskId;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Effects are side effects that state machines request
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Emit an event for other components to observe
    Emit(Event),
    /// Spawn a new tmux session
    SpawnSession {
        name: String,
        cwd: PathBuf,
        command: String,
    },
    /// Kill an existing tmux session
    KillSession { name: String },
    /// Send input to a tmux session
    SendToSession { name: String, input: String },
    /// Create a git worktree
    CreateWorktree { branch: String, path: PathBuf },
    /// Remove a git worktree
    RemoveWorktree { path: PathBuf },
    /// Execute a git merge operation
    Merge {
        path: PathBuf,
        branch: String,
        strategy: MergeStrategy,
    },
    /// Save state to storage
    SaveState { kind: String, id: String },
    /// Save a pipeline checkpoint
    SaveCheckpoint {
        pipeline_id: PipelineId,
        checkpoint: Checkpoint,
    },
    /// Schedule a task for execution
    ScheduleTask { task_id: TaskId, delay: Option<Duration> },
    /// Cancel a scheduled task
    CancelTask { task_id: TaskId },
    /// Set a timer
    SetTimer { id: String, duration: Duration },
    /// Cancel a timer
    CancelTimer { id: String },
    /// Log a message
    Log { level: LogLevel, message: String },
}

/// A checkpoint capturing pipeline state for recovery
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub pipeline_id: PipelineId,
    pub phase: String,
    pub inputs: std::collections::HashMap<String, String>,
    pub outputs: std::collections::HashMap<String, String>,
    pub created_at: Instant,
    pub sequence: u64,
}

/// Events emitted by state machines
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    // Workspace events
    WorkspaceCreated { id: String, name: String },
    WorkspaceReady { id: String },
    WorkspaceDeleted { id: String },

    // Session events
    SessionStarted { id: String, workspace_id: String },
    SessionActive { id: String },
    SessionIdle { id: String },
    SessionDead { id: String, reason: String },

    // Pipeline events
    PipelineCreated { id: String, kind: String },
    PipelinePhase { id: String, phase: String },
    PipelineComplete { id: String },
    PipelineFailed { id: String, reason: String },
    PipelineBlocked { id: String, reason: String },
    PipelineResumed { id: String, phase: String },
    PipelineRestored { id: String, from_sequence: u64 },

    // Queue events
    QueueItemAdded { queue: String, item_id: String },
    QueueItemTaken { queue: String, item_id: String },
    QueueItemClaimed { queue: String, item_id: String, claim_id: String },
    QueueItemComplete { queue: String, item_id: String },
    QueueItemFailed { queue: String, item_id: String, reason: String },
    QueueItemReleased { queue: String, item_id: String, reason: String },
    QueueItemDeadLettered { queue: String, item_id: String, reason: String },

    // Task events
    TaskStarted { id: TaskId, session_id: SessionId },
    TaskComplete { id: TaskId, output: Option<String> },
    TaskFailed { id: TaskId, reason: String },
    TaskStuck { id: TaskId, since: Instant },
    TaskNudged { id: TaskId, count: u32 },
    TaskRestarted { id: TaskId, session_id: SessionId },
    TaskRecovered { id: TaskId },

    // Timer events
    TimerFired { id: String },
}

/// Merge strategies for git operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    FastForward,
    Rebase,
    Merge,
}

/// Log levels for effect-based logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

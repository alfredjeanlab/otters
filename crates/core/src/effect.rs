//! Effects and events for state machine orchestration

use std::path::PathBuf;

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
    /// Log a message
    Log { level: LogLevel, message: String },
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

    // Queue events
    QueueItemAdded { queue: String, item_id: String },
    QueueItemTaken { queue: String, item_id: String },
    QueueItemComplete { queue: String, item_id: String },
    QueueItemFailed {
        queue: String,
        item_id: String,
        reason: String,
    },
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

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

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
    ScheduleTask {
        task_id: TaskId,
        delay: Option<Duration>,
    },
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
    pub inputs: std::collections::BTreeMap<String, String>,
    pub outputs: std::collections::BTreeMap<String, String>,
    pub created_at: Instant,
    pub sequence: u64,
}

/// Events emitted by state machines
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Event {
    // Workspace events
    WorkspaceCreated {
        id: String,
        name: String,
    },
    WorkspaceReady {
        id: String,
    },
    WorkspaceDeleted {
        id: String,
    },

    // Session events
    SessionStarted {
        id: String,
        workspace_id: String,
    },
    SessionActive {
        id: String,
    },
    SessionIdle {
        id: String,
    },
    SessionDead {
        id: String,
        reason: String,
    },

    // Pipeline events
    PipelineCreated {
        id: String,
        kind: String,
    },
    PipelinePhase {
        id: String,
        phase: String,
    },
    PipelineComplete {
        id: String,
    },
    PipelineFailed {
        id: String,
        reason: String,
    },
    PipelineBlocked {
        id: String,
        reason: String,
    },
    PipelineResumed {
        id: String,
        phase: String,
    },
    PipelineRestored {
        id: String,
        from_sequence: u64,
    },

    // Queue events
    QueueItemAdded {
        queue: String,
        item_id: String,
    },
    QueueItemTaken {
        queue: String,
        item_id: String,
    },
    QueueItemClaimed {
        queue: String,
        item_id: String,
        claim_id: String,
    },
    QueueItemComplete {
        queue: String,
        item_id: String,
    },
    QueueItemFailed {
        queue: String,
        item_id: String,
        reason: String,
    },
    QueueItemReleased {
        queue: String,
        item_id: String,
        reason: String,
    },
    QueueItemDeadLettered {
        queue: String,
        item_id: String,
        reason: String,
    },

    // Task events
    TaskStarted {
        id: TaskId,
        session_id: SessionId,
    },
    TaskComplete {
        id: TaskId,
        output: Option<String>,
    },
    TaskFailed {
        id: TaskId,
        reason: String,
    },
    TaskStuck {
        id: TaskId,
        #[serde(skip, default = "Instant::now")]
        since: Instant,
    },
    TaskNudged {
        id: TaskId,
        count: u32,
    },
    TaskRestarted {
        id: TaskId,
        session_id: SessionId,
    },
    TaskRecovered {
        id: TaskId,
    },

    // Timer events
    TimerFired {
        id: String,
    },

    // Lock events
    LockAcquired {
        name: String,
        holder: String,
        metadata: Option<String>,
    },
    LockReleased {
        name: String,
        holder: String,
    },
    LockDenied {
        name: String,
        holder: String,
        current_holder: String,
    },
    LockReclaimed {
        name: String,
        previous_holder: String,
        new_holder: String,
    },
    LockStale {
        name: String,
        holder: String,
    },

    // Semaphore events
    SemaphoreAcquired {
        name: String,
        holder_id: String,
        weight: u32,
        metadata: Option<String>,
        available: u32,
    },
    SemaphoreReleased {
        name: String,
        holder_id: String,
        weight: u32,
        available: u32,
    },
    SemaphoreDenied {
        name: String,
        holder_id: String,
        requested: u32,
        available: u32,
    },
    SemaphoreReclaimed {
        name: String,
        holder_id: String,
        weight: u32,
    },
    SemaphoreHolderStale {
        name: String,
        holder_id: String,
        weight: u32,
    },

    // Guard events
    GuardEvaluating {
        guard_id: String,
        pipeline_id: String,
    },
    GuardPassed {
        guard_id: String,
        pipeline_id: String,
    },
    GuardFailed {
        guard_id: String,
        pipeline_id: String,
        reason: String,
    },

    // Strategy events
    StrategyStarted {
        id: String,
        name: String,
    },
    StrategyAttemptStarted {
        id: String,
        attempt: String,
        index: usize,
    },
    StrategyAttemptFailed {
        id: String,
        attempt: String,
        reason: String,
        rolling_back: bool,
    },
    StrategyRollbackComplete {
        id: String,
        attempt: String,
    },
    StrategySucceeded {
        id: String,
        attempt: String,
    },
    StrategyExhausted {
        id: String,
        #[serde(skip)]
        action: crate::strategy::ExhaustAction,
    },
    StrategyFailed {
        id: String,
        reason: String,
    },

    // Cron events
    CronEnabled {
        id: String,
    },
    CronDisabled {
        id: String,
    },
    CronTriggered {
        id: String,
    },
    CronCompleted {
        id: String,
        run_count: u64,
    },
    CronFailed {
        id: String,
        error: String,
    },

    // Action events
    ActionTriggered {
        id: String,
        source: String,
    },
    ActionCompleted {
        id: String,
    },
    ActionFailed {
        id: String,
        error: String,
    },
    ActionReady {
        id: String,
    },
    ActionRejected {
        id: String,
        source: String,
        reason: String,
    },

    // Watcher events
    WatcherTriggered {
        id: String,
        consecutive: u32,
    },
    WatcherResolved {
        id: String,
    },
    WatcherEscalated {
        id: String,
    },
    WatcherPaused {
        id: String,
    },
    WatcherResumed {
        id: String,
    },

    // Scanner events
    ScannerStarted {
        id: String,
    },
    ScannerFound {
        id: String,
        count: u32,
    },
    ScannerCleaned {
        id: String,
        count: u64,
        total: u64,
    },
    ScannerFailed {
        id: String,
        error: String,
    },
    ScannerDeleteResource {
        scanner_id: String,
        resource_id: String,
    },
    ScannerReleaseResource {
        scanner_id: String,
        resource_id: String,
    },
    ScannerFailResource {
        scanner_id: String,
        resource_id: String,
        reason: String,
    },
    ScannerDeadLetterResource {
        scanner_id: String,
        resource_id: String,
    },
    ScannerArchiveResource {
        scanner_id: String,
        resource_id: String,
        destination: String,
    },
}

impl Event {
    /// Get the event name for pattern matching
    /// Format: "category:action" or "category:subcategory:action"
    pub fn name(&self) -> String {
        match self {
            // Workspace events
            Event::WorkspaceCreated { .. } => "workspace:created".to_string(),
            Event::WorkspaceReady { .. } => "workspace:ready".to_string(),
            Event::WorkspaceDeleted { .. } => "workspace:deleted".to_string(),

            // Session events
            Event::SessionStarted { .. } => "session:started".to_string(),
            Event::SessionActive { .. } => "session:active".to_string(),
            Event::SessionIdle { .. } => "session:idle".to_string(),
            Event::SessionDead { .. } => "session:dead".to_string(),

            // Pipeline events
            Event::PipelineCreated { .. } => "pipeline:created".to_string(),
            Event::PipelinePhase { .. } => "pipeline:phase".to_string(),
            Event::PipelineComplete { .. } => "pipeline:complete".to_string(),
            Event::PipelineFailed { .. } => "pipeline:failed".to_string(),
            Event::PipelineBlocked { .. } => "pipeline:blocked".to_string(),
            Event::PipelineResumed { .. } => "pipeline:resumed".to_string(),
            Event::PipelineRestored { .. } => "pipeline:restored".to_string(),

            // Queue events
            Event::QueueItemAdded { .. } => "queue:item:added".to_string(),
            Event::QueueItemTaken { .. } => "queue:item:taken".to_string(),
            Event::QueueItemClaimed { .. } => "queue:item:claimed".to_string(),
            Event::QueueItemComplete { .. } => "queue:item:complete".to_string(),
            Event::QueueItemFailed { .. } => "queue:item:failed".to_string(),
            Event::QueueItemReleased { .. } => "queue:item:released".to_string(),
            Event::QueueItemDeadLettered { .. } => "queue:item:deadlettered".to_string(),

            // Task events
            Event::TaskStarted { .. } => "task:started".to_string(),
            Event::TaskComplete { .. } => "task:complete".to_string(),
            Event::TaskFailed { .. } => "task:failed".to_string(),
            Event::TaskStuck { .. } => "task:stuck".to_string(),
            Event::TaskNudged { .. } => "task:nudged".to_string(),
            Event::TaskRestarted { .. } => "task:restarted".to_string(),
            Event::TaskRecovered { .. } => "task:recovered".to_string(),

            // Timer events
            Event::TimerFired { .. } => "timer:fired".to_string(),

            // Lock events
            Event::LockAcquired { .. } => "lock:acquired".to_string(),
            Event::LockReleased { .. } => "lock:released".to_string(),
            Event::LockDenied { .. } => "lock:denied".to_string(),
            Event::LockReclaimed { .. } => "lock:reclaimed".to_string(),
            Event::LockStale { .. } => "lock:stale".to_string(),

            // Semaphore events
            Event::SemaphoreAcquired { .. } => "semaphore:acquired".to_string(),
            Event::SemaphoreReleased { .. } => "semaphore:released".to_string(),
            Event::SemaphoreDenied { .. } => "semaphore:denied".to_string(),
            Event::SemaphoreReclaimed { .. } => "semaphore:reclaimed".to_string(),
            Event::SemaphoreHolderStale { .. } => "semaphore:stale".to_string(),

            // Guard events
            Event::GuardEvaluating { .. } => "guard:evaluating".to_string(),
            Event::GuardPassed { .. } => "guard:passed".to_string(),
            Event::GuardFailed { .. } => "guard:failed".to_string(),

            // Strategy events
            Event::StrategyStarted { .. } => "strategy:started".to_string(),
            Event::StrategyAttemptStarted { .. } => "strategy:attempt:started".to_string(),
            Event::StrategyAttemptFailed { .. } => "strategy:attempt:failed".to_string(),
            Event::StrategyRollbackComplete { .. } => "strategy:rollback:complete".to_string(),
            Event::StrategySucceeded { .. } => "strategy:succeeded".to_string(),
            Event::StrategyExhausted { .. } => "strategy:exhausted".to_string(),
            Event::StrategyFailed { .. } => "strategy:failed".to_string(),

            // Cron events
            Event::CronEnabled { .. } => "cron:enabled".to_string(),
            Event::CronDisabled { .. } => "cron:disabled".to_string(),
            Event::CronTriggered { .. } => "cron:triggered".to_string(),
            Event::CronCompleted { .. } => "cron:completed".to_string(),
            Event::CronFailed { .. } => "cron:failed".to_string(),

            // Action events
            Event::ActionTriggered { .. } => "action:triggered".to_string(),
            Event::ActionCompleted { .. } => "action:completed".to_string(),
            Event::ActionFailed { .. } => "action:failed".to_string(),
            Event::ActionReady { .. } => "action:ready".to_string(),
            Event::ActionRejected { .. } => "action:rejected".to_string(),

            // Watcher events
            Event::WatcherTriggered { .. } => "watcher:triggered".to_string(),
            Event::WatcherResolved { .. } => "watcher:resolved".to_string(),
            Event::WatcherEscalated { .. } => "watcher:escalated".to_string(),
            Event::WatcherPaused { .. } => "watcher:paused".to_string(),
            Event::WatcherResumed { .. } => "watcher:resumed".to_string(),

            // Scanner events
            Event::ScannerStarted { .. } => "scanner:started".to_string(),
            Event::ScannerFound { .. } => "scanner:found".to_string(),
            Event::ScannerCleaned { .. } => "scanner:cleaned".to_string(),
            Event::ScannerFailed { .. } => "scanner:failed".to_string(),
            Event::ScannerDeleteResource { .. } => "scanner:delete".to_string(),
            Event::ScannerReleaseResource { .. } => "scanner:release".to_string(),
            Event::ScannerFailResource { .. } => "scanner:fail".to_string(),
            Event::ScannerDeadLetterResource { .. } => "scanner:deadletter".to_string(),
            Event::ScannerArchiveResource { .. } => "scanner:archive".to_string(),
        }
    }
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

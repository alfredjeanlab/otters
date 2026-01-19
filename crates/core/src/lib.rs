// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! oj-core: Core library for the Otter Jobs (oj) CLI tool
//!
//! This crate provides:
//! - Pure state machines for pipelines, workspaces, sessions, queues, and tasks
//! - Adapter traits for external integrations (tmux, git, etc.)
//! - Effect-based orchestration
//! - JSON-based storage
//! - Events system for loose coupling and observability

pub mod clock;
pub mod config;
pub mod coordination;
pub mod events;
pub mod id;
pub mod limits;

pub mod adapters;
pub mod engine;
pub mod pipelines;
pub mod runbook;
pub mod scheduling;
pub mod storage;

// State machines (order matters for dependencies)
pub mod effect;
pub mod pipeline;
pub mod queue;
pub mod session;
pub mod strategy;
pub mod task;
pub mod workspace;

// Re-exports
pub use clock::{Clock, FakeClock, SystemClock};
pub use effect::{Checkpoint, Effect, Event, MergeStrategy};
pub use id::{IdGen, SequentialIdGen, UuidIdGen};
pub use strategy::{
    Attempt, ExhaustAction, Strategy, StrategyEffect, StrategyEvent, StrategyId, StrategyState,
};
pub use task::{Task, TaskEvent, TaskId, TaskState};

// Re-export adapters
pub use adapters::{
    ClaudeAdapter, FakeAdapters, FakeNotifier, GitAdapter, IssueAdapter, Notification,
    NotifyAdapter, NotifyError, NotifyUrgency, OsascriptNotifier, RealAdapters, RepoAdapter,
    SessionAdapter, TmuxAdapter, WkAdapter,
};

// Re-export engine
pub use engine::executor::Adapters;
pub use engine::{
    EffectResult, Engine, EngineError, EventDrivenWorker, MergeWorker, RecoveryAction,
    RecoveryConfig, RecoveryState, ScheduledItem, ScheduledKind, Scheduler, WakeReason,
    WorkerConfig, WorkerError,
};

// Re-export events
pub use events::{
    EventBus, EventLog, EventPattern, EventReceiver, EventRecord, EventSender, SubscriberId,
    Subscription,
};

// Re-export config
pub use config::{NotifyConfig, NotifyRule};

// Re-export coordination
pub use coordination::{
    BlockedGuard, CoordinationManager, CoordinationStats, GuardCondition, GuardInputType,
    GuardInputs, GuardResult, GuardType, HolderId, IssueStatus, Lock, LockConfig, LockInput,
    LockState, MaintenanceConfig, MaintenanceTask, PhaseGuards, PipelineGuards, RegisteredGuard,
    Semaphore, SemaphoreConfig, SemaphoreHolder, SemaphoreInput, StorableCoordinationState,
    StorableLock, StorableSemaphore,
};

// Re-export scheduling
pub use scheduling::{
    Action, ActionConfig, ActionEvent, ActionId, ActionState, CleanupAction, Cron, CronConfig,
    CronEvent, CronId, CronState, ResourceInfo, Scanner, ScannerCondition, ScannerConfig,
    ScannerEvent, ScannerId, ScannerSource, ScannerState, SchedulingManager, SchedulingStats,
    SourceValue, Watcher, WatcherCondition, WatcherConfig, WatcherEvent, WatcherId,
    WatcherResponse, WatcherSource, WatcherState,
};

// Re-export limits
pub use limits::{ResourceLimits, ResourceMonitor, ResourceStatus, ResourceUsage, UsageLevel};

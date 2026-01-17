//! oj-core: Core library for the Otter Jobs (oj) CLI tool
//!
//! This crate provides:
//! - Pure state machines for pipelines, workspaces, sessions, queues, and tasks
//! - Adapter traits for external integrations (tmux, git, etc.)
//! - Effect-based orchestration
//! - JSON-based storage

pub mod clock;
pub mod id;

pub mod adapters;
pub mod engine;
pub mod pipelines;
pub mod storage;

// State machines (order matters for dependencies)
pub mod pipeline;
pub mod session;
pub mod task;
pub mod effect;
pub mod queue;
pub mod workspace;

// Re-exports
pub use clock::{Clock, FakeClock, SystemClock};
pub use effect::{Checkpoint, Effect, Event, MergeStrategy};
pub use id::{IdGen, SequentialIdGen, UuidIdGen};
pub use task::{Task, TaskEvent, TaskId, TaskState};

// Re-export adapters
pub use adapters::{
    ClaudeAdapter, FakeAdapters, GitAdapter, IssueAdapter, RepoAdapter, SessionAdapter,
    TmuxAdapter, WkAdapter,
};

// Re-export engine
pub use engine::{
    Engine, EngineError, EffectResult,
    RecoveryAction, RecoveryConfig, RecoveryState,
    ScheduledItem, ScheduledKind, Scheduler,
};
pub use engine::executor::Adapters;

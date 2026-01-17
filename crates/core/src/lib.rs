//! oj-core: Core library for the Otter Jobs (oj) CLI tool
//!
//! This crate provides:
//! - Pure state machines for pipelines, workspaces, sessions, and queues
//! - Adapter traits for external integrations (tmux, git, etc.)
//! - Effect-based orchestration
//! - JSON-based storage

pub mod clock;
pub mod effect;
pub mod id;

pub mod adapters;
pub mod engine;
pub mod pipelines;
pub mod storage;

// State machines
pub mod pipeline;
pub mod queue;
pub mod session;
pub mod workspace;

// Re-exports
pub use clock::{Clock, FakeClock, SystemClock};
pub use effect::{Effect, Event, MergeStrategy};
pub use id::{IdGen, SequentialIdGen, UuidIdGen};

// Re-export adapters
pub use adapters::{
    ClaudeAdapter, FakeAdapters, GitAdapter, IssueAdapter, RepoAdapter, SessionAdapter,
    TmuxAdapter, WkAdapter,
};

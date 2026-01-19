// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Engine module for orchestrating state machines and effects

pub mod executor;
pub mod recovery;
pub mod runtime;
pub mod scheduler;
mod signals;
pub mod worker;

// Re-exports
pub use recovery::{RecoveryAction, RecoveryConfig, RecoveryState};
pub use runtime::{EffectResult, Engine, EngineError};
pub use scheduler::{ScheduledItem, ScheduledKind, Scheduler};
pub use worker::{EventDrivenWorker, MergeWorker, WakeReason, WorkerConfig, WorkerError};

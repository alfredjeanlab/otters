// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! Otter Jobs execution engine

mod error;
mod events;
mod executor;
mod monitor;
mod phases;
mod runtime;
mod scheduler;
pub mod session_log;
mod spawn;
mod workspace;

pub use error::RuntimeError;
pub use executor::{ExecuteError, Executor};
pub use runtime::{Runtime, RuntimeConfig, RuntimeDeps};
pub use scheduler::Scheduler;
pub use workspace::prepare_for_agent;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! oj-core: Core library for the Otter Jobs (oj) CLI tool

pub mod clock;
pub mod effect;
pub mod event;
pub mod id;
pub mod operation;
pub mod pipeline;
pub mod traced;
pub mod worker;

pub use clock::{Clock, FakeClock, SystemClock};
pub use effect::Effect;
pub use event::Event;
pub use id::{IdGen, SequentialIdGen, UuidIdGen};
pub use operation::Operation;
pub use pipeline::{PhaseStatus, Pipeline};
pub use traced::TracedEffect;
pub use worker::{Worker, WorkerStatus};

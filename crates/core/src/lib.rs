// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! oj-core: Core library for the Otter Jobs (oj) CLI tool

pub mod clock;
pub mod id;

pub use clock::{Clock, FakeClock, SystemClock};
pub use id::{IdGen, SequentialIdGen, UuidIdGen};

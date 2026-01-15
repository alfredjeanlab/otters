// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Events system for loose coupling and observability
//!
//! This module provides:
//! - `EventBus` - Route events to matching subscribers using patterns
//! - `EventLog` - Structured audit trail of all events
//! - `EventPattern` - Pattern matching for event subscriptions

mod bus;
mod log;
mod subscription;

pub use bus::{EventBus, EventReceiver, EventSender};
pub use log::{EventLog, EventRecord};
pub use subscription::{EventPattern, SubscriberId, Subscription};

#[cfg(test)]
mod tests;

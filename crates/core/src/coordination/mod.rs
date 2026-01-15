// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Coordination primitives for distributed resource management
//!
//! This module provides:
//! - **Lock** - Exclusive access with heartbeat-based stale detection
//! - **Semaphore** - Multi-holder resource limiting with weighted slots
//! - **Guard** - Composable conditions that gate phase transitions
//! - **CoordinationManager** - Unified interface for coordination operations

pub mod guard;
pub mod lock;
pub mod maintenance;
pub mod manager;
pub mod phase_guard;
pub mod semaphore;
pub mod storage;

pub use guard::{GuardCondition, GuardInputType, GuardInputs, GuardResult, IssueStatus};
pub use lock::{HolderId, Lock, LockConfig, LockInput, LockState};
pub use maintenance::{CoordinationStats, MaintenanceConfig, MaintenanceTask};
pub use manager::{CoordinationManager, RegisteredGuard};
pub use phase_guard::{BlockedGuard, GuardType, PhaseGuards, PipelineGuards};
pub use semaphore::{Semaphore, SemaphoreConfig, SemaphoreHolder, SemaphoreInput};
pub use storage::{StorableCoordinationState, StorableLock, StorableSemaphore};

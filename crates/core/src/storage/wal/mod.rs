// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Write-Ahead Log (WAL) module
//!
//! This module provides durable, append-only logging for all state changes.
//! The WAL is the source of truth - state is derived by replaying entries.
//!
//! ## Architecture
//!
//! ```text
//! Operation → WalEntry → WalWriter → disk (wal.jsonl)
//!                                         ↓
//!                               WalReader → replay → MaterializedState
//! ```
//!
//! ## Durability Guarantees
//!
//! - Every write is followed by `fsync()` before returning
//! - Checksums detect corruption from bit flips
//! - Truncated writes (crash during append) are detected on read
//! - Recovery truncates WAL at last valid entry

pub mod entry;
pub mod operation;
pub mod reader;
pub mod snapshot;
pub mod state;
pub mod store;
pub mod writer;

pub use entry::WalEntry;
pub use operation::*;
pub use reader::{WalCorruption, WalEntryIter, WalReadError, WalReader, WalValidation};
pub use snapshot::{SnapshotError, SnapshotManager, SnapshotMeta, StorableState};
pub use state::{ApplyError, MaterializedState, StoredEvent};
pub use store::{WalStore, WalStoreConfig, WalStoreError};
pub use writer::WalWriter;

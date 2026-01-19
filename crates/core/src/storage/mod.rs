// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Storage module for persistence
//!
//! This module provides WAL-based storage with durability guarantees.
//!
//! ## Key Components
//!
//! - `WalStore`: Main storage interface with automatic recovery
//! - `WalWriter`/`WalReader`: Low-level WAL file operations
//! - `SnapshotManager`: Periodic full-state snapshots for fast recovery
//!
//! ## File Layout
//!
//! ```text
//! .oj/
//! ├── wal.jsonl           # Write-ahead log (append-only)
//! └── snapshots/
//!     └── {seq}-{timestamp}.json
//! ```

use thiserror::Error;

pub mod wal;

pub use wal::{
    ApplyError, MaterializedState, Operation, SnapshotError, SnapshotManager, SnapshotMeta,
    StorableState, StoredEvent, WalEntry, WalReadError, WalReader, WalStore, WalStoreConfig,
    WalStoreError, WalWriter,
};

/// Basic storage errors for low-level operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

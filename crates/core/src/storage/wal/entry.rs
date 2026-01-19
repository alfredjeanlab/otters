// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WAL entry structure with checksum verification
//!
//! Each WAL entry contains a sequence number, timestamp, machine ID,
//! operation, and CRC32 checksum for integrity verification.

use super::operation::Operation;
use crate::storage::StorageError;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single entry in the write-ahead log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Monotonically increasing sequence number
    pub sequence: u64,
    /// Microseconds since Unix epoch
    pub timestamp_micros: u64,
    /// Unique machine identifier (for future multi-machine sync)
    pub machine_id: String,
    /// The operation being recorded
    pub operation: Operation,
    /// CRC32 checksum of serialized operation
    pub checksum: u32,
}

impl WalEntry {
    /// Create a new WAL entry with computed checksum
    pub fn new(sequence: u64, machine_id: &str, operation: Operation) -> Self {
        let checksum = Self::calculate_checksum(&operation);
        let timestamp_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        Self {
            sequence,
            timestamp_micros,
            machine_id: machine_id.to_string(),
            operation,
            checksum,
        }
    }

    /// Create a new WAL entry with a specific timestamp (for testing)
    pub fn new_with_timestamp(
        sequence: u64,
        timestamp_micros: u64,
        machine_id: &str,
        operation: Operation,
    ) -> Self {
        let checksum = Self::calculate_checksum(&operation);
        Self {
            sequence,
            timestamp_micros,
            machine_id: machine_id.to_string(),
            operation,
            checksum,
        }
    }

    /// Calculate CRC32 checksum of the operation
    fn calculate_checksum(operation: &Operation) -> u32 {
        // Unwrap safety: Operation always serializes successfully since it only
        // contains String, BTreeMap<String, String>, i32, u32, u64, Option, and serde_json::Value
        let json = serde_json::to_string(operation).unwrap_or_else(|_| String::new());
        crc32fast::hash(json.as_bytes())
    }

    /// Verify the checksum matches the operation
    pub fn verify(&self) -> bool {
        self.checksum == Self::calculate_checksum(&self.operation)
    }

    /// Serialize to newline-delimited JSON (one line)
    pub fn to_line(&self) -> Result<String, StorageError> {
        serde_json::to_string(self).map_err(StorageError::from)
    }

    /// Parse from a single line of JSON
    pub fn from_line(line: &str) -> Result<Self, StorageError> {
        serde_json::from_str(line).map_err(StorageError::from)
    }
}

impl PartialEq for WalEntry {
    fn eq(&self, other: &Self) -> bool {
        self.sequence == other.sequence
            && self.timestamp_micros == other.timestamp_micros
            && self.machine_id == other.machine_id
            && self.operation == other.operation
            && self.checksum == other.checksum
    }
}

impl Eq for WalEntry {}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;

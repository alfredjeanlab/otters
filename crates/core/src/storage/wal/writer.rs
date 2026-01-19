// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WAL writer for durable append operations
//!
//! The writer provides durable append-only operations with fsync
//! to ensure writes are persisted before returning.

use super::entry::WalEntry;
use super::operation::Operation;
use crate::storage::StorageError;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// WAL writer for durable append operations
pub struct WalWriter {
    path: PathBuf,
    file: File,
    next_sequence: u64,
    machine_id: String,
    bytes_written: u64,
}

impl WalWriter {
    /// Open or create a WAL file
    ///
    /// If the file exists, scans to find the next sequence number.
    pub fn open(path: &Path, machine_id: &str) -> Result<Self, StorageError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // First, scan existing file to find the last sequence number
        let next_sequence = if path.exists() {
            Self::scan_last_sequence(path)?.map(|s| s + 1).unwrap_or(0)
        } else {
            0
        };

        // Open file for appending
        let file = OpenOptions::new().create(true).append(true).open(path)?;

        Ok(Self {
            path: path.to_path_buf(),
            file,
            next_sequence,
            machine_id: machine_id.to_string(),
            bytes_written: 0,
        })
    }

    /// Create a new WAL file in a temporary directory for testing
    pub fn open_temp(machine_id: &str) -> Result<Self, StorageError> {
        let temp_dir = std::env::temp_dir().join(format!("oj-wal-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir)?;
        let path = temp_dir.join("wal.jsonl");
        Self::open(&path, machine_id)
    }

    /// Scan a WAL file to find the last valid sequence number
    fn scan_last_sequence(path: &Path) -> Result<Option<u64>, StorageError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut last_sequence = None;

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => break, // Stop at read error
            };

            if line.is_empty() {
                continue;
            }

            match WalEntry::from_line(&line) {
                Ok(entry) => {
                    if entry.verify() {
                        last_sequence = Some(entry.sequence);
                    } else {
                        break; // Stop at checksum mismatch
                    }
                }
                Err(_) => break, // Stop at parse error (truncated write)
            }
        }

        Ok(last_sequence)
    }

    /// Append an operation to the WAL
    ///
    /// Returns the assigned sequence number. The operation is durably
    /// persisted (fsync'd) before this method returns.
    pub fn append(&mut self, operation: Operation) -> Result<u64, StorageError> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let entry = WalEntry::new(sequence, &self.machine_id, operation);
        let line = entry.to_line()?;

        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;

        // Critical: sync to ensure durability before returning
        self.file.sync_all()?;

        self.bytes_written += line.len() as u64 + 1;
        Ok(sequence)
    }

    /// Append an operation with a specific timestamp (for testing)
    pub fn append_with_timestamp(
        &mut self,
        operation: Operation,
        timestamp_micros: u64,
    ) -> Result<u64, StorageError> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let entry =
            WalEntry::new_with_timestamp(sequence, timestamp_micros, &self.machine_id, operation);
        let line = entry.to_line()?;

        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;

        self.bytes_written += line.len() as u64 + 1;
        Ok(sequence)
    }

    /// Force sync to disk
    ///
    /// This is called automatically after each append, but can be called
    /// manually if needed.
    pub fn sync(&mut self) -> Result<(), StorageError> {
        self.file.sync_all()?;
        Ok(())
    }

    /// Get current sequence number (next to be assigned)
    pub fn sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Get the last assigned sequence number
    ///
    /// Returns None if no entries have been written.
    pub fn last_sequence(&self) -> Option<u64> {
        if self.next_sequence == 0 {
            None
        } else {
            Some(self.next_sequence - 1)
        }
    }

    /// Get bytes written since open
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Get the path to the WAL file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the machine ID
    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;

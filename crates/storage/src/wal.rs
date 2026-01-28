// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Write-ahead log for durable storage

use oj_core::Operation;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur in WAL operations
#[derive(Debug, Error)]
pub enum WalError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Write-ahead log for durable operation storage
pub struct Wal {
    file: File,
    sequence: u64,
}

impl Wal {
    /// Open or create a WAL at the given path
    pub fn open(path: &Path) -> Result<Self, WalError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        // Count existing entries to set sequence number
        let reader = BufReader::new(File::open(path)?);
        let sequence = reader.lines().count() as u64;

        Ok(Self { file, sequence })
    }

    /// Append an operation to the log
    pub fn append(&mut self, op: &Operation) -> Result<u64, WalError> {
        self.sequence += 1;
        let entry = WalEntry {
            seq: self.sequence,
            op: op.clone(),
        };
        let line = serde_json::to_string(&entry)?;
        writeln!(self.file, "{}", line)?;
        self.file.sync_all()?;
        Ok(self.sequence)
    }

    /// Get the current sequence number
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Replay all operations from the log
    pub fn replay(path: &Path) -> Result<Vec<Operation>, WalError> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let reader = BufReader::new(file);
        let mut ops = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            let entry: WalEntry = serde_json::from_str(&line)?;
            ops.push(entry.op);
        }

        Ok(ops)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalEntry {
    seq: u64,
    op: Operation,
}

#[cfg(test)]
#[path = "wal_tests.rs"]
mod tests;

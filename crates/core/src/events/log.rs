// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Event log for audit trail

use super::subscription::EventPattern;
use crate::effect::Event;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;

/// A logged event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    /// Monotonic sequence number
    pub sequence: u64,
    /// Event timestamp (milliseconds since log creation)
    pub timestamp_ms: u64,
    /// The event name
    pub name: String,
    /// The full event data
    pub event: Event,
}

/// Event log for audit trail
pub struct EventLog {
    path: PathBuf,
    sequence: u64,
    start_time: Instant,
}

impl EventLog {
    /// Open or create an event log at the given path
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        // Count existing entries to set sequence
        let sequence = if path.exists() {
            let file = File::open(&path)?;
            BufReader::new(file).lines().count() as u64
        } else {
            0
        };

        Ok(Self {
            path,
            sequence,
            start_time: Instant::now(),
        })
    }

    /// Append an event to the log
    pub fn append(&mut self, event: Event) -> std::io::Result<EventRecord> {
        self.sequence += 1;

        let record = EventRecord {
            sequence: self.sequence,
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            name: event.name(),
            event,
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let json = serde_json::to_string(&record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{}", json)?;

        Ok(record)
    }

    /// Read all events from the log
    pub fn read_all(&self) -> std::io::Result<Vec<EventRecord>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            let record: EventRecord = serde_json::from_str(&line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            records.push(record);
        }

        Ok(records)
    }

    /// Query events by name pattern
    pub fn query(&self, pattern: &EventPattern) -> std::io::Result<Vec<EventRecord>> {
        let all = self.read_all()?;
        Ok(all
            .into_iter()
            .filter(|r| pattern.matches(&r.name))
            .collect())
    }

    /// Query events after a sequence number
    pub fn after(&self, sequence: u64) -> std::io::Result<Vec<EventRecord>> {
        let all = self.read_all()?;
        Ok(all.into_iter().filter(|r| r.sequence > sequence).collect())
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence
    }
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;

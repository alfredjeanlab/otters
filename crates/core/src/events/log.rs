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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_log() -> (EventLog, TempDir) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.log");
        let log = EventLog::open(path).unwrap();
        (log, tmp)
    }

    #[test]
    fn append_and_read_events() {
        let (mut log, _tmp) = make_test_log();

        let event1 = Event::PipelineCreated {
            id: "p-1".to_string(),
            kind: "build".to_string(),
        };
        let event2 = Event::PipelineComplete {
            id: "p-1".to_string(),
        };

        log.append(event1).unwrap();
        log.append(event2).unwrap();

        let records = log.read_all().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sequence, 1);
        assert_eq!(records[1].sequence, 2);
        assert_eq!(records[0].name, "pipeline:created");
        assert_eq!(records[1].name, "pipeline:complete");
    }

    #[test]
    fn query_by_pattern() {
        let (mut log, _tmp) = make_test_log();

        log.append(Event::PipelineCreated {
            id: "p-1".to_string(),
            kind: "build".to_string(),
        })
        .unwrap();
        log.append(Event::TaskStarted {
            id: crate::task::TaskId("t-1".to_string()),
            session_id: crate::session::SessionId("s-1".to_string()),
        })
        .unwrap();
        log.append(Event::PipelineComplete {
            id: "p-1".to_string(),
        })
        .unwrap();

        let pattern = EventPattern::new("pipeline:*");
        let results = log.query(&pattern).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_after_sequence() {
        let (mut log, _tmp) = make_test_log();

        for i in 1..=5 {
            log.append(Event::TimerFired {
                id: format!("timer-{}", i),
            })
            .unwrap();
        }

        let results = log.after(3).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].sequence, 4);
        assert_eq!(results[1].sequence, 5);
    }

    #[test]
    fn persists_across_reopen() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.log");

        // Write some events
        {
            let mut log = EventLog::open(path.clone()).unwrap();
            log.append(Event::PipelineComplete {
                id: "p-1".to_string(),
            })
            .unwrap();
            log.append(Event::PipelineComplete {
                id: "p-2".to_string(),
            })
            .unwrap();
        }

        // Reopen and verify
        {
            let log = EventLog::open(path).unwrap();
            assert_eq!(log.current_sequence(), 2);

            let records = log.read_all().unwrap();
            assert_eq!(records.len(), 2);
        }
    }
}

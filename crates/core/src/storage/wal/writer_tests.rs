// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::storage::wal::operation::{Operation, PipelineCreateOp, PipelineDeleteOp};
use std::collections::BTreeMap;
use tempfile::TempDir;

fn temp_wal_path() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.wal");
    (dir, path)
}

fn sample_operation() -> Operation {
    Operation::PipelineCreate(PipelineCreateOp {
        id: "pipe-1".to_string(),
        kind: "Dynamic".to_string(),
        name: "Test".to_string(),
        workspace_id: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        created_at_micros: 0,
    })
}

#[test]
fn writer_creates_new_file() {
    let (_dir, path) = temp_wal_path();

    let writer = WalWriter::open(&path, "test-machine").unwrap();

    assert!(path.exists());
    assert_eq!(writer.sequence(), 0);
    assert_eq!(writer.bytes_written(), 0);
}

#[test]
fn writer_append_increments_sequence() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();

    let seq0 = writer.append(sample_operation()).unwrap();
    let seq1 = writer.append(sample_operation()).unwrap();
    let seq2 = writer.append(sample_operation()).unwrap();

    assert_eq!(seq0, 0);
    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(writer.sequence(), 3);
}

#[test]
fn writer_records_bytes_written() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();

    writer.append(sample_operation()).unwrap();

    assert!(writer.bytes_written() > 0);
}

#[test]
fn writer_persists_entries_to_disk() {
    let (_dir, path) = temp_wal_path();

    {
        let mut writer = WalWriter::open(&path, "m1").unwrap();
        writer.append(sample_operation()).unwrap();
        writer.append(sample_operation()).unwrap();
    }

    // Verify by reading the file back
    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = content.lines().collect();

    assert_eq!(lines.len(), 2);

    let entry0: WalEntry = serde_json::from_str(lines[0]).unwrap();
    let entry1: WalEntry = serde_json::from_str(lines[1]).unwrap();

    assert_eq!(entry0.sequence, 0);
    assert_eq!(entry1.sequence, 1);
    assert!(entry0.verify());
    assert!(entry1.verify());
}

#[test]
fn writer_resumes_from_existing_file() {
    let (_dir, path) = temp_wal_path();

    // First writer session
    {
        let mut writer = WalWriter::open(&path, "m1").unwrap();
        writer.append(sample_operation()).unwrap();
        writer.append(sample_operation()).unwrap();
    }

    // Second writer session - should resume
    {
        let mut writer = WalWriter::open(&path, "m1").unwrap();

        assert_eq!(writer.sequence(), 2); // Should start at 2

        let seq = writer.append(sample_operation()).unwrap();
        assert_eq!(seq, 2);
    }

    // Verify all entries
    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[test]
fn writer_handles_empty_existing_file() {
    let (_dir, path) = temp_wal_path();

    // Create empty file
    std::fs::write(&path, "").unwrap();

    let mut writer = WalWriter::open(&path, "m1").unwrap();

    assert_eq!(writer.sequence(), 0);

    let seq = writer.append(sample_operation()).unwrap();
    assert_eq!(seq, 0);
}

#[test]
fn writer_preserves_machine_id() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "special-machine-42").unwrap();
    writer.append(sample_operation()).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let entry: WalEntry = serde_json::from_str(content.lines().next().unwrap()).unwrap();

    assert_eq!(entry.machine_id, "special-machine-42");
}

#[test]
fn writer_each_entry_on_new_line() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();

    for _ in 0..5 {
        writer.append(sample_operation()).unwrap();
    }

    let content = std::fs::read_to_string(&path).unwrap();

    // Count newlines
    let newline_count = content.chars().filter(|&c| c == '\n').count();
    assert_eq!(newline_count, 5);

    // Each line should be valid JSON
    for line in content.lines() {
        let entry: WalEntry = serde_json::from_str(line).unwrap();
        assert!(entry.verify());
    }
}

#[test]
fn writer_sync_is_idempotent() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();
    writer.append(sample_operation()).unwrap();

    // Multiple syncs should not cause issues
    writer.sync().unwrap();
    writer.sync().unwrap();
    writer.sync().unwrap();
}

#[test]
fn writer_last_sequence_before_write() {
    let (_dir, path) = temp_wal_path();

    let writer = WalWriter::open(&path, "m1").unwrap();

    assert_eq!(writer.last_sequence(), None);
}

#[test]
fn writer_last_sequence_after_write() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();
    writer.append(sample_operation()).unwrap();
    writer.append(sample_operation()).unwrap();

    assert_eq!(writer.last_sequence(), Some(1));
}

#[test]
fn writer_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("dirs").join("wal.jsonl");

    let writer = WalWriter::open(&path, "m1").unwrap();

    assert!(path.parent().unwrap().exists());
    drop(writer);
    assert!(path.exists());
}

#[test]
fn writer_handles_different_operations() {
    let (_dir, path) = temp_wal_path();

    let mut writer = WalWriter::open(&path, "m1").unwrap();

    let ops = vec![
        Operation::PipelineCreate(PipelineCreateOp {
            id: "p1".to_string(),
            kind: "Dynamic".to_string(),
            name: "Test".to_string(),
            workspace_id: None,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            created_at_micros: 0,
        }),
        Operation::PipelineDelete(PipelineDeleteOp {
            id: "p1".to_string(),
        }),
        Operation::SnapshotTaken {
            snapshot_id: "snap-1".to_string(),
        },
    ];

    for op in ops {
        writer.append(op).unwrap();
    }

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = content.lines().collect();

    assert_eq!(lines.len(), 3);

    // Verify each entry
    for line in lines {
        let entry: WalEntry = serde_json::from_str(line).unwrap();
        assert!(entry.verify());
    }
}

#[test]
fn writer_temp_creates_unique_files() {
    let writer1 = WalWriter::open_temp("m1").unwrap();
    let writer2 = WalWriter::open_temp("m2").unwrap();

    assert_ne!(writer1.path(), writer2.path());
}

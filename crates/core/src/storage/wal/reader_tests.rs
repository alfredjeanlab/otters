// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::storage::wal::entry::WalEntry;
use crate::storage::wal::operation::{Operation, PipelineCreateOp};
use crate::storage::wal::writer::WalWriter;
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

fn write_entries(path: &Path, count: usize) {
    let mut writer = WalWriter::open(path, "test").unwrap();
    for _ in 0..count {
        writer.append(sample_operation()).unwrap();
    }
}

#[test]
fn reader_reads_all_entries() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 5);

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert_eq!(entries.len(), 5);

    for (i, entry_result) in entries.iter().enumerate() {
        let entry = entry_result.as_ref().unwrap();
        assert_eq!(entry.sequence, i as u64);
    }
}

#[test]
fn reader_handles_empty_file() {
    let (_dir, path) = temp_wal_path();
    std::fs::write(&path, "").unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert!(entries.is_empty());
}

#[test]
fn reader_skips_empty_lines() {
    let (_dir, path) = temp_wal_path();

    // Write entries with blank lines between
    let entry0 = WalEntry::new(0, "m1", sample_operation());
    let entry1 = WalEntry::new(1, "m1", sample_operation());

    let content = format!(
        "{}\n\n{}\n\n",
        entry0.to_line().unwrap(),
        entry1.to_line().unwrap()
    );
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert_eq!(entries.len(), 2);
}

#[test]
fn reader_entries_from_skips_earlier_sequences() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 10);

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries_from(5).unwrap().collect();

    assert_eq!(entries.len(), 5);

    for entry_result in entries {
        let entry = entry_result.unwrap();
        assert!(entry.sequence >= 5);
    }
}

#[test]
fn reader_last_sequence_returns_correct_value() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 7);

    let reader = WalReader::open(&path).unwrap();
    let last = reader.last_sequence().unwrap();

    assert_eq!(last, Some(6));
}

#[test]
fn reader_last_sequence_returns_none_for_empty() {
    let (_dir, path) = temp_wal_path();
    std::fs::write(&path, "").unwrap();

    let reader = WalReader::open(&path).unwrap();
    let last = reader.last_sequence().unwrap();

    assert_eq!(last, None);
}

#[test]
fn reader_count_returns_correct_value() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 15);

    let reader = WalReader::open(&path).unwrap();
    let count = reader.count().unwrap();

    assert_eq!(count, 15);
}

#[test]
fn reader_stops_at_corrupted_entry() {
    let (_dir, path) = temp_wal_path();

    // Write valid entries
    write_entries(&path, 3);

    // Append corrupted entry
    let mut content = std::fs::read_to_string(&path).unwrap();
    content.push_str("this is not valid json\n");
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    // Should get 3 valid entries, then error
    assert_eq!(entries.len(), 4);
    assert!(entries[0].is_ok());
    assert!(entries[1].is_ok());
    assert!(entries[2].is_ok());
    assert!(matches!(entries[3], Err(WalReadError::Corrupted { .. })));
}

#[test]
fn reader_stops_at_checksum_mismatch() {
    let (_dir, path) = temp_wal_path();

    // Write valid entries
    write_entries(&path, 2);

    // Append entry with bad checksum
    let mut content = std::fs::read_to_string(&path).unwrap();
    content.push_str(r#"{"sequence":2,"timestamp_micros":123,"machine_id":"m1","operation":{"type":"snapshot_taken","snapshot_id":"s1"},"checksum":99999}"#);
    content.push('\n');
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert_eq!(entries.len(), 3);
    assert!(entries[0].is_ok());
    assert!(entries[1].is_ok());
    assert!(matches!(
        entries[2],
        Err(WalReadError::ChecksumMismatch { .. })
    ));
}

#[test]
fn reader_handles_truncated_final_entry() {
    let (_dir, path) = temp_wal_path();

    // Write valid entries
    write_entries(&path, 2);

    // Append truncated entry
    let mut content = std::fs::read_to_string(&path).unwrap();
    content.push_str(r#"{"sequence":2,"timestamp_micros":123,"machine_"#);
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert_eq!(entries.len(), 3);
    assert!(entries[0].is_ok());
    assert!(entries[1].is_ok());
    assert!(matches!(entries[2], Err(WalReadError::Corrupted { .. })));
}

#[test]
fn reader_validate_reports_corruption() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 5);

    // Append corruption
    let mut content = std::fs::read_to_string(&path).unwrap();
    content.push_str("garbage\n");
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let validation = reader.validate().unwrap();

    assert_eq!(validation.valid_entries, 5);
    assert_eq!(validation.last_valid_sequence, Some(4));
    assert!(validation.corruption.is_some());
    assert_eq!(validation.corruption.as_ref().unwrap().line, 6);
}

#[test]
fn reader_validate_clean_file() {
    let (_dir, path) = temp_wal_path();
    write_entries(&path, 5);

    let reader = WalReader::open(&path).unwrap();
    let validation = reader.validate().unwrap();

    assert_eq!(validation.valid_entries, 5);
    assert_eq!(validation.last_valid_sequence, Some(4));
    assert!(validation.corruption.is_none());
}

#[test]
fn reader_fails_for_nonexistent_file() {
    let (_dir, path) = temp_wal_path();
    // Don't create the file

    let result = WalReader::open(&path);

    assert!(matches!(result, Err(WalReadError::Io(_))));
}

#[test]
fn reader_open_or_empty_handles_nonexistent() {
    let (_dir, path) = temp_wal_path();
    // Don't create the file

    let reader = WalReader::open_or_empty(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert!(entries.is_empty());
}

#[test]
fn reader_verifies_each_entry_checksum() {
    let (_dir, path) = temp_wal_path();

    // Write entries and tamper with operation content but keep original checksum
    let entry0 = WalEntry::new(0, "m1", sample_operation());
    let mut entry1_json: serde_json::Value = serde_json::from_str(
        &WalEntry::new(1, "m1", sample_operation())
            .to_line()
            .unwrap(),
    )
    .unwrap();
    // Tamper with operation
    entry1_json["operation"]["id"] = serde_json::json!("TAMPERED");

    let content = format!(
        "{}\n{}\n",
        entry0.to_line().unwrap(),
        serde_json::to_string(&entry1_json).unwrap()
    );
    std::fs::write(&path, content).unwrap();

    let reader = WalReader::open(&path).unwrap();
    let entries: Vec<_> = reader.entries().unwrap().collect();

    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_ok());
    assert!(matches!(
        entries[1],
        Err(WalReadError::ChecksumMismatch { .. })
    ));
}

#[test]
fn reader_path_accessor_works() {
    let (_dir, path) = temp_wal_path();
    std::fs::write(&path, "").unwrap();

    let reader = WalReader::open(&path).unwrap();

    assert_eq!(reader.path(), &path);
}

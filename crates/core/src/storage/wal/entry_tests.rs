// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::storage::wal::operation::{Operation, PipelineCreateOp};
use std::collections::BTreeMap;

fn sample_operation() -> Operation {
    Operation::PipelineCreate(PipelineCreateOp {
        id: "test-pipe".to_string(),
        kind: "Dynamic".to_string(),
        name: "Test Pipeline".to_string(),
        workspace_id: Some("ws-1".to_string()),
        inputs: {
            let mut m = BTreeMap::new();
            m.insert("key".to_string(), "value".to_string());
            m
        },
        outputs: BTreeMap::new(),
        created_at_micros: 1705123456789000,
    })
}

#[test]
fn entry_creation_computes_checksum() {
    let op = sample_operation();
    let entry = WalEntry::new(1, "machine-1", op);

    assert_eq!(entry.sequence, 1);
    assert_eq!(entry.machine_id, "machine-1");
    assert!(entry.timestamp_micros > 0);
    assert!(entry.checksum > 0);
    assert!(entry.verify());
}

#[test]
fn entry_roundtrip_serialization() {
    let op = sample_operation();
    let entry = WalEntry::new_with_timestamp(42, 1705123456789000, "test-machine", op);

    let line = entry.to_line().unwrap();
    let parsed = WalEntry::from_line(&line).unwrap();

    assert_eq!(entry, parsed);
    assert!(parsed.verify());
}

#[test]
fn entry_checksum_verification_passes_for_valid() {
    let entry = WalEntry::new(1, "m1", sample_operation());
    assert!(entry.verify());
}

#[test]
fn entry_checksum_verification_fails_for_tampered() {
    let op = sample_operation();
    let mut entry = WalEntry::new(1, "m1", op);

    // Tamper with the operation
    if let Operation::PipelineCreate(ref mut create_op) = entry.operation {
        create_op.name = "TAMPERED".to_string();
    }

    assert!(!entry.verify());
}

#[test]
fn entry_checksum_fails_for_wrong_checksum() {
    let op = sample_operation();
    let mut entry = WalEntry::new(1, "m1", op);

    // Tamper with the checksum
    entry.checksum = 12345;

    assert!(!entry.verify());
}

#[test]
fn entry_to_line_is_single_line() {
    let entry = WalEntry::new(1, "m1", sample_operation());
    let line = entry.to_line().unwrap();

    assert!(!line.contains('\n'));
    assert!(!line.contains('\r'));
}

#[test]
fn entry_from_line_parses_valid_json() {
    let json = r#"{"sequence":1,"timestamp_micros":1705123456789000,"machine_id":"test","operation":{"type":"snapshot_taken","snapshot_id":"snap-1"},"checksum":2927095877}"#;
    let entry = WalEntry::from_line(json).unwrap();

    assert_eq!(entry.sequence, 1);
    assert_eq!(entry.machine_id, "test");
    assert!(matches!(
        entry.operation,
        Operation::SnapshotTaken { snapshot_id } if snapshot_id == "snap-1"
    ));
}

#[test]
fn entry_from_line_fails_for_invalid_json() {
    let bad_json = "not valid json {";
    let result = WalEntry::from_line(bad_json);
    assert!(result.is_err());
}

#[test]
fn entry_from_line_fails_for_truncated_json() {
    let truncated = r#"{"sequence":1,"timestamp_micros":1705123456789000,"machine_id":"te"#;
    let result = WalEntry::from_line(truncated);
    assert!(result.is_err());
}

#[test]
fn entry_sequence_is_preserved() {
    let entry = WalEntry::new(12345, "m1", sample_operation());
    let line = entry.to_line().unwrap();
    let parsed = WalEntry::from_line(&line).unwrap();

    assert_eq!(parsed.sequence, 12345);
}

#[test]
fn entry_timestamp_is_preserved() {
    let ts = 1705123456789000u64;
    let entry = WalEntry::new_with_timestamp(1, ts, "m1", sample_operation());
    let line = entry.to_line().unwrap();
    let parsed = WalEntry::from_line(&line).unwrap();

    assert_eq!(parsed.timestamp_micros, ts);
}

#[test]
fn different_operations_have_different_checksums() {
    let op1 = Operation::SnapshotTaken {
        snapshot_id: "snap-1".to_string(),
    };
    let op2 = Operation::SnapshotTaken {
        snapshot_id: "snap-2".to_string(),
    };

    let entry1 = WalEntry::new(1, "m1", op1);
    let entry2 = WalEntry::new(1, "m1", op2);

    assert_ne!(entry1.checksum, entry2.checksum);
}

#[test]
fn identical_operations_have_same_checksum() {
    let op1 = Operation::SnapshotTaken {
        snapshot_id: "snap-1".to_string(),
    };
    let op2 = Operation::SnapshotTaken {
        snapshot_id: "snap-1".to_string(),
    };

    let entry1 = WalEntry::new(1, "m1", op1);
    let entry2 = WalEntry::new(2, "m1", op2); // Different sequence, same op

    assert_eq!(entry1.checksum, entry2.checksum);
}

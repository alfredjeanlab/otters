// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::collections::HashMap;

#[test]
fn wal_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write operations
    {
        let mut wal = Wal::open(&path).unwrap();
        wal.append(&Operation::PipelineCreate {
            id: "pipe-1".to_string(),
            kind: "build".to_string(),
            name: "test".to_string(),
            inputs: HashMap::new(),
            initial_phase: "init".to_string(),
        })
        .unwrap();
        wal.append(&Operation::PipelineTransition {
            id: "pipe-1".to_string(),
            phase: "plan".to_string(),
        })
        .unwrap();
    }

    // Read back
    let ops = Wal::replay(&path).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], Operation::PipelineCreate { .. }));
    assert!(matches!(ops[1], Operation::PipelineTransition { .. }));
}

#[test]
fn wal_sequence_continues() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // First session
    {
        let mut wal = Wal::open(&path).unwrap();
        assert_eq!(wal.sequence(), 0);
        wal.append(&Operation::PipelineDelete {
            id: "x".to_string(),
        })
        .unwrap();
        assert_eq!(wal.sequence(), 1);
    }

    // Second session - sequence should continue
    {
        let wal = Wal::open(&path).unwrap();
        assert_eq!(wal.sequence(), 1);
    }
}

#[test]
fn wal_replay_nonexistent() {
    let path = Path::new("/nonexistent/path/wal");
    let ops = Wal::replay(path).unwrap();
    assert!(ops.is_empty());
}

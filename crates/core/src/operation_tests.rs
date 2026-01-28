// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn legacy_wal_without_initial_phase() {
    // WAL entries from before initial_phase was added should still parse
    let legacy_json =
        r#"{"PipelineCreate":{"id":"pipe-1","kind":"build","name":"test","inputs":{}}}"#;

    let op: Operation = serde_json::from_str(legacy_json).unwrap();

    match op {
        Operation::PipelineCreate { initial_phase, .. } => {
            assert_eq!(initial_phase, "init", "should default to 'init'");
        }
        _ => panic!("expected PipelineCreate"),
    }
}

#[test]
fn operation_serialization_roundtrip() {
    let ops = vec![
        Operation::PipelineCreate {
            id: "pipe-1".to_string(),
            kind: "build".to_string(),
            name: "test-feature".to_string(),
            inputs: [("prompt".to_string(), "Add feature".to_string())]
                .into_iter()
                .collect(),
            initial_phase: "init".to_string(),
        },
        Operation::PipelineTransition {
            id: "pipe-1".to_string(),
            phase: "plan".to_string(),
        },
        Operation::WorkspaceCreate {
            id: "ws-1".to_string(),
            path: PathBuf::from("/tmp/worktree"),
            branch: "feature/test".to_string(),
        },
    ];

    for op in ops {
        let json = serde_json::to_string(&op).unwrap();
        let parsed: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, parsed);
    }
}

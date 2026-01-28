// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn apply_pipeline_create() {
    let mut state = MaterializedState::default();
    state.apply(&Operation::PipelineCreate {
        id: "pipe-1".to_string(),
        kind: "build".to_string(),
        name: "test".to_string(),
        inputs: HashMap::new(),
        initial_phase: "init".to_string(),
    });

    assert!(state.pipelines.contains_key("pipe-1"));
}

#[test]
fn apply_pipeline_delete() {
    let mut state = MaterializedState::default();
    state.apply(&Operation::PipelineCreate {
        id: "pipe-1".to_string(),
        kind: "build".to_string(),
        name: "test".to_string(),
        inputs: HashMap::new(),
        initial_phase: "init".to_string(),
    });
    state.apply(&Operation::PipelineDelete {
        id: "pipe-1".to_string(),
    });

    assert!(!state.pipelines.contains_key("pipe-1"));
}

#[test]
fn apply_workspace_lifecycle() {
    let mut state = MaterializedState::default();
    state.apply(&Operation::WorkspaceCreate {
        id: "ws-1".to_string(),
        path: PathBuf::from("/tmp/test"),
        branch: "feature/test".to_string(),
    });

    assert!(state.workspaces.contains_key("ws-1"));
    assert_eq!(state.workspaces["ws-1"].path, PathBuf::from("/tmp/test"));

    state.apply(&Operation::WorkspaceDelete {
        id: "ws-1".to_string(),
    });
    assert!(!state.workspaces.contains_key("ws-1"));
}

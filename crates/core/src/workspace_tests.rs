use super::*;

#[test]
fn workspace_transitions_from_creating_to_ready() {
    let ws = Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
    let (ws, _) = ws.transition(WorkspaceEvent::SetupComplete);
    assert_eq!(ws.state, WorkspaceState::Ready);
}

#[test]
fn workspace_transitions_from_ready_to_in_use() {
    let ws = Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
    let (ws, _) = ws.transition(WorkspaceEvent::SessionStarted {
        session_id: SessionId("sess-1".to_string()),
    });
    assert_eq!(
        ws.state,
        WorkspaceState::InUse {
            session_id: "sess-1".to_string()
        }
    );
}

#[test]
fn workspace_transitions_from_in_use_to_ready_on_clean_end() {
    let ws = Workspace {
        state: WorkspaceState::InUse {
            session_id: "sess-1".to_string(),
        },
        ..Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x")
    };
    let (ws, _) = ws.transition(WorkspaceEvent::SessionEnded { clean: true });
    assert_eq!(ws.state, WorkspaceState::Ready);
}

#[test]
fn workspace_transitions_from_in_use_to_dirty_on_unclean_end() {
    let ws = Workspace {
        state: WorkspaceState::InUse {
            session_id: "sess-1".to_string(),
        },
        ..Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x")
    };
    let (ws, _) = ws.transition(WorkspaceEvent::SessionEnded { clean: false });
    assert_eq!(ws.state, WorkspaceState::Dirty);
}

#[test]
fn workspace_transitions_to_stale_on_branch_gone() {
    let ws = Workspace::new_ready("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
    let (ws, _) = ws.transition(WorkspaceEvent::BranchGone);
    assert_eq!(ws.state, WorkspaceState::Stale);
}

#[test]
fn invalid_transition_is_ignored() {
    let ws = Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
    let (ws2, effects) = ws.transition(WorkspaceEvent::SessionEnded { clean: true });
    assert_eq!(ws.state, ws2.state);
    assert!(effects.is_empty());
}

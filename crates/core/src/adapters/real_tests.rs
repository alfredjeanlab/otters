use super::*;

#[test]
fn real_adapters_implements_adapters_trait() {
    let adapters = RealAdapters::new();
    // Verify we can access each adapter type
    let _sessions: TmuxAdapter = adapters.sessions();
    let _repos: GitAdapter = adapters.repos();
    let _issues: WkAdapter = adapters.issues();
    let _notify: OsascriptNotifier = adapters.notify();
}

#[test]
fn real_adapters_with_repo_root() {
    let adapters = RealAdapters::with_repo_root(PathBuf::from("/tmp/test"));
    let repos = adapters.repos();
    assert_eq!(repos.repo_root, PathBuf::from("/tmp/test"));
}

#[test]
fn real_adapters_with_session_prefix() {
    let adapters = RealAdapters::new().with_session_prefix("test-");
    let sessions = adapters.sessions();
    assert_eq!(sessions.session_prefix, "test-");
}

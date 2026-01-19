use super::*;

#[tokio::test]
async fn fake_session_adapter_records_calls() {
    let adapters = FakeAdapters::new();
    let sessions = adapters.sessions();

    let id = sessions
        .spawn("test", Path::new("/tmp"), "echo hello")
        .await
        .unwrap();

    assert_eq!(id.0, "test");
    assert_eq!(
        adapters.calls(),
        vec![AdapterCall::SpawnSession {
            name: "test".to_string(),
            cwd: PathBuf::from("/tmp"),
            cmd: "echo hello".to_string(),
        }]
    );
}

#[tokio::test]
async fn fake_session_adapter_prevents_duplicates() {
    let adapters = FakeAdapters::new();
    let sessions = adapters.sessions();

    sessions
        .spawn("test", Path::new("/tmp"), "echo hello")
        .await
        .unwrap();
    let result = sessions
        .spawn("test", Path::new("/tmp"), "echo world")
        .await;

    assert!(matches!(result, Err(SessionError::AlreadyExists(_))));
}

#[tokio::test]
async fn fake_repo_adapter_tracks_worktrees() {
    let adapters = FakeAdapters::new();
    let repos = adapters.repos();

    repos
        .worktree_add("feature-x", Path::new("/tmp/worktree"))
        .await
        .unwrap();

    let worktrees = repos.worktree_list().await.unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].branch, "feature-x");
}

#[tokio::test]
async fn fake_issue_adapter_creates_issues() {
    let adapters = FakeAdapters::new();
    let issues = adapters.issues();

    let id = issues
        .create("feature", "Add auth", &["plan:auth"], None)
        .await
        .unwrap();

    let issue = issues.get(&id).await.unwrap();
    assert_eq!(issue.title, "Add auth");
    assert!(issue.labels.contains(&"plan:auth".to_string()));
}

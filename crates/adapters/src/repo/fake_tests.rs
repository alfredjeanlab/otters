// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[tokio::test]
async fn fake_repo_worktree_lifecycle() {
    let adapter = FakeRepoAdapter::new();

    adapter
        .worktree_add("feature/test", Path::new("/tmp/test"))
        .await
        .unwrap();

    assert!(adapter.get_worktree(Path::new("/tmp/test")).is_some());

    let list = adapter.worktree_list().await.unwrap();
    assert!(list.contains(&"/tmp/test".to_string()));

    adapter
        .worktree_remove(Path::new("/tmp/test"))
        .await
        .unwrap();
    assert!(adapter.get_worktree(Path::new("/tmp/test")).is_none());
}

#[tokio::test]
async fn fake_repo_branch_conflict() {
    let adapter = FakeRepoAdapter::new();

    adapter
        .worktree_add("feature/test", Path::new("/tmp/test1"))
        .await
        .unwrap();

    let result = adapter
        .worktree_add("feature/test", Path::new("/tmp/test2"))
        .await;
    assert!(matches!(result, Err(RepoError::BranchExists(_))));
}

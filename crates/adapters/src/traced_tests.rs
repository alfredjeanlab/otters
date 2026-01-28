// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

/// A writer that captures log output for testing
#[derive(Clone, Default)]
struct CapturedLogs {
    logs: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self::default()
    }

    fn contents(&self) -> String {
        let logs = self.logs.lock().unwrap();
        String::from_utf8_lossy(&logs).to_string()
    }
}

impl std::io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.logs.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogs;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Run a test with captured tracing output
fn with_tracing<F, Fut>(f: F) -> (String, Fut::Output)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future,
{
    let logs = CapturedLogs::new();
    let logs_clone = logs.clone();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(logs_clone)
        .with_ansi(false)
        .without_time()
        .finish();

    let result = tracing::subscriber::with_default(subscriber, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f())
    });

    (logs.contents(), result)
}

// =============================================================================
// Precondition validation tests
// =============================================================================

#[tokio::test]
async fn traced_session_rejects_nonexistent_cwd() {
    let fake = crate::session::FakeSessionAdapter::default();
    let traced = TracedSessionAdapter::new(fake);

    let result = traced
        .spawn("test", Path::new("/nonexistent/path"), "cmd", &[])
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("working directory does not exist"),
        "Expected error about working directory, got: {}",
        err
    );
}

#[tokio::test]
async fn traced_repo_rejects_nonexistent_parent() {
    let fake = crate::repo::FakeRepoAdapter::default();
    let traced = TracedRepoAdapter::new(fake);

    let result = traced
        .worktree_add(
            "feature/test",
            &PathBuf::from("/nonexistent/parent/worktree"),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("parent directory does not exist"),
        "Expected error about parent directory, got: {}",
        err
    );
}

// =============================================================================
// Tracing output verification tests
// =============================================================================

#[test]
fn traced_session_spawn_logs_entry_and_completion() {
    let (logs, result) = with_tracing(|| async {
        let fake = crate::session::FakeSessionAdapter::default();
        let traced = TracedSessionAdapter::new(fake);

        // Use /tmp which exists on all systems
        traced
            .spawn("test-agent", Path::new("/tmp"), "echo hello", &[])
            .await
    });

    assert!(result.is_ok(), "spawn should succeed: {:?}", result);

    // Verify entry logging
    assert!(
        logs.contains("session.spawn"),
        "Should log span name. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("test-agent"),
        "Should log session name. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("starting"),
        "Should log entry message. Logs:\n{}",
        logs
    );

    // Verify completion logging
    assert!(
        logs.contains("session created"),
        "Should log completion. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("elapsed_ms"),
        "Should log timing. Logs:\n{}",
        logs
    );
}

#[test]
fn traced_session_spawn_logs_precondition_failure() {
    let (logs, result) = with_tracing(|| async {
        let fake = crate::session::FakeSessionAdapter::default();
        let traced = TracedSessionAdapter::new(fake);

        traced
            .spawn("test", Path::new("/nonexistent/path"), "cmd", &[])
            .await
    });

    assert!(result.is_err());

    // Verify error logging
    assert!(
        logs.contains("working directory does not exist"),
        "Should log precondition failure. Logs:\n{}",
        logs
    );
}

#[test]
fn traced_session_send_logs_operation() {
    let (logs, _) = with_tracing(|| async {
        let fake = crate::session::FakeSessionAdapter::default();
        let traced = TracedSessionAdapter::new(fake.clone());

        // First spawn a session
        let session_id = traced
            .spawn("test", Path::new("/tmp"), "echo", &[])
            .await
            .unwrap();

        // Then send to it
        traced.send(&session_id, "hello").await
    });

    assert!(
        logs.contains("session.send"),
        "Should log send span. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("sending"),
        "Should log send entry. Logs:\n{}",
        logs
    );
}

#[test]
fn traced_session_kill_logs_operation() {
    let (logs, _) = with_tracing(|| async {
        let fake = crate::session::FakeSessionAdapter::default();
        let traced = TracedSessionAdapter::new(fake.clone());

        // First spawn a session
        let session_id = traced
            .spawn("test", Path::new("/tmp"), "echo", &[])
            .await
            .unwrap();

        // Then kill it
        traced.kill(&session_id).await
    });

    assert!(
        logs.contains("session.kill"),
        "Should log kill span. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("killed"),
        "Should log kill completion. Logs:\n{}",
        logs
    );
}

#[test]
fn traced_repo_worktree_add_logs_entry_and_completion() {
    let (logs, result) = with_tracing(|| async {
        let fake = crate::repo::FakeRepoAdapter::default();
        let traced = TracedRepoAdapter::new(fake);

        // Use /tmp/test-worktree - parent /tmp exists
        traced
            .worktree_add("feature/test", &PathBuf::from("/tmp/test-worktree"))
            .await
    });

    assert!(result.is_ok(), "worktree_add should succeed: {:?}", result);

    // Verify entry logging
    assert!(
        logs.contains("repo.worktree_add"),
        "Should log span name. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("feature/test"),
        "Should log branch name. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("adding worktree"),
        "Should log entry message. Logs:\n{}",
        logs
    );

    // Verify completion logging
    assert!(
        logs.contains("worktree added"),
        "Should log completion. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("elapsed_ms"),
        "Should log timing. Logs:\n{}",
        logs
    );
}

#[test]
fn traced_repo_worktree_remove_logs_operation() {
    let (logs, _) = with_tracing(|| async {
        let fake = crate::repo::FakeRepoAdapter::default();
        let traced = TracedRepoAdapter::new(fake);

        // First add a worktree so removal succeeds
        traced
            .worktree_add("feature/test", &PathBuf::from("/tmp/test-worktree"))
            .await
            .unwrap();

        // Then remove it
        traced
            .worktree_remove(&PathBuf::from("/tmp/test-worktree"))
            .await
    });

    assert!(
        logs.contains("repo.worktree_remove"),
        "Should log span name. Logs:\n{}",
        logs
    );
    assert!(
        logs.contains("worktree removed"),
        "Should log completion. Logs:\n{}",
        logs
    );
}

// =============================================================================
// Delegation tests - verify traced wrapper delegates to inner adapter
// =============================================================================

#[tokio::test]
async fn traced_session_delegates_spawn_to_inner() {
    let fake = crate::session::FakeSessionAdapter::default();
    let traced = TracedSessionAdapter::new(fake.clone());

    let session_id = traced
        .spawn(
            "my-agent",
            Path::new("/tmp"),
            "echo hello",
            &[("KEY".to_string(), "VALUE".to_string())],
        )
        .await
        .unwrap();

    // Verify the inner adapter received the call
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);

    match &calls[0] {
        crate::session::SessionCall::Spawn {
            name,
            cwd,
            cmd,
            env,
        } => {
            assert_eq!(name, "my-agent");
            assert_eq!(cwd, &PathBuf::from("/tmp"));
            assert_eq!(cmd, "echo hello");
            assert_eq!(env, &[("KEY".to_string(), "VALUE".to_string())]);
        }
        other => panic!("Expected Spawn call, got {:?}", other),
    }

    // Verify we can retrieve the session
    assert!(fake.get_session(&session_id).is_some());
}

#[tokio::test]
async fn traced_repo_delegates_worktree_add_to_inner() {
    let fake = crate::repo::FakeRepoAdapter::default();
    let traced = TracedRepoAdapter::new(fake.clone());

    traced
        .worktree_add("feature/branch", &PathBuf::from("/tmp/worktree"))
        .await
        .unwrap();

    let calls = fake.calls();
    assert_eq!(calls.len(), 1);

    match &calls[0] {
        crate::repo::RepoCall::AddWorktree { branch, path } => {
            assert_eq!(branch, "feature/branch");
            assert_eq!(path, &PathBuf::from("/tmp/worktree"));
        }
        other => panic!("Expected AddWorktree call, got {:?}", other),
    }
}

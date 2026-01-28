// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::RuntimeDeps;
use oj_adapters::{FakeNotifyAdapter, FakeRepoAdapter, FakeSessionAdapter};
use oj_core::Operation;
use std::collections::HashMap;
use tempfile::tempdir;

async fn setup() -> Executor<FakeSessionAdapter, FakeRepoAdapter, FakeNotifyAdapter> {
    let dir = tempdir().unwrap();
    let wal = Wal::open(&dir.path().join("test.wal")).unwrap();

    Executor::new(
        RuntimeDeps {
            sessions: FakeSessionAdapter::new(),
            repos: FakeRepoAdapter::new(),
            notify: FakeNotifyAdapter::new(),
            wal: Arc::new(Mutex::new(wal)),
            state: Arc::new(Mutex::new(MaterializedState::default())),
        },
        Arc::new(Mutex::new(Scheduler::new())),
    )
}

#[tokio::test]
async fn executor_persist_effect() {
    let executor = setup().await;

    executor
        .execute(Effect::Persist {
            operation: Operation::PipelineCreate {
                id: "pipe-1".to_string(),
                kind: "build".to_string(),
                name: "test".to_string(),
                inputs: HashMap::new(),
                initial_phase: "init".to_string(),
            },
        })
        .await
        .unwrap();

    let state = executor.state();
    let state = state.lock().unwrap();
    assert!(state.pipelines.contains_key("pipe-1"));
}

#[tokio::test]
async fn executor_timer_effect() {
    let executor = setup().await;

    executor
        .execute(Effect::SetTimer {
            id: "test-timer".to_string(),
            duration: std::time::Duration::from_secs(60),
        })
        .await
        .unwrap();

    let scheduler = executor.scheduler();
    let scheduler = scheduler.lock().unwrap();
    assert!(scheduler.has_timers());
}

#[tokio::test]
async fn shell_effect_runs_command() {
    let executor = setup().await;

    let event = executor
        .execute(Effect::Shell {
            pipeline_id: "test".to_string(),
            phase: "init".to_string(),
            command: "echo hello".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            env: HashMap::new(),
        })
        .await
        .unwrap();

    assert!(matches!(
        event,
        Some(Event::ShellCompleted { exit_code: 0, .. })
    ));
}

#[tokio::test]
async fn shell_failure_returns_nonzero() {
    let executor = setup().await;

    let event = executor
        .execute(Effect::Shell {
            pipeline_id: "test".to_string(),
            phase: "init".to_string(),
            command: "exit 1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            env: HashMap::new(),
        })
        .await
        .unwrap();

    assert!(matches!(
        event,
        Some(Event::ShellCompleted { exit_code: 1, .. })
    ));
}

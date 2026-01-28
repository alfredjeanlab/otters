// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[tokio::test]
async fn fake_session_spawn() {
    let adapter = FakeSessionAdapter::new();
    let id = adapter
        .spawn(
            "test",
            Path::new("/tmp"),
            "echo hello",
            &[("KEY".to_string(), "value".to_string())],
        )
        .await
        .unwrap();

    assert!(adapter.get_session(&id).is_some());

    let calls = adapter.calls();
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0], SessionCall::Spawn { .. }));
}

#[tokio::test]
async fn fake_session_lifecycle() {
    let adapter = FakeSessionAdapter::new();
    let id = adapter
        .spawn("test", Path::new("/tmp"), "cmd", &[])
        .await
        .unwrap();

    assert!(adapter.is_alive(&id).await.unwrap());

    adapter.set_exited(&id, 0);
    assert!(!adapter.is_alive(&id).await.unwrap());
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::coordination::{HolderId, LockConfig};
use crate::storage::wal::MaterializedState;
use std::time::Duration;

#[test]
fn scan_locks() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Acquire a lock through the coordination manager
    state.coordination.ensure_lock(LockConfig::new("deploy"));
    state.coordination.acquire_lock(
        "deploy",
        HolderId::new("agent-1"),
        Some("test-metadata".into()),
        &clock,
    );

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Locks)
        .expect("scan should succeed");

    assert_eq!(resources.len(), 1);
    assert!(resources[0].id.contains("lock:deploy"));
    assert_eq!(resources[0].holder.as_deref(), Some("agent-1"));
}

#[test]
fn scan_locks_empty_when_free() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Create a lock but don't acquire it
    state.coordination.ensure_lock(LockConfig::new("deploy"));

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Locks)
        .expect("scan should succeed");

    // Free locks shouldn't show up as resources
    assert!(resources.is_empty());
}

#[test]
fn scan_queue_items() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Create a queue with items
    let mut queue = crate::queue::Queue::new("work");
    let item = crate::queue::QueueItem::new("item-1", std::collections::BTreeMap::new());
    queue = queue.push(item);
    state.queues.insert("work".into(), queue);

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Queue {
            name: "work".into(),
        })
        .expect("scan should succeed");

    assert_eq!(resources.len(), 1);
    assert!(resources[0].id.contains("queue:work:item-1"));
}

#[test]
fn scan_queue_not_found() {
    let clock = FakeClock::new();
    let state = MaterializedState::new();

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let result = scanner.scan(&ScannerSource::Queue {
        name: "nonexistent".into(),
    });

    assert!(matches!(result, Err(ScanError::ListFailed { .. })));
}

#[test]
fn scan_command_output_json() {
    let clock = FakeClock::new();
    let state = MaterializedState::new();

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Command {
            command:
                r#"echo '[{"id":"res-1","age_seconds":100},{"id":"res-2","age_seconds":200}]'"#
                    .into(),
        })
        .expect("scan should succeed");

    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].id, "res-1");
    assert_eq!(resources[0].age, Some(Duration::from_secs(100)));
    assert_eq!(resources[1].id, "res-2");
    assert_eq!(resources[1].age, Some(Duration::from_secs(200)));
}

#[test]
fn scan_command_output_lines() {
    let clock = FakeClock::new();
    let state = MaterializedState::new();

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Command {
            command: "printf 'res-1\nres-2\nres-3'".into(),
        })
        .expect("scan should succeed");

    assert_eq!(resources.len(), 3);
    assert_eq!(resources[0].id, "res-1");
    assert_eq!(resources[1].id, "res-2");
    assert_eq!(resources[2].id, "res-3");
}

#[test]
fn scan_sessions() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Create a session
    let session = crate::session::Session::new(
        "agent-1",
        crate::workspace::WorkspaceId("ws-1".into()),
        Duration::from_secs(60),
        &clock,
    );
    let session = session.record_heartbeat(clock.now());
    state
        .sessions
        .insert(crate::session::SessionId("agent-1".into()), session);

    let scanner = DefaultResourceScanner::new(&state, &clock);
    let resources = scanner
        .scan(&ScannerSource::Sessions)
        .expect("scan should succeed");

    assert_eq!(resources.len(), 1);
    assert!(resources[0].id.contains("session:agent-1"));
}

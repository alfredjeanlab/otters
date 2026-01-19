// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::{Clock, FakeClock};
use crate::session::SessionId;
use crate::storage::wal::MaterializedState;
use crate::workspace::WorkspaceId;
use std::time::Duration;

#[test]
fn fetch_session_idle_time() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Create a session without a heartbeat (so idle_time returns ZERO)
    let session = crate::session::Session::new(
        "agent-1",
        WorkspaceId("ws-1".into()),
        Duration::from_secs(60),
        &clock,
    );
    state.sessions.insert(SessionId("agent-1".into()), session);

    let fetcher = DefaultSourceFetcher::new(&state);
    let result = fetcher
        .fetch(
            &WatcherSource::Session {
                name: "agent-1".into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    // Session with no heartbeat returns ZERO idle time
    assert!(matches!(result, SourceValue::Idle { .. }));
}

#[test]
fn fetch_session_with_heartbeat() {
    let clock = FakeClock::new();
    let mut state = MaterializedState::new();

    // Create a session with a heartbeat
    let session = crate::session::Session::new(
        "agent-1",
        WorkspaceId("ws-1".into()),
        Duration::from_secs(60),
        &clock,
    );
    let session = session.record_heartbeat(clock.now());
    state.sessions.insert(SessionId("agent-1".into()), session);

    let fetcher = DefaultSourceFetcher::new(&state);
    let result = fetcher
        .fetch(
            &WatcherSource::Session {
                name: "agent-1".into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    // Session with heartbeat returns an Idle value (duration depends on real time)
    assert!(matches!(result, SourceValue::Idle { .. }));
}

#[test]
fn fetch_session_not_found() {
    let state = MaterializedState::new();
    let fetcher = DefaultSourceFetcher::new(&state);

    let result = fetcher.fetch(
        &WatcherSource::Session {
            name: "nonexistent".into(),
        },
        &FetchContext::default(),
    );

    assert!(matches!(result, Err(FetchError::SessionNotFound { .. })));
}

#[test]
fn fetch_queue_depth() {
    let mut state = MaterializedState::new();

    // Create a queue with items
    let mut queue = crate::queue::Queue::new("work");
    let item1 = crate::queue::QueueItem::new("item-1", std::collections::BTreeMap::new());
    let item2 = crate::queue::QueueItem::new("item-2", std::collections::BTreeMap::new());
    queue = queue.push(item1);
    queue = queue.push(item2);
    state.queues.insert("work".into(), queue);

    let fetcher = DefaultSourceFetcher::new(&state);
    let result = fetcher
        .fetch(
            &WatcherSource::Queue {
                name: "work".into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    assert_eq!(result, SourceValue::Numeric { value: 2 });
}

#[test]
fn fetch_queue_empty() {
    let state = MaterializedState::new();
    let fetcher = DefaultSourceFetcher::new(&state);

    let result = fetcher
        .fetch(
            &WatcherSource::Queue {
                name: "nonexistent".into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    assert_eq!(result, SourceValue::Numeric { value: 0 });
}

#[test]
fn fetch_command_output_numeric() {
    let state = MaterializedState::new();
    let fetcher = DefaultSourceFetcher::new(&state);

    let result = fetcher
        .fetch(
            &WatcherSource::Command {
                command: "echo 42".into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    assert_eq!(result, SourceValue::Numeric { value: 42 });
}

#[test]
fn fetch_command_output_json() {
    let state = MaterializedState::new();
    let fetcher = DefaultSourceFetcher::new(&state);

    let result = fetcher
        .fetch(
            &WatcherSource::Command {
                command: r#"echo '{"count": 5}'"#.into(),
            },
            &FetchContext::default(),
        )
        .expect("fetch should succeed");

    assert_eq!(result, SourceValue::EventCount { count: 5 });
}

#[test]
fn fetch_command_with_interpolation() {
    let state = MaterializedState::new();
    let fetcher = DefaultSourceFetcher::new(&state);

    let context = FetchContext::new().with_variable("name", "world");
    let result = fetcher
        .fetch(
            &WatcherSource::Command {
                command: "echo hello {name}".into(),
            },
            &context,
        )
        .expect("fetch should succeed");

    assert_eq!(
        result,
        SourceValue::Text {
            value: "hello world".into()
        }
    );
}

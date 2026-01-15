// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Integration tests for the events system

use super::*;
use crate::effect::Event;
use crate::task::TaskId;

#[tokio::test]
async fn event_bus_integration() {
    let bus = EventBus::new();

    // Subscribe to all events
    let mut global = bus.set_global_handler();

    // Subscribe to specific patterns
    let pipeline_sub = Subscription::new(
        "pipeline-watcher",
        vec![EventPattern::new("pipeline:*")],
        "Watch pipeline events",
    );
    let mut pipeline_rx = bus.subscribe(pipeline_sub);

    let task_sub = Subscription::new(
        "task-watcher",
        vec![EventPattern::new("task:**")],
        "Watch task events",
    );
    let mut task_rx = bus.subscribe(task_sub);

    // Publish events
    bus.publish(Event::PipelineCreated {
        id: "p-1".to_string(),
        kind: "build".to_string(),
    });

    bus.publish(Event::TaskStarted {
        id: TaskId("t-1".to_string()),
        session_id: crate::session::SessionId("s-1".to_string()),
    });

    bus.publish(Event::PipelineComplete {
        id: "p-1".to_string(),
    });

    // Global handler should receive all 3
    assert!(global.try_recv().is_ok());
    assert!(global.try_recv().is_ok());
    assert!(global.try_recv().is_ok());

    // Pipeline watcher should receive 2
    assert!(pipeline_rx.try_recv().is_ok());
    assert!(pipeline_rx.try_recv().is_ok());
    assert!(pipeline_rx.try_recv().is_err());

    // Task watcher should receive 1
    assert!(task_rx.try_recv().is_ok());
    assert!(task_rx.try_recv().is_err());
}

#[tokio::test]
async fn event_log_with_bus() {
    let tmp = tempfile::TempDir::new().unwrap();
    let log_path = tmp.path().join("events.log");
    let mut log = EventLog::open(log_path).unwrap();

    let bus = EventBus::new();
    let mut global = bus.set_global_handler();

    // Publish events through bus
    let events = vec![
        Event::PipelineCreated {
            id: "p-1".to_string(),
            kind: "build".to_string(),
        },
        Event::PipelinePhase {
            id: "p-1".to_string(),
            phase: "plan".to_string(),
        },
        Event::PipelineComplete {
            id: "p-1".to_string(),
        },
    ];

    for event in events {
        bus.publish(event);
    }

    // Log events from global handler
    while let Ok(event) = global.try_recv() {
        log.append(event).unwrap();
    }

    // Verify log contents
    let records = log.read_all().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].name, "pipeline:created");
    assert_eq!(records[1].name, "pipeline:phase");
    assert_eq!(records[2].name, "pipeline:complete");
}

#[test]
fn pattern_matching_edge_cases() {
    // Empty pattern should not match anything
    let empty = EventPattern::new("");
    assert!(!empty.matches("something"));
    assert!(!empty.matches(""));

    // Nested wildcards
    let nested = EventPattern::new("queue:*:*");
    assert!(nested.matches("queue:item:added"));
    assert!(nested.matches("queue:foo:bar"));
    assert!(!nested.matches("queue:item")); // Only 2 segments

    // Mixed patterns
    let mixed = EventPattern::new("pipeline:*:action");
    assert!(mixed.matches("pipeline:sub:action"));
    assert!(!mixed.matches("pipeline:sub:other"));
}

#[test]
fn event_names_follow_convention() {
    // All events should have category:action format
    let events: Vec<Event> = vec![
        Event::WorkspaceCreated {
            id: "ws-1".to_string(),
            name: "test".to_string(),
        },
        Event::SessionStarted {
            id: "s-1".to_string(),
            workspace_id: "ws-1".to_string(),
        },
        Event::PipelineCreated {
            id: "p-1".to_string(),
            kind: "build".to_string(),
        },
        Event::QueueItemAdded {
            queue: "q-1".to_string(),
            item_id: "i-1".to_string(),
        },
        Event::TaskStarted {
            id: TaskId("t-1".to_string()),
            session_id: crate::session::SessionId("s-1".to_string()),
        },
        Event::TimerFired {
            id: "timer-1".to_string(),
        },
    ];

    for event in events {
        let name = event.name();
        assert!(
            name.contains(':'),
            "Event name '{}' should contain ':'",
            name
        );
        assert!(
            !name.starts_with(':'),
            "Event name '{}' should not start with ':'",
            name
        );
        assert!(
            !name.ends_with(':'),
            "Event name '{}' should not end with ':'",
            name
        );
    }
}

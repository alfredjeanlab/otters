// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn event_serialization_roundtrip() {
    let events = vec![
        Event::CommandInvoked {
            command: "build".to_string(),
            args: [("name".to_string(), "test".to_string())]
                .into_iter()
                .collect(),
        },
        Event::WorkerWake {
            worker: "builds".to_string(),
        },
        Event::SessionStarted {
            session_id: "sess-1".to_string(),
        },
        Event::AgentDone {
            pipeline_id: "pipe-1".to_string(),
        },
        Event::ShellCompleted {
            pipeline_id: "pipe-1".to_string(),
            phase: "init".to_string(),
            exit_code: 0,
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}

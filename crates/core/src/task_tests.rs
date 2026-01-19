use super::*;
use crate::clock::FakeClock;

fn make_task(clock: &impl Clock) -> Task {
    Task::new(
        "task-1",
        PipelineId("pipeline-1".to_string()),
        "execute",
        Duration::from_secs(30),
        Duration::from_secs(120),
        clock,
    )
}

#[test]
fn task_starts_in_pending_state() {
    let clock = FakeClock::new();
    let task = make_task(&clock);
    assert!(task.is_pending());
    assert!(!task.is_terminal());
}

#[test]
fn task_transitions_pending_to_running() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, effects) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    assert!(task.is_running());
    assert!(task.session_id.is_some());
    assert!(task.started_at.is_some());
    assert!(task.last_heartbeat.is_some());
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskStarted { .. })
    ));
}

#[test]
fn task_heartbeat_updates_timestamp() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    clock.advance(Duration::from_secs(10));
    let new_time = clock.now();

    let (task, effects) = task.transition(
        TaskEvent::Heartbeat {
            timestamp: new_time,
        },
        &clock,
    );

    assert_eq!(task.last_heartbeat, Some(new_time));
    assert!(effects.is_empty());
}

#[test]
fn task_becomes_stuck_after_threshold() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    // Advance past stuck threshold
    clock.advance(Duration::from_secs(150));

    // Tick with session_idle_time indicating session has been idle for 150 seconds
    let (task, effects) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );

    assert!(task.is_stuck());
    assert_eq!(effects.len(), 1);
    assert!(matches!(&effects[0], Effect::Emit(Event::TaskStuck { .. })));
}

#[test]
fn task_stuck_nudge_increments_counter() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    clock.advance(Duration::from_secs(150));
    let (task, _) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );
    assert!(task.is_stuck());

    let (task, effects) = task.transition(TaskEvent::Nudged, &clock);

    if let TaskState::Stuck { nudge_count, .. } = task.state {
        assert_eq!(nudge_count, 1);
    } else {
        panic!("Expected Stuck state");
    }
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskNudged { count: 1, .. })
    ));
}

#[test]
fn task_stuck_can_recover_with_heartbeat() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    clock.advance(Duration::from_secs(150));
    let (task, _) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );
    assert!(task.is_stuck());

    let now = clock.now();
    let (task, effects) = task.transition(TaskEvent::Heartbeat { timestamp: now }, &clock);

    assert!(task.is_running());
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskRecovered { .. })
    ));
}

#[test]
fn task_stuck_can_restart_with_new_session() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    clock.advance(Duration::from_secs(150));
    let (task, _) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );
    assert!(task.is_stuck());

    let (task, effects) = task.transition(
        TaskEvent::Restart {
            session_id: SessionId("sess-2".to_string()),
        },
        &clock,
    );

    assert!(task.is_running());
    assert_eq!(task.session_id, Some(SessionId("sess-2".to_string())));
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskRestarted { .. })
    ));
}

#[test]
fn task_running_to_done() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    let (task, effects) = task.transition(
        TaskEvent::Complete {
            output: Some("success".to_string()),
        },
        &clock,
    );

    assert!(task.is_terminal());
    assert!(matches!(
        task.state,
        TaskState::Done {
            output: Some(ref s)
        } if s == "success"
    ));
    assert!(task.completed_at.is_some());
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskComplete { .. })
    ));
}

#[test]
fn task_running_to_failed() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    let (task, effects) = task.transition(
        TaskEvent::Fail {
            reason: "error".to_string(),
        },
        &clock,
    );

    assert!(task.is_terminal());
    assert!(matches!(
        task.state,
        TaskState::Failed { ref reason } if reason == "error"
    ));
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskFailed { .. })
    ));
}

#[test]
fn task_stuck_can_complete() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    clock.advance(Duration::from_secs(150));
    let (task, _) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );
    assert!(task.is_stuck());

    let (task, effects) = task.transition(TaskEvent::Complete { output: None }, &clock);

    assert!(task.is_terminal());
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::TaskComplete { .. })
    ));
}

#[test]
fn task_invalid_transitions_are_no_op() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    // Pending cannot complete
    let (task2, effects) = task.transition(TaskEvent::Complete { output: None }, &clock);
    assert!(task2.is_pending());
    assert!(effects.is_empty());

    // Pending cannot fail
    let (task2, effects) = task.transition(
        TaskEvent::Fail {
            reason: "x".to_string(),
        },
        &clock,
    );
    assert!(task2.is_pending());
    assert!(effects.is_empty());

    // Terminal states cannot transition
    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );
    let (task, _) = task.transition(TaskEvent::Complete { output: None }, &clock);
    assert!(task.is_terminal());

    let (task2, effects) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-2".to_string()),
        },
        &clock,
    );
    assert!(task2.is_terminal());
    assert!(effects.is_empty());

    let (task2, effects) = task.transition(
        TaskEvent::Restart {
            session_id: SessionId("sess-2".to_string()),
        },
        &clock,
    );
    assert!(task2.is_terminal());
    assert!(effects.is_empty());
}

#[test]
fn task_tick_does_nothing_when_not_stuck() {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );

    // Only advance a little, not past threshold
    clock.advance(Duration::from_secs(10));

    // Tick with session_idle_time less than stuck threshold
    let (task, effects) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(10)),
        },
        &clock,
    );

    assert!(task.is_running());
    assert!(effects.is_empty());
}

use yare::parameterized;

// Helper to create a task in a given state
fn task_in_state(state: &str, clock: &FakeClock) -> Task {
    let mut task = make_task(clock);

    match state {
        "pending" => {}
        "running" => {
            let (t, _) = task.transition(
                TaskEvent::Start {
                    session_id: SessionId("sess-1".to_string()),
                },
                clock,
            );
            task = t;
        }
        "stuck" => {
            let (t, _) = task.transition(
                TaskEvent::Start {
                    session_id: SessionId("sess-1".to_string()),
                },
                clock,
            );
            task = t;
            clock.advance(Duration::from_secs(150));
            let (t, _) = task.transition(
                TaskEvent::Tick {
                    session_idle_time: Some(Duration::from_secs(150)),
                },
                clock,
            );
            task = t;
        }
        "done" => {
            let (t, _) = task.transition(
                TaskEvent::Start {
                    session_id: SessionId("sess-1".to_string()),
                },
                clock,
            );
            task = t;
            let (t, _) = task.transition(TaskEvent::Complete { output: None }, clock);
            task = t;
        }
        "failed" => {
            let (t, _) = task.transition(
                TaskEvent::Start {
                    session_id: SessionId("sess-1".to_string()),
                },
                clock,
            );
            task = t;
            let (t, _) = task.transition(
                TaskEvent::Fail {
                    reason: "error".to_string(),
                },
                clock,
            );
            task = t;
        }
        _ => panic!("Unknown state: {}", state),
    }
    task
}

#[parameterized(
        pending_to_running = { "pending", "start", "running" },
        running_to_done = { "running", "complete", "done" },
        running_to_failed = { "running", "fail", "failed" },
        stuck_to_done = { "stuck", "complete", "done" },
        stuck_to_running_via_heartbeat = { "stuck", "heartbeat", "running" },
        stuck_to_running_via_restart = { "stuck", "restart", "running" },
    )]
fn valid_transitions(initial: &str, event: &str, expected: &str) {
    let clock = FakeClock::new();
    let task = task_in_state(initial, &clock);

    let event = match event {
        "start" => TaskEvent::Start {
            session_id: SessionId("sess-2".to_string()),
        },
        "complete" => TaskEvent::Complete {
            output: Some("output".to_string()),
        },
        "fail" => TaskEvent::Fail {
            reason: "failure".to_string(),
        },
        "heartbeat" => TaskEvent::Heartbeat {
            timestamp: clock.now(),
        },
        "restart" => TaskEvent::Restart {
            session_id: SessionId("sess-2".to_string()),
        },
        _ => panic!("Unknown event: {}", event),
    };

    let (new_task, effects) = task.transition(event, &clock);

    let state_name = match &new_task.state {
        TaskState::Pending => "pending",
        TaskState::Running => "running",
        TaskState::Stuck { .. } => "stuck",
        TaskState::Done { .. } => "done",
        TaskState::Failed { .. } => "failed",
    };

    assert_eq!(
        state_name, expected,
        "Expected state {} but got {}",
        expected, state_name
    );
    assert!(!effects.is_empty(), "Expected effects for valid transition");
}

#[parameterized(
        pending_cannot_complete = { "pending", "complete" },
        pending_cannot_fail = { "pending", "fail" },
        pending_cannot_nudge = { "pending", "nudge" },
        done_cannot_start = { "done", "start" },
        done_cannot_complete = { "done", "complete" },
        failed_cannot_restart = { "failed", "restart" },
        failed_cannot_complete = { "failed", "complete" },
    )]
fn invalid_transitions_are_no_op(initial: &str, event: &str) {
    let clock = FakeClock::new();
    let task = task_in_state(initial, &clock);
    let initial_state_discriminant = std::mem::discriminant(&task.state);

    let event = match event {
        "start" => TaskEvent::Start {
            session_id: SessionId("sess-2".to_string()),
        },
        "complete" => TaskEvent::Complete { output: None },
        "fail" => TaskEvent::Fail {
            reason: "x".to_string(),
        },
        "nudge" => TaskEvent::Nudged,
        "restart" => TaskEvent::Restart {
            session_id: SessionId("sess-2".to_string()),
        },
        _ => panic!("Unknown event: {}", event),
    };

    let (new_task, effects) = task.transition(event, &clock);

    assert_eq!(
        std::mem::discriminant(&new_task.state),
        initial_state_discriminant,
        "State should not change on invalid transition"
    );
    assert!(
        effects.is_empty(),
        "Invalid transitions should produce no effects"
    );
}

#[parameterized(
        nudge_from_0_to_1 = { 0, 1 },
        nudge_from_1_to_2 = { 1, 2 },
        nudge_from_5_to_6 = { 5, 6 },
    )]
fn stuck_nudge_increments_counter(initial_count: u32, expected_count: u32) {
    let clock = FakeClock::new();
    let task = make_task(&clock);

    // Start and make stuck
    let (task, _) = task.transition(
        TaskEvent::Start {
            session_id: SessionId("sess-1".to_string()),
        },
        &clock,
    );
    clock.advance(Duration::from_secs(150));
    let (mut task, _) = task.transition(
        TaskEvent::Tick {
            session_idle_time: Some(Duration::from_secs(150)),
        },
        &clock,
    );

    // Nudge to initial_count
    for _ in 0..initial_count {
        let (t, _) = task.transition(TaskEvent::Nudged, &clock);
        task = t;
    }

    // Now nudge once more
    let (task, effects) = task.transition(TaskEvent::Nudged, &clock);

    if let TaskState::Stuck { nudge_count, .. } = task.state {
        assert_eq!(nudge_count, expected_count);
    } else {
        panic!("Expected Stuck state");
    }
    assert!(!effects.is_empty());
}

// Property-based tests
use proptest::prelude::*;

proptest! {
    #[test]
    fn task_always_ends_in_terminal_or_valid_state(
        complete_or_fail in any::<bool>(),
        output in any::<Option<String>>()
    ) {
        let clock = FakeClock::new();
        let task = Task::new(
            "test-task",
            PipelineId("pipeline-1".to_string()),
            "execute",
            Duration::from_secs(30),
            Duration::from_secs(120),
            &clock,
        );

        // Start the task
        let (task, _) = task.transition(
            TaskEvent::Start {
                session_id: SessionId("sess-1".to_string()),
            },
            &clock,
        );

        prop_assert!(task.is_running());

        // Complete or fail it
        let (task, _) = if complete_or_fail {
            task.transition(TaskEvent::Complete { output }, &clock)
        } else {
            task.transition(
                TaskEvent::Fail { reason: "test failure".to_string() },
                &clock,
            )
        };

        prop_assert!(task.is_terminal());
    }

    #[test]
    fn task_heartbeat_prevents_stuck_state(
        heartbeat_intervals in proptest::collection::vec(1..100u64, 1..10)
    ) {
        let clock = FakeClock::new();
        let stuck_threshold = Duration::from_secs(120);
        let task = Task::new(
            "test-task",
            PipelineId("pipeline-1".to_string()),
            "execute",
            Duration::from_secs(30),
            stuck_threshold,
            &clock,
        );

        let (mut task, _) = task.transition(
            TaskEvent::Start {
                session_id: SessionId("sess-1".to_string()),
            },
            &clock,
        );

        // Send heartbeats at various intervals, all less than stuck_threshold
        for interval in heartbeat_intervals {
            clock.advance(Duration::from_secs(interval));
            let now = clock.now();
            let (t, _) = task.transition(TaskEvent::Heartbeat { timestamp: now }, &clock);
            task = t;

            // Tick should not make us stuck because session_idle_time is reset
            // (simulating that the session received a heartbeat too)
            let (t, _) = task.transition(
                TaskEvent::Tick {
                    session_idle_time: Some(Duration::from_secs(0)), // Fresh heartbeat
                },
                &clock,
            );
            task = t;

            prop_assert!(task.is_running(), "Task became stuck despite heartbeat");
        }
    }

    #[test]
    fn task_stuck_nudge_count_monotonically_increases(nudge_count in 1..20usize) {
        let clock = FakeClock::new();
        let task = Task::new(
            "test-task",
            PipelineId("pipeline-1".to_string()),
            "execute",
            Duration::from_secs(30),
            Duration::from_secs(120),
            &clock,
        );

        let (task, _) = task.transition(
            TaskEvent::Start {
                session_id: SessionId("sess-1".to_string()),
            },
            &clock,
        );

        // Make task stuck
        clock.advance(Duration::from_secs(150));
        let (mut task, _) = task.transition(
            TaskEvent::Tick {
                session_idle_time: Some(Duration::from_secs(150)),
            },
            &clock,
        );
        prop_assert!(task.is_stuck());

        // Nudge it multiple times
        for expected_count in 1..=nudge_count {
            let (t, _) = task.transition(TaskEvent::Nudged, &clock);
            task = t;

            if let TaskState::Stuck { nudge_count: count, .. } = &task.state {
                prop_assert_eq!(*count, expected_count as u32);
            } else {
                prop_assert!(false, "Task should still be stuck");
            }
        }
    }
}

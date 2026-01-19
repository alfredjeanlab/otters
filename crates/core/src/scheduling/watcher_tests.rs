// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use std::time::Duration;

fn make_idle_watcher() -> Watcher {
    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test-session".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("nudge")))
    .with_response(
        WatcherResponse::new(ActionId::new("restart"))
            .with_delay(Duration::from_secs(120))
            .requires_previous_failure(),
    )
    .with_response(
        WatcherResponse::new(ActionId::new("escalate"))
            .with_delay(Duration::from_secs(300))
            .requires_previous_failure(),
    );

    Watcher::new(WatcherId::new("test"), config)
}

fn make_simple_watcher() -> Watcher {
    let config = WatcherConfig::new(
        "simple-watcher",
        WatcherSource::Task {
            id: "task-1".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(60),
        },
        Duration::from_secs(30),
    )
    .with_response(WatcherResponse::new(ActionId::new("alert")));

    Watcher::new(WatcherId::new("simple"), config)
}

#[test]
fn new_watcher_is_active() {
    let watcher = make_idle_watcher();

    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(watcher.consecutive_triggers, 0);
    assert!(watcher.last_check.is_none());
}

#[test]
fn check_condition_not_met_reschedules() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(100), // Below 300s threshold
            },
        },
        &clock,
    );

    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(watcher.consecutive_triggers, 0);
    assert!(watcher.last_check.is_some());

    // Should reschedule check
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id == "watcher:test:check" && *duration == Duration::from_secs(60))));
}

#[test]
fn check_condition_met_triggers_immediate_response() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400), // Above 300s threshold
            },
        },
        &clock,
    );

    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));
    assert_eq!(watcher.consecutive_triggers, 1);

    // Should emit WatcherTriggered and ActionTriggered
    assert_eq!(effects.len(), 2);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::WatcherTriggered { id, consecutive })
        if id == "test" && *consecutive == 1)
    ));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionTriggered { id, source })
        if id == "nudge" && source == "watcher:test-watcher")
    ));
}

#[test]
fn response_succeeded_returns_to_active() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    // Trigger condition
    let (watcher, _) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400),
            },
        },
        &clock,
    );
    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));

    // Response succeeded
    let (watcher, effects) = watcher.transition(WatcherEvent::ResponseSucceeded, &clock);

    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(watcher.consecutive_triggers, 0); // Reset

    // Should emit WatcherResolved and reschedule check
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherResolved { id }) if id == "test")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "watcher:test:check")));
}

#[test]
fn response_failed_advances_to_next_with_delay() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    // Trigger condition
    let (watcher, _) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400),
            },
        },
        &clock,
    );

    // First response failed
    let (watcher, effects) = watcher.transition(WatcherEvent::ResponseFailed, &clock);

    // Should be waiting for delayed response
    assert!(matches!(
        watcher.state,
        WatcherState::WaitingForResponse { response_index: 1 }
    ));

    // Should set timer for delay
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id == "watcher:test:response" && *duration == Duration::from_secs(120))));
}

#[test]
fn response_delay_expired_triggers_action() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    // Trigger and fail first response
    let (watcher, _) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400),
            },
        },
        &clock,
    );
    let (watcher, _) = watcher.transition(WatcherEvent::ResponseFailed, &clock);
    assert!(matches!(
        watcher.state,
        WatcherState::WaitingForResponse { response_index: 1 }
    ));

    // Delay expired
    let (watcher, effects) = watcher.transition(WatcherEvent::ResponseDelayExpired, &clock);

    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 1 }
    ));

    // Should trigger the restart action
    assert_eq!(effects.len(), 1);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionTriggered { id, source })
        if id == "restart" && source == "watcher:test-watcher")
    ));
}

#[test]
fn chain_exhausted_escalates() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    // Trigger and fail all responses
    let (watcher, _) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400),
            },
        },
        &clock,
    );
    let (watcher, _) = watcher.transition(WatcherEvent::ResponseFailed, &clock);
    let (watcher, _) = watcher.transition(WatcherEvent::ResponseDelayExpired, &clock);
    let (watcher, _) = watcher.transition(WatcherEvent::ResponseFailed, &clock);
    let (watcher, _) = watcher.transition(WatcherEvent::ResponseDelayExpired, &clock);

    // Final response fails - chain exhausted
    let (watcher, effects) = watcher.transition(WatcherEvent::ResponseFailed, &clock);

    // Should be back to active with escalation event
    assert!(matches!(watcher.state, WatcherState::Active));

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherEscalated { id }) if id == "test")));
}

#[test]
fn pause_cancels_check_timer() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    let (watcher, effects) = watcher.transition(WatcherEvent::Pause, &clock);

    assert!(matches!(watcher.state, WatcherState::Paused));

    // Should cancel timer and emit pause event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CancelTimer { id }
        if id == "watcher:test:check")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherPaused { id }) if id == "test")));
}

#[test]
fn resume_reschedules_check() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    // Pause first
    let (watcher, _) = watcher.transition(WatcherEvent::Pause, &clock);
    assert!(matches!(watcher.state, WatcherState::Paused));

    // Resume
    let (watcher, effects) = watcher.transition(WatcherEvent::Resume, &clock);

    assert!(matches!(watcher.state, WatcherState::Active));

    // Should reschedule check and emit resume event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "watcher:test:check")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherResumed { id }) if id == "test")));
}

#[test]
fn check_while_paused_is_noop() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    let (watcher, _) = watcher.transition(WatcherEvent::Pause, &clock);

    let (new_watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Idle {
                duration: Duration::from_secs(400),
            },
        },
        &clock,
    );

    assert!(matches!(new_watcher.state, WatcherState::Paused));
    assert!(effects.is_empty());
}

#[test]
fn matches_condition_evaluates_correctly() {
    let config = WatcherConfig::new(
        "pattern-watcher",
        WatcherSource::Events {
            pattern: "error".to_string(),
        },
        WatcherCondition::Matches {
            pattern: "timeout".to_string(),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("alert")));

    let watcher = Watcher::new(WatcherId::new("pattern"), config);
    let clock = FakeClock::new();

    // Should not trigger for non-matching output
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Text {
                value: "success".to_string(),
            },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(effects.len(), 1); // Just reschedule

    // Should trigger for matching output
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Text {
                value: "connection timeout error".to_string(),
            },
        },
        &clock,
    );
    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));
    assert!(effects.len() >= 2); // Triggered + ActionTriggered
}

#[test]
fn exceeds_condition_evaluates_correctly() {
    let config = WatcherConfig::new(
        "threshold-watcher",
        WatcherSource::Command {
            command: "check_queue_depth".to_string(),
        },
        WatcherCondition::Exceeds { threshold: 100 },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("scale")));

    let watcher = Watcher::new(WatcherId::new("threshold"), config);
    let clock = FakeClock::new();

    // Should not trigger for value at or below threshold
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Numeric { value: 100 },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(effects.len(), 1);

    // Should trigger for value above threshold
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Numeric { value: 150 },
        },
        &clock,
    );
    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));
    assert!(effects.len() >= 2);
}

#[test]
fn stuck_in_state_condition_evaluates_correctly() {
    let config = WatcherConfig::new(
        "stuck-watcher",
        WatcherSource::Pipeline {
            id: "pipeline-1".to_string(),
        },
        WatcherCondition::StuckInState {
            state: "blocked".to_string(),
            threshold: Duration::from_secs(600),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("unblock")));

    let watcher = Watcher::new(WatcherId::new("stuck"), config);
    let clock = FakeClock::new();

    // Should not trigger for different state
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::State {
                state: "running".to_string(),
                duration: Duration::from_secs(1000),
            },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(effects.len(), 1);

    // Should not trigger for state below threshold
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::State {
                state: "blocked".to_string(),
                duration: Duration::from_secs(300),
            },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(effects.len(), 1);

    // Should trigger for state at or above threshold
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::State {
                state: "blocked".to_string(),
                duration: Duration::from_secs(700),
            },
        },
        &clock,
    );
    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));
    assert!(effects.len() >= 2);
}

#[test]
fn consecutive_failures_condition() {
    let config = WatcherConfig::new(
        "failure-watcher",
        WatcherSource::Command {
            command: "health_check".to_string(),
        },
        WatcherCondition::ConsecutiveFailures { count: 3 },
        Duration::from_secs(30),
    )
    .with_response(WatcherResponse::new(ActionId::new("alert")));

    let watcher = Watcher::new(WatcherId::new("failures"), config);
    let clock = FakeClock::new();

    // First error - not enough
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Error {
                message: "connection failed".to_string(),
            },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(watcher.consecutive_triggers, 1);
    assert_eq!(effects.len(), 1);

    // Second error - still not enough
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Error {
                message: "connection failed".to_string(),
            },
        },
        &clock,
    );
    assert!(matches!(watcher.state, WatcherState::Active));
    assert_eq!(watcher.consecutive_triggers, 2);
    assert_eq!(effects.len(), 1);

    // Third error - triggers
    let (watcher, effects) = watcher.transition(
        WatcherEvent::Check {
            value: SourceValue::Error {
                message: "connection failed".to_string(),
            },
        },
        &clock,
    );
    assert!(matches!(
        watcher.state,
        WatcherState::Triggered { response_index: 0 }
    ));
    assert_eq!(watcher.consecutive_triggers, 3);
    assert!(effects.len() >= 2);
}

#[test]
fn watcher_state_display() {
    assert_eq!(WatcherState::Active.to_string(), "active");
    assert_eq!(WatcherState::Paused.to_string(), "paused");
    assert_eq!(
        WatcherState::Triggered { response_index: 2 }.to_string(),
        "triggered:2"
    );
    assert_eq!(
        WatcherState::WaitingForResponse { response_index: 1 }.to_string(),
        "waiting:1"
    );
}

#[test]
fn watcher_state_from_string() {
    assert!(matches!(
        WatcherState::from_string("active"),
        WatcherState::Active
    ));
    assert!(matches!(
        WatcherState::from_string("paused"),
        WatcherState::Paused
    ));
    assert!(matches!(
        WatcherState::from_string("triggered:2"),
        WatcherState::Triggered { response_index: 2 }
    ));
    assert!(matches!(
        WatcherState::from_string("waiting:1"),
        WatcherState::WaitingForResponse { response_index: 1 }
    ));
}

#[test]
fn watcher_id_conversions() {
    let id = WatcherId::new("test");
    assert_eq!(id.to_string(), "test");

    let id: WatcherId = "test".into();
    assert_eq!(id.0, "test");

    let id: WatcherId = "test".to_string().into();
    assert_eq!(id.0, "test");
}

#[test]
fn timer_id_formats() {
    let watcher = make_simple_watcher();

    assert_eq!(watcher.check_timer_id(), "watcher:simple:check");
    assert_eq!(watcher.response_timer_id(), "watcher:simple:response");
}

#[test]
fn is_active() {
    let clock = FakeClock::new();
    let watcher = make_idle_watcher();

    assert!(watcher.is_active());

    let (watcher, _) = watcher.transition(WatcherEvent::Pause, &clock);
    assert!(!watcher.is_active());

    let (watcher, _) = watcher.transition(WatcherEvent::Resume, &clock);
    assert!(watcher.is_active());
}

#[test]
fn watcher_response_builder() {
    let resp = WatcherResponse::new(ActionId::new("test"))
        .with_delay(Duration::from_secs(60))
        .requires_previous_failure();

    assert_eq!(resp.action.0, "test");
    assert_eq!(resp.delay, Duration::from_secs(60));
    assert!(resp.requires_previous_failure);
}

#[test]
fn watcher_config_builder() {
    let config = WatcherConfig::new(
        "test",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(60),
        },
        Duration::from_secs(30),
    )
    .with_response(WatcherResponse::new(ActionId::new("a")))
    .with_response(WatcherResponse::new(ActionId::new("b")));

    assert_eq!(config.name, "test");
    assert_eq!(config.check_interval, Duration::from_secs(30));
    assert_eq!(config.response_chain.len(), 2);
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use std::time::Duration;

fn make_cron(clock: &FakeClock) -> Cron {
    let config = CronConfig::new("test-cron", Duration::from_secs(60));
    Cron::new(CronId::new("test"), config, clock)
}

fn make_enabled_cron(clock: &FakeClock) -> Cron {
    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    Cron::new(CronId::new("test"), config, clock)
}

#[test]
fn new_cron_is_disabled_by_default() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    assert_eq!(cron.state, CronState::Disabled);
    assert!(cron.next_run.is_none());
    assert_eq!(cron.run_count, 0);
}

#[test]
fn new_enabled_cron_has_next_run() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert!(cron.next_run.is_some());
}

#[test]
fn enable_schedules_timer() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    let (cron, effects) = cron.transition(CronEvent::Enable, &clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert!(cron.next_run.is_some());

    // Should emit SetTimer and CronEnabled event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id == "cron:test" && *duration == Duration::from_secs(60))));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronEnabled { id })
        if id == "test")));
}

#[test]
fn disable_cancels_timer() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);

    let (cron, effects) = cron.transition(CronEvent::Disable, &clock);

    assert_eq!(cron.state, CronState::Disabled);
    assert!(cron.next_run.is_none());

    // Should emit CancelTimer and CronDisabled event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CancelTimer { id } if id == "cron:test")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronDisabled { id })
        if id == "test")));
}

#[test]
fn tick_while_disabled_is_noop() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    let (new_cron, effects) = cron.transition(CronEvent::Tick, &clock);

    assert_eq!(new_cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn tick_while_enabled_starts_running() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);

    let (cron, effects) = cron.transition(CronEvent::Tick, &clock);

    assert_eq!(cron.state, CronState::Running);
    assert!(cron.last_run.is_some());
    assert!(cron.next_run.is_none()); // Cleared during running

    // Should emit CronTriggered event
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id })
        if id == "test")));
}

#[test]
fn tick_while_running_is_noop() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);
    let (cron, _) = cron.transition(CronEvent::Tick, &clock);
    assert_eq!(cron.state, CronState::Running);

    // Another tick while running should be a no-op
    let (cron, effects) = cron.transition(CronEvent::Tick, &clock);

    assert_eq!(cron.state, CronState::Running);
    assert!(effects.is_empty());
}

#[test]
fn complete_reschedules_timer() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);
    let (cron, _) = cron.transition(CronEvent::Tick, &clock);
    assert_eq!(cron.state, CronState::Running);
    assert_eq!(cron.run_count, 0);

    let (cron, effects) = cron.transition(CronEvent::Complete, &clock);

    assert_eq!(cron.state, CronState::Enabled);
    assert_eq!(cron.run_count, 1);
    assert!(cron.next_run.is_some());

    // Should emit SetTimer and CronCompleted event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "cron:test")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::CronCompleted { id, run_count })
        if id == "test" && *run_count == 1)
    ));
}

#[test]
fn fail_reschedules_timer() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);
    let (cron, _) = cron.transition(CronEvent::Tick, &clock);
    assert_eq!(cron.state, CronState::Running);

    let (cron, effects) = cron.transition(
        CronEvent::Fail {
            error: "test error".to_string(),
        },
        &clock,
    );

    assert_eq!(cron.state, CronState::Enabled);
    assert_eq!(cron.run_count, 1); // Still increments run count
    assert!(cron.next_run.is_some());

    // Should emit SetTimer and CronFailed event
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "cron:test")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::CronFailed { id, error })
        if id == "test" && error == "test error")
    ));
}

#[test]
fn disable_while_running_stops_after_completion() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);
    let (cron, _) = cron.transition(CronEvent::Tick, &clock);
    assert_eq!(cron.state, CronState::Running);

    // Disable while running
    let (cron, effects) = cron.transition(CronEvent::Disable, &clock);
    assert_eq!(cron.state, CronState::Disabled);
    assert_eq!(effects.len(), 2); // CancelTimer + CronDisabled

    // Complete after disable - should stay disabled, no reschedule
    let (cron, effects) = cron.transition(CronEvent::Complete, &clock);
    assert_eq!(cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn enable_already_enabled_is_noop() {
    let clock = FakeClock::new();
    let cron = make_enabled_cron(&clock);

    let (new_cron, effects) = cron.transition(CronEvent::Enable, &clock);

    // State unchanged
    assert_eq!(new_cron.state, CronState::Enabled);
    assert!(effects.is_empty());
}

#[test]
fn disable_already_disabled_is_noop() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    let (new_cron, effects) = cron.transition(CronEvent::Disable, &clock);

    // State unchanged
    assert_eq!(new_cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn complete_while_disabled_is_noop() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    let (new_cron, effects) = cron.transition(CronEvent::Complete, &clock);

    assert_eq!(new_cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn fail_while_disabled_is_noop() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    let (new_cron, effects) = cron.transition(
        CronEvent::Fail {
            error: "test".to_string(),
        },
        &clock,
    );

    assert_eq!(new_cron.state, CronState::Disabled);
    assert!(effects.is_empty());
}

#[test]
fn multiple_cycles_increment_run_count() {
    let clock = FakeClock::new();
    let mut cron = make_enabled_cron(&clock);

    for i in 1..=3 {
        (cron, _) = cron.transition(CronEvent::Tick, &clock);
        assert_eq!(cron.state, CronState::Running);

        (cron, _) = cron.transition(CronEvent::Complete, &clock);
        assert_eq!(cron.state, CronState::Enabled);
        assert_eq!(cron.run_count, i);
    }
}

#[test]
fn timer_id_format() {
    let clock = FakeClock::new();
    let cron = make_cron(&clock);

    assert_eq!(cron.timer_id(), "cron:test");
}

#[test]
fn is_active_returns_correct_values() {
    let clock = FakeClock::new();

    // Disabled cron is not active
    let cron = make_cron(&clock);
    assert!(!cron.is_active());

    // Enabled cron is active
    let cron = make_enabled_cron(&clock);
    assert!(cron.is_active());

    // Running cron is active
    let (cron, _) = cron.transition(CronEvent::Tick, &clock);
    assert!(cron.is_active());
}

#[test]
fn cron_state_display() {
    assert_eq!(CronState::Enabled.to_string(), "enabled");
    assert_eq!(CronState::Disabled.to_string(), "disabled");
    assert_eq!(CronState::Running.to_string(), "running");
}

#[test]
fn cron_state_from_str() {
    assert_eq!("enabled".parse::<CronState>().unwrap(), CronState::Enabled);
    assert_eq!(
        "disabled".parse::<CronState>().unwrap(),
        CronState::Disabled
    );
    assert_eq!("running".parse::<CronState>().unwrap(), CronState::Running);
    assert!("invalid".parse::<CronState>().is_err());
}

#[test]
fn cron_id_conversions() {
    let id = CronId::new("test");
    assert_eq!(id.to_string(), "test");

    let id: CronId = "test".into();
    assert_eq!(id.0, "test");

    let id: CronId = "test".to_string().into();
    assert_eq!(id.0, "test");
}

#[test]
fn cron_config_builder() {
    let config = CronConfig::new("test", Duration::from_secs(60));
    assert_eq!(config.name, "test");
    assert_eq!(config.interval, Duration::from_secs(60));
    assert!(!config.enabled);

    let config = config.enabled();
    assert!(config.enabled);
}

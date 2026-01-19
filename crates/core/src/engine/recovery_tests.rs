use super::*;
use crate::clock::{Clock, FakeClock};
use crate::pipeline::PipelineId;

fn make_stuck_task(clock: &impl crate::clock::Clock) -> Task {
    let mut task = Task::new(
        "task-1",
        PipelineId("pipeline-1".to_string()),
        "execute",
        Duration::from_secs(30),
        Duration::from_secs(120),
        clock,
    );
    // Manually set to stuck state
    task.state = TaskState::Stuck {
        since: clock.now(),
        nudge_count: 0,
    };
    task
}

#[test]
fn recovery_starts_with_nudge() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let state = RecoveryState::default();
    let config = RecoveryConfig::default();

    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Nudge);
}

#[test]
fn recovery_waits_for_nudge_cooldown() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let mut state = RecoveryState::default();
    let config = RecoveryConfig::default();

    // Record a nudge
    state.record_nudge(clock.now());

    // Should wait for cooldown
    let action = state.next_action(&task, &config, clock.now());
    assert!(matches!(action, RecoveryAction::Wait { .. }));
}

#[test]
fn recovery_nudges_again_after_cooldown() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let mut state = RecoveryState::default();
    let config = RecoveryConfig::default();

    // Record a nudge
    state.record_nudge(clock.now());

    // Advance past cooldown
    clock.advance(config.nudge_cooldown + Duration::from_secs(1));

    // Should nudge again
    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Nudge);
}

#[test]
fn recovery_restarts_after_max_nudges() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let mut state = RecoveryState::default();
    let config = RecoveryConfig::default();

    // Exhaust nudges
    for _ in 0..config.max_nudges {
        state.record_nudge(clock.now());
        clock.advance(config.nudge_cooldown + Duration::from_secs(1));
    }

    // Should restart
    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Restart);
}

#[test]
fn recovery_escalates_after_max_restarts() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let mut state = RecoveryState::default();
    let config = RecoveryConfig::default();

    // Exhaust nudges
    for _ in 0..config.max_nudges {
        state.record_nudge(clock.now());
        clock.advance(config.nudge_cooldown + Duration::from_secs(1));
    }

    // Exhaust restarts
    for _ in 0..config.max_restarts {
        state.record_restart(clock.now());
        clock.advance(config.restart_cooldown + Duration::from_secs(1));
        // Need to re-exhaust nudges after each restart
        for _ in 0..config.max_nudges {
            state.record_nudge(clock.now());
            clock.advance(config.nudge_cooldown + Duration::from_secs(1));
        }
    }

    // Should escalate
    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::Escalate);
}

#[test]
fn recovery_restart_resets_nudge_count() {
    let clock = FakeClock::new();
    let mut state = RecoveryState::default();

    // Record some nudges
    state.record_nudge(clock.now());
    state.record_nudge(clock.now());
    assert_eq!(state.nudge_count, 2);

    // Restart
    state.record_restart(clock.now());

    // Nudge count should be reset
    assert_eq!(state.nudge_count, 0);
    assert!(state.last_nudge.is_none());
}

#[test]
fn recovery_no_action_for_non_stuck_task() {
    let clock = FakeClock::new();
    let task = Task::new(
        "task-1",
        PipelineId("pipeline-1".to_string()),
        "execute",
        Duration::from_secs(30),
        Duration::from_secs(120),
        &clock,
    );
    let state = RecoveryState::default();
    let config = RecoveryConfig::default();

    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::None);
}

#[test]
fn recovery_no_action_after_escalation() {
    let clock = FakeClock::new();
    let task = make_stuck_task(&clock);
    let mut state = RecoveryState::default();
    let config = RecoveryConfig::default();

    state.record_escalation();

    let action = state.next_action(&task, &config, clock.now());
    assert_eq!(action, RecoveryAction::None);
}

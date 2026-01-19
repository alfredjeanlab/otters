// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use std::time::{Duration, Instant};

fn make_action() -> Action {
    let config = ActionConfig::new("test-action", Duration::from_secs(60));
    Action::new(ActionId::new("test"), config)
}

#[test]
fn new_action_is_ready() {
    let action = make_action();

    assert!(matches!(action.state, ActionState::Ready));
    assert!(action.last_executed.is_none());
    assert_eq!(action.execution_count, 0);
}

#[test]
fn can_trigger_when_ready() {
    let action = make_action();

    assert!(action.can_trigger());
}

#[test]
fn trigger_starts_executing() {
    let clock = FakeClock::new();
    let action = make_action();

    let (action, effects) = action.transition(
        ActionEvent::Trigger {
            source: "test-source".to_string(),
        },
        &clock,
    );

    assert!(matches!(action.state, ActionState::Executing));

    // Should emit ActionTriggered event
    assert_eq!(effects.len(), 1);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionTriggered { id, source })
        if id == "test" && source == "test-source")
    ));
}

#[test]
fn complete_enters_cooldown() {
    let clock = FakeClock::new();
    let action = make_action();

    // Trigger first
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );
    assert!(matches!(action.state, ActionState::Executing));

    // Complete
    let (action, effects) = action.transition(ActionEvent::Complete, &clock);

    assert!(matches!(action.state, ActionState::Cooling { .. }));
    assert!(action.last_executed.is_some());
    assert_eq!(action.execution_count, 1);

    // Should emit SetTimer and ActionCompleted
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id == "action:test:cooldown" && *duration == Duration::from_secs(60))));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionCompleted { id })
        if id == "test")));
}

#[test]
fn fail_enters_cooldown() {
    let clock = FakeClock::new();
    let action = make_action();

    // Trigger first
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );

    // Fail
    let (action, effects) = action.transition(
        ActionEvent::Fail {
            error: "test error".to_string(),
        },
        &clock,
    );

    assert!(matches!(action.state, ActionState::Cooling { .. }));
    assert_eq!(action.execution_count, 1);

    // Should emit SetTimer and ActionFailed
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "action:test:cooldown")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionFailed { id, error })
        if id == "test" && error == "test error")
    ));
}

#[test]
fn trigger_rejected_during_cooldown() {
    let clock = FakeClock::new();
    let action = make_action();

    // Trigger and complete to enter cooldown
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );
    let (action, _) = action.transition(ActionEvent::Complete, &clock);
    assert!(matches!(action.state, ActionState::Cooling { .. }));

    // Try to trigger again
    let (action, effects) = action.transition(
        ActionEvent::Trigger {
            source: "second".to_string(),
        },
        &clock,
    );

    // State unchanged
    assert!(matches!(action.state, ActionState::Cooling { .. }));

    // Should emit ActionRejected
    assert_eq!(effects.len(), 1);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionRejected { id, source, reason })
        if id == "test" && source == "second" && reason.contains("cooldown"))
    ));
}

#[test]
fn trigger_rejected_during_execution() {
    let clock = FakeClock::new();
    let action = make_action();

    // Trigger first
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "first".to_string(),
        },
        &clock,
    );
    assert!(matches!(action.state, ActionState::Executing));

    // Try to trigger again while executing
    let (action, effects) = action.transition(
        ActionEvent::Trigger {
            source: "second".to_string(),
        },
        &clock,
    );

    // State unchanged
    assert!(matches!(action.state, ActionState::Executing));

    // Should emit ActionRejected
    assert_eq!(effects.len(), 1);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionRejected { id, source, reason })
        if id == "test" && source == "second" && reason == "already executing")
    ));
}

#[test]
fn cooldown_expired_returns_to_ready() {
    let clock = FakeClock::new();
    let action = make_action();

    // Trigger, complete, then expire cooldown
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );
    let (action, _) = action.transition(ActionEvent::Complete, &clock);
    assert!(matches!(action.state, ActionState::Cooling { .. }));

    let (action, effects) = action.transition(ActionEvent::CooldownExpired, &clock);

    assert!(matches!(action.state, ActionState::Ready));

    // Should emit ActionReady
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionReady { id })
        if id == "test")));
}

#[test]
fn can_trigger_after_cooldown_expires() {
    let clock = FakeClock::new();
    let action = make_action();

    // Full cycle
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "first".to_string(),
        },
        &clock,
    );
    let (action, _) = action.transition(ActionEvent::Complete, &clock);
    let (action, _) = action.transition(ActionEvent::CooldownExpired, &clock);
    assert!(action.can_trigger());

    // Should be able to trigger again
    let (action, effects) = action.transition(
        ActionEvent::Trigger {
            source: "second".to_string(),
        },
        &clock,
    );

    assert!(matches!(action.state, ActionState::Executing));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionTriggered { id, source })
        if id == "test" && source == "second")
    ));
}

#[test]
fn multiple_cycles_increment_count() {
    let clock = FakeClock::new();
    let mut action = make_action();

    for i in 1..=3 {
        (action, _) = action.transition(
            ActionEvent::Trigger {
                source: format!("cycle-{}", i),
            },
            &clock,
        );
        (action, _) = action.transition(ActionEvent::Complete, &clock);
        assert_eq!(action.execution_count, i);
        (action, _) = action.transition(ActionEvent::CooldownExpired, &clock);
    }
}

#[test]
fn cooldown_timer_id_format() {
    let action = make_action();
    assert_eq!(action.cooldown_timer_id(), "action:test:cooldown");
}

#[test]
fn is_on_cooldown() {
    let clock = FakeClock::new();
    let action = make_action();

    assert!(!action.is_on_cooldown());

    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );
    assert!(!action.is_on_cooldown());

    let (action, _) = action.transition(ActionEvent::Complete, &clock);
    assert!(action.is_on_cooldown());

    let (action, _) = action.transition(ActionEvent::CooldownExpired, &clock);
    assert!(!action.is_on_cooldown());
}

#[test]
fn remaining_cooldown() {
    let clock = FakeClock::new();
    let action = make_action();

    // No cooldown when ready
    assert!(action.remaining_cooldown(&clock).is_none());

    // Enter cooldown
    let (action, _) = action.transition(
        ActionEvent::Trigger {
            source: "test".to_string(),
        },
        &clock,
    );
    let (action, _) = action.transition(ActionEvent::Complete, &clock);

    // Should have remaining cooldown
    let remaining = action.remaining_cooldown(&clock);
    assert!(remaining.is_some());
    assert!(remaining.unwrap() <= Duration::from_secs(60));

    // Advance time
    clock.advance(Duration::from_secs(30));
    let remaining = action.remaining_cooldown(&clock);
    assert!(remaining.is_some());
    assert!(remaining.unwrap() <= Duration::from_secs(30));
}

#[test]
fn action_state_display() {
    assert_eq!(ActionState::Ready.to_string(), "ready");
    assert_eq!(
        ActionState::Cooling {
            until: Instant::now()
        }
        .to_string(),
        "cooling"
    );
    assert_eq!(ActionState::Executing.to_string(), "executing");
}

#[test]
fn action_id_conversions() {
    let id = ActionId::new("test");
    assert_eq!(id.to_string(), "test");

    let id: ActionId = "test".into();
    assert_eq!(id.0, "test");

    let id: ActionId = "test".to_string().into();
    assert_eq!(id.0, "test");
}

#[test]
fn action_config_builder() {
    let config = ActionConfig::new("test", Duration::from_secs(60));
    assert_eq!(config.name, "test");
    assert_eq!(config.cooldown, Duration::from_secs(60));
}

#[test]
fn complete_while_ready_is_noop() {
    let clock = FakeClock::new();
    let action = make_action();

    let (new_action, effects) = action.transition(ActionEvent::Complete, &clock);

    assert!(matches!(new_action.state, ActionState::Ready));
    assert!(effects.is_empty());
}

#[test]
fn cooldown_expired_while_ready_is_noop() {
    let clock = FakeClock::new();
    let action = make_action();

    let (new_action, effects) = action.transition(ActionEvent::CooldownExpired, &clock);

    assert!(matches!(new_action.state, ActionState::Ready));
    assert!(effects.is_empty());
}

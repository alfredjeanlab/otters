// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::Event;
use std::time::Duration;

fn create_test_strategy(clock: &impl Clock) -> Strategy {
    let attempts = vec![
        Attempt::with_run(
            "fast-forward",
            "git merge --ff-only",
            Duration::from_secs(60),
        ),
        Attempt::with_run("rebase", "git rebase main", Duration::from_secs(120))
            .with_rollback("git rebase --abort"),
        Attempt::with_task("agent-merge", "merge_agent", Duration::from_secs(300)),
    ];

    Strategy::new("test-strategy-1", "merge", attempts, clock)
}

// ============================================================================
// Basic state transitions
// ============================================================================

#[test]
fn new_strategy_starts_in_ready_state() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    assert!(matches!(strategy.state, StrategyState::Ready));
    assert_eq!(strategy.current_attempt, 0);
    assert!(!strategy.is_terminal());
}

#[test]
fn start_without_checkpoint_begins_first_attempt() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    let (strategy, effects) = strategy.transition(StrategyEvent::Start, &clock);

    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));

    // Should have: RunAttempt, SetAttemptTimer, StrategyStarted, StrategyAttemptStarted
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_index: 0,
            ..
        }
    )));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::SetAttemptTimer {
            attempt_index: 0,
            ..
        }
    )));
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategyStarted { .. }))));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(Event::StrategyAttemptStarted { index: 0, .. })
    )));
}

#[test]
fn start_with_checkpoint_goes_to_checkpointing() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock).with_checkpoint("git rev-parse HEAD");

    let (strategy, effects) = strategy.transition(StrategyEvent::Start, &clock);

    assert!(matches!(strategy.state, StrategyState::Checkpointing));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunCheckpoint { command } if command == "git rev-parse HEAD"
    )));
}

#[test]
fn checkpoint_complete_starts_first_attempt() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock).with_checkpoint("git rev-parse HEAD");

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, effects) = strategy.transition(
        StrategyEvent::CheckpointComplete {
            value: "abc123".to_string(),
        },
        &clock,
    );

    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert_eq!(strategy.checkpoint_value, Some("abc123".to_string()));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_index: 0,
            ..
        }
    )));
}

#[test]
fn checkpoint_failure_transitions_to_failed() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock).with_checkpoint("git rev-parse HEAD");

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, effects) = strategy.transition(
        StrategyEvent::CheckpointFailed {
            reason: "not a git repo".to_string(),
        },
        &clock,
    );

    assert!(matches!(strategy.state, StrategyState::Failed { .. }));
    assert!(strategy.is_terminal());
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategyFailed { .. }))));
}

// ============================================================================
// Attempt success/failure
// ============================================================================

#[test]
fn attempt_success_transitions_to_succeeded() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, effects) = strategy.transition(StrategyEvent::AttemptSucceeded, &clock);

    assert!(matches!(
        strategy.state,
        StrategyState::Succeeded { ref attempt_name } if attempt_name == "fast-forward"
    ));
    assert!(strategy.is_terminal());
    assert!(strategy.succeeded());
    assert_eq!(strategy.successful_attempt(), Some("fast-forward"));

    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::CancelAttemptTimer { .. })));
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategySucceeded { .. }))));
}

#[test]
fn attempt_failure_without_rollback_tries_next() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // First attempt (fast-forward) has no rollback
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "not fast-forward".to_string(),
        },
        &clock,
    );

    // Should go directly to second attempt
    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));
    assert_eq!(strategy.current_attempt, 1);

    // Should emit failure event (not rolling back) and start next attempt
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(Event::StrategyAttemptFailed {
            rolling_back: false,
            ..
        })
    )));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_index: 1,
            ..
        }
    )));
}

#[test]
fn attempt_failure_with_rollback_goes_to_rolling_back() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Start and fail first attempt
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "not fast-forward".to_string(),
        },
        &clock,
    );

    // Now we're on second attempt (rebase) which has a rollback
    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));

    // Fail second attempt
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "conflict".to_string(),
        },
        &clock,
    );

    assert!(matches!(
        strategy.state,
        StrategyState::RollingBack { attempt_index: 1 }
    ));

    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunRollback { attempt_index: 1, command, .. } if command == "git rebase --abort"
    )));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(Event::StrategyAttemptFailed {
            rolling_back: true,
            ..
        })
    )));
}

#[test]
fn rollback_complete_tries_next_attempt() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Start → fail fast-forward → try rebase → fail rebase → rolling back
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "not fast-forward".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "conflict".to_string(),
        },
        &clock,
    );

    // Now rollback completes
    let (strategy, effects) = strategy.transition(StrategyEvent::RollbackComplete, &clock);

    // Should try third attempt (agent-merge)
    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 2,
            ..
        }
    ));
    assert_eq!(strategy.current_attempt, 2);

    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(Event::StrategyRollbackComplete { .. })
    )));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::SpawnTask { task_name, .. } if task_name == "merge_agent"
    )));
}

#[test]
fn rollback_failure_transitions_to_failed() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Start → fail fast-forward → try rebase → fail rebase → rolling back
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "not fast-forward".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "conflict".to_string(),
        },
        &clock,
    );

    // Rollback fails
    let (strategy, effects) = strategy.transition(
        StrategyEvent::RollbackFailed {
            reason: "unrecoverable".to_string(),
        },
        &clock,
    );

    assert!(matches!(strategy.state, StrategyState::Failed { .. }));
    assert!(strategy.is_terminal());
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategyFailed { .. }))));
}

// ============================================================================
// Exhaustion
// ============================================================================

#[test]
fn all_attempts_failed_transitions_to_exhausted() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Fail all attempts
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail 1".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail 2".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(StrategyEvent::RollbackComplete, &clock);

    // Now on third attempt (last one)
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail 3".to_string(),
        },
        &clock,
    );

    assert!(matches!(strategy.state, StrategyState::Exhausted));
    assert!(strategy.is_terminal());
    assert!(strategy.is_exhausted());
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategyExhausted { .. }))));
}

#[test]
fn exhaust_action_is_included_in_event() {
    let clock = FakeClock::new();

    // Create a strategy with just one attempt
    let attempts = vec![Attempt::with_run(
        "only",
        "echo hi",
        Duration::from_secs(10),
    )];
    let strategy =
        Strategy::new("test", "single", attempts, &clock).with_on_exhaust(ExhaustAction::Retry {
            after: Duration::from_secs(300),
        });

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "failed".to_string(),
        },
        &clock,
    );

    assert!(matches!(strategy.state, StrategyState::Exhausted));

    let exhaust_event = effects
        .iter()
        .find(|e| matches!(e, StrategyEffect::Emit(Event::StrategyExhausted { .. })));
    assert!(exhaust_event.is_some());
}

// ============================================================================
// Timeout handling
// ============================================================================

#[test]
fn tick_within_timeout_does_nothing() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Advance clock but stay within timeout (60 seconds for first attempt)
    clock.advance(Duration::from_secs(30));

    let (new_strategy, effects) = strategy.transition(StrategyEvent::Tick, &clock);

    // Should still be trying
    assert!(matches!(
        new_strategy.state,
        StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert!(effects.is_empty());
}

#[test]
fn tick_past_timeout_triggers_failure() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Advance clock past timeout (60 seconds for first attempt)
    clock.advance(Duration::from_secs(61));

    let (strategy, effects) = strategy.transition(StrategyEvent::Tick, &clock);

    // Should have moved to next attempt (timeout treated as failure)
    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));

    // Should have canceled timer and started next attempt
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::CancelAttemptTimer { .. })));
}

// ============================================================================
// Task-based attempts
// ============================================================================

#[test]
fn task_complete_succeeds_strategy() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Get to third attempt (agent-merge task)
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail 1".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail 2".to_string(),
        },
        &clock,
    );
    let (strategy, _) = strategy.transition(StrategyEvent::RollbackComplete, &clock);

    // Now on agent-merge task
    assert!(matches!(
        strategy.state,
        StrategyState::Trying {
            attempt_index: 2,
            ..
        }
    ));

    // Task completes
    let (strategy, effects) = strategy.transition(
        StrategyEvent::TaskComplete {
            task_id: TaskId("task-1".to_string()),
        },
        &clock,
    );

    assert!(matches!(
        strategy.state,
        StrategyState::Succeeded { ref attempt_name } if attempt_name == "agent-merge"
    ));
    assert!(effects
        .iter()
        .any(|e| matches!(e, StrategyEffect::Emit(Event::StrategySucceeded { .. }))));
}

#[test]
fn task_failure_is_handled_like_attempt_failure() {
    let clock = FakeClock::new();

    // Create strategy with only task-based attempts
    let attempts = vec![Attempt::with_task(
        "task1",
        "my_task",
        Duration::from_secs(60),
    )];
    let strategy = Strategy::new("test", "task-only", attempts, &clock);

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    let (strategy, effects) = strategy.transition(
        StrategyEvent::TaskFailed {
            task_id: TaskId("task-1".to_string()),
            reason: "task error".to_string(),
        },
        &clock,
    );

    // Should be exhausted (only one attempt)
    assert!(matches!(strategy.state, StrategyState::Exhausted));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(Event::StrategyAttemptFailed { reason, .. }) if reason == "task error"
    )));
}

#[test]
fn task_assigned_updates_current_task_id() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_task(
        "task1",
        "my_task",
        Duration::from_secs(60),
    )];
    let strategy = Strategy::new("test", "task-strategy", attempts, &clock);

    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(strategy.current_task_id.is_none());

    let (strategy, _) = strategy.transition(
        StrategyEvent::TaskAssigned {
            task_id: TaskId("assigned-task".to_string()),
        },
        &clock,
    );

    assert_eq!(
        strategy.current_task_id,
        Some(TaskId("assigned-task".to_string()))
    );
}

// ============================================================================
// Checkpoint value available to rollback
// ============================================================================

#[test]
fn rollback_receives_checkpoint_value() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_run("attempt", "try something", Duration::from_secs(60))
            .with_rollback("git reset --hard {checkpoint}"),
    ];
    let strategy = Strategy::new("test", "with-checkpoint", attempts, &clock)
        .with_checkpoint("git rev-parse HEAD");

    // Start → checkpointing
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Checkpoint complete with value
    let (strategy, _) = strategy.transition(
        StrategyEvent::CheckpointComplete {
            value: "abc123".to_string(),
        },
        &clock,
    );

    // Attempt fails
    let (_, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "failed".to_string(),
        },
        &clock,
    );

    // Rollback should have checkpoint value
    let rollback_effect = effects
        .iter()
        .find(|e| matches!(e, StrategyEffect::RunRollback { .. }));
    assert!(matches!(
        rollback_effect,
        Some(StrategyEffect::RunRollback { checkpoint_value: Some(v), .. }) if v == "abc123"
    ));
}

// ============================================================================
// Invalid transitions
// ============================================================================

#[test]
fn invalid_transitions_are_no_ops() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Start from Ready
    let (strategy_after_start, _) = strategy.transition(StrategyEvent::Start, &clock);

    // These should be no-ops in Trying state
    let (s, effects) = strategy_after_start.transition(StrategyEvent::Start, &clock);
    assert_eq!(s.state, strategy_after_start.state);
    assert!(effects.is_empty());

    let (s, effects) = strategy_after_start.transition(
        StrategyEvent::CheckpointComplete {
            value: "x".to_string(),
        },
        &clock,
    );
    assert_eq!(s.state, strategy_after_start.state);
    assert!(effects.is_empty());

    let (s, effects) = strategy_after_start.transition(StrategyEvent::RollbackComplete, &clock);
    assert_eq!(s.state, strategy_after_start.state);
    assert!(effects.is_empty());
}

#[test]
fn terminal_states_ignore_events() {
    let clock = FakeClock::new();
    let strategy = create_test_strategy(&clock);

    // Get to succeeded state
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    let (strategy, _) = strategy.transition(StrategyEvent::AttemptSucceeded, &clock);

    assert!(strategy.is_terminal());

    // All events should be no-ops
    let (s, effects) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(matches!(s.state, StrategyState::Succeeded { .. }));
    assert!(effects.is_empty());

    let (s, effects) = strategy.transition(StrategyEvent::AttemptSucceeded, &clock);
    assert!(matches!(s.state, StrategyState::Succeeded { .. }));
    assert!(effects.is_empty());

    let (s, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "x".to_string(),
        },
        &clock,
    );
    assert!(matches!(s.state, StrategyState::Succeeded { .. }));
    assert!(effects.is_empty());
}

// ============================================================================
// Determinism
// ============================================================================

#[test]
fn same_state_and_event_produce_same_result() {
    // Use the same clock for both strategies to ensure deterministic instants
    let clock = FakeClock::new();

    let strategy1 = create_test_strategy(&clock);
    let strategy2 = create_test_strategy(&clock);

    // Apply same events
    let (s1, e1) = strategy1.transition(StrategyEvent::Start, &clock);
    let (s2, e2) = strategy2.transition(StrategyEvent::Start, &clock);

    // Compare state names (avoiding Instant comparison issues)
    assert_eq!(s1.state.name(), s2.state.name());
    assert_eq!(s1.current_attempt, s2.current_attempt);
    assert_eq!(e1.len(), e2.len());

    let (s1, e1) = s1.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail".to_string(),
        },
        &clock,
    );
    let (s2, e2) = s2.transition(
        StrategyEvent::AttemptFailed {
            reason: "fail".to_string(),
        },
        &clock,
    );

    assert_eq!(s1.state.name(), s2.state.name());
    assert_eq!(s1.current_attempt, s2.current_attempt);
    assert_eq!(e1.len(), e2.len());
}

// ============================================================================
// Helper methods
// ============================================================================

#[test]
fn attempt_constructors_work_correctly() {
    let run_attempt = Attempt::with_run("name", "command", Duration::from_secs(30));
    assert_eq!(run_attempt.name, "name");
    assert_eq!(run_attempt.run, Some("command".to_string()));
    assert!(run_attempt.task.is_none());
    assert_eq!(run_attempt.timeout, Duration::from_secs(30));
    assert!(run_attempt.rollback.is_none());

    let task_attempt = Attempt::with_task("task-name", "my_task", Duration::from_secs(60));
    assert_eq!(task_attempt.name, "task-name");
    assert!(task_attempt.run.is_none());
    assert_eq!(task_attempt.task, Some("my_task".to_string()));

    let with_rollback = run_attempt.with_rollback("rollback cmd");
    assert_eq!(with_rollback.rollback, Some("rollback cmd".to_string()));
}

#[test]
fn strategy_builder_methods_work() {
    let clock = FakeClock::new();
    let strategy = Strategy::new("id", "name", vec![], &clock)
        .with_checkpoint("checkpoint cmd")
        .with_on_exhaust(ExhaustAction::Fail);

    assert_eq!(strategy.checkpoint, Some("checkpoint cmd".to_string()));
    assert_eq!(strategy.on_exhaust, ExhaustAction::Fail);
}

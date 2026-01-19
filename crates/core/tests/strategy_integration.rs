// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Integration tests for strategy execution.
//!
//! Tests the full strategy lifecycle including checkpoint, rollback, and exhaustion.

use oj_core::clock::FakeClock;
use oj_core::strategy::{Attempt, ExhaustAction, Strategy, StrategyEffect, StrategyEvent};
use std::time::Duration;

// =============================================================================
// Strategy Lifecycle Tests
// =============================================================================

#[test]
fn strategy_tries_approaches_in_order() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_run("fast", "echo fast", Duration::from_secs(60)),
        Attempt::with_run("medium", "echo medium", Duration::from_secs(120)),
        Attempt::with_run("slow", "echo slow", Duration::from_secs(300)),
    ];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start should begin first attempt
    let (strategy, effects) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_name,
            ..
        } if attempt_name == "fast"
    )));

    // First attempt fails, should try second
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "failed".to_string(),
        },
        &clock,
    );
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_name,
            ..
        } if attempt_name == "medium"
    )));

    // Second attempt succeeds
    let (strategy, _effects) = strategy.transition(StrategyEvent::AttemptSucceeded, &clock);
    assert!(strategy.succeeded());
    assert_eq!(strategy.successful_attempt(), Some("medium"));
}

#[test]
fn strategy_rolls_back_on_failure() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_run("first", "make changes", Duration::from_secs(60))
            .with_rollback("git reset --hard {checkpoint}"),
        Attempt::with_run("second", "different approach", Duration::from_secs(120)),
    ];

    let strategy =
        Strategy::new("strat-1", "test", attempts, &clock).with_checkpoint("git rev-parse HEAD");

    // Start - should run checkpoint first
    let (strategy, effects) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Checkpointing
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunCheckpoint { command } if command == "git rev-parse HEAD"
    )));

    // Checkpoint complete
    let (strategy, effects) = strategy.transition(
        StrategyEvent::CheckpointComplete {
            value: "abc123".to_string(),
        },
        &clock,
    );
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert_eq!(strategy.checkpoint_value, Some("abc123".to_string()));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_name,
            ..
        } if attempt_name == "first"
    )));

    // First attempt fails - should rollback
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "merge conflict".to_string(),
        },
        &clock,
    );
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::RollingBack { attempt_index: 0 }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunRollback {
            checkpoint_value: Some(ref v),
            ..
        } if v == "abc123"
    )));

    // Rollback complete - should try second attempt
    let (strategy, effects) = strategy.transition(StrategyEvent::RollbackComplete, &clock);
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::RunAttempt {
            attempt_name,
            ..
        } if attempt_name == "second"
    )));
}

#[test]
fn strategy_escalates_on_exhaust() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_run(
        "only",
        "try something",
        Duration::from_secs(60),
    )];

    let strategy =
        Strategy::new("strat-1", "test", attempts, &clock).with_on_exhaust(ExhaustAction::Escalate);

    // Start and run first (only) attempt
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Attempt fails - should be exhausted since no more attempts
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "failed".to_string(),
        },
        &clock,
    );

    assert!(strategy.is_exhausted());
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(oj_core::effect::Event::StrategyExhausted {
            action: ExhaustAction::Escalate,
            ..
        })
    )));
}

#[test]
fn strategy_fails_on_exhaust() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_run("attempt1", "cmd1", Duration::from_secs(60)),
        Attempt::with_run("attempt2", "cmd2", Duration::from_secs(60)),
    ];

    let strategy =
        Strategy::new("strat-1", "test", attempts, &clock).with_on_exhaust(ExhaustAction::Fail);

    // Start
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // First fails
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "error".to_string(),
        },
        &clock,
    );

    // Second fails - should be exhausted
    let (strategy, effects) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "error".to_string(),
        },
        &clock,
    );

    assert!(strategy.is_exhausted());
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(oj_core::effect::Event::StrategyExhausted {
            action: ExhaustAction::Fail,
            ..
        })
    )));
}

// =============================================================================
// Timeout Handling
// =============================================================================

#[test]
fn strategy_handles_timeout() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_run("slow", "long running cmd", Duration::from_secs(60)),
        Attempt::with_run("fast", "quick cmd", Duration::from_secs(30)),
    ];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start first attempt
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));

    // Advance clock past timeout
    clock.advance(Duration::from_secs(65));

    // Tick should detect timeout and fail the attempt
    let (strategy, _) = strategy.transition(StrategyEvent::Tick, &clock);

    // Should be trying second attempt now
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));
}

#[test]
fn strategy_tick_does_nothing_within_timeout() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_run("attempt", "cmd", Duration::from_secs(60))];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start first attempt
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Advance clock but stay within timeout
    clock.advance(Duration::from_secs(30));

    // Tick should do nothing
    let (strategy, effects) = strategy.transition(StrategyEvent::Tick, &clock);

    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert!(effects.is_empty());
}

// =============================================================================
// Task-Based Attempts
// =============================================================================

#[test]
fn strategy_spawns_task_for_task_based_attempt() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_task(
        "agent",
        "merge_agent",
        Duration::from_secs(300),
    )];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start - should spawn task
    let (strategy, effects) = strategy.transition(StrategyEvent::Start, &clock);

    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 0,
            ..
        }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::SpawnTask {
            task_name,
            ..
        } if task_name == "merge_agent"
    )));
}

#[test]
fn strategy_task_completion_succeeds_strategy() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_task(
        "agent",
        "test_agent",
        Duration::from_secs(60),
    )];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Assign task
    let (strategy, _) = strategy.transition(
        StrategyEvent::TaskAssigned {
            task_id: oj_core::task::TaskId("task-1".to_string()),
        },
        &clock,
    );
    assert!(strategy.current_task_id.is_some());

    // Task completes
    let (strategy, _) = strategy.transition(
        StrategyEvent::TaskComplete {
            task_id: oj_core::task::TaskId("task-1".to_string()),
        },
        &clock,
    );

    assert!(strategy.succeeded());
    assert_eq!(strategy.successful_attempt(), Some("agent"));
}

#[test]
fn strategy_task_failure_tries_next_attempt() {
    let clock = FakeClock::new();

    let attempts = vec![
        Attempt::with_task("first_agent", "agent1", Duration::from_secs(60)),
        Attempt::with_run("fallback", "simple cmd", Duration::from_secs(60)),
    ];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Task fails
    let (strategy, _) = strategy.transition(
        StrategyEvent::TaskFailed {
            task_id: oj_core::task::TaskId("task-1".to_string()),
            reason: "agent failed".to_string(),
        },
        &clock,
    );

    // Should try second attempt
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Trying {
            attempt_index: 1,
            ..
        }
    ));
}

// =============================================================================
// Rollback Failure
// =============================================================================

#[test]
fn strategy_rollback_failure_is_fatal() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_run("first", "cmd", Duration::from_secs(60))
        .with_rollback("git reset --hard")];

    let strategy = Strategy::new("strat-1", "test", attempts, &clock);

    // Start
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);

    // Attempt fails, triggers rollback
    let (strategy, _) = strategy.transition(
        StrategyEvent::AttemptFailed {
            reason: "failed".to_string(),
        },
        &clock,
    );
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::RollingBack { .. }
    ));

    // Rollback fails - should be fatal
    let (strategy, effects) = strategy.transition(
        StrategyEvent::RollbackFailed {
            reason: "reset failed".to_string(),
        },
        &clock,
    );

    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Failed { .. }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(oj_core::effect::Event::StrategyFailed { .. })
    )));
}

// =============================================================================
// Checkpoint Failure
// =============================================================================

#[test]
fn strategy_checkpoint_failure_is_fatal() {
    let clock = FakeClock::new();

    let attempts = vec![Attempt::with_run("attempt", "cmd", Duration::from_secs(60))];

    let strategy =
        Strategy::new("strat-1", "test", attempts, &clock).with_checkpoint("git rev-parse HEAD");

    // Start - triggers checkpoint
    let (strategy, _) = strategy.transition(StrategyEvent::Start, &clock);
    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Checkpointing
    ));

    // Checkpoint fails
    let (strategy, effects) = strategy.transition(
        StrategyEvent::CheckpointFailed {
            reason: "not a git repo".to_string(),
        },
        &clock,
    );

    assert!(matches!(
        strategy.state,
        oj_core::strategy::StrategyState::Failed { .. }
    ));
    assert!(effects.iter().any(|e| matches!(
        e,
        StrategyEffect::Emit(oj_core::effect::Event::StrategyFailed { .. })
    )));
}

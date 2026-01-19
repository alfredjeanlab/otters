// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Strategy state machine
//!
//! A strategy represents a fallback chain that tries approaches in order,
//! with optional checkpoint/rollback semantics. When an attempt fails,
//! the strategy either:
//! - Rolls back (if rollback defined) then tries the next attempt
//! - Directly tries the next attempt (if no rollback)
//! - Exhausts (if no more attempts) and takes the configured action

use crate::clock::Clock;
use crate::effect::Event;
use crate::task::TaskId;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Unique identifier for a strategy instance
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub String);

impl std::fmt::Display for StrategyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StrategyId {
    fn from(s: String) -> Self {
        StrategyId(s)
    }
}

impl From<&str> for StrategyId {
    fn from(s: &str) -> Self {
        StrategyId(s.to_string())
    }
}

/// Definition of an attempt within a strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// Human-readable name for this attempt
    pub name: String,
    /// Shell command to run (mutually exclusive with task)
    pub run: Option<String>,
    /// Task to spawn (mutually exclusive with run)
    pub task: Option<String>,
    /// Maximum time for this attempt
    pub timeout: Duration,
    /// Rollback command to run on failure (has access to checkpoint value)
    pub rollback: Option<String>,
}

impl Attempt {
    /// Create a new attempt with a shell command
    pub fn with_run(
        name: impl Into<String>,
        command: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            name: name.into(),
            run: Some(command.into()),
            task: None,
            timeout,
            rollback: None,
        }
    }

    /// Create a new attempt with a task
    pub fn with_task(name: impl Into<String>, task: impl Into<String>, timeout: Duration) -> Self {
        Self {
            name: name.into(),
            run: None,
            task: Some(task.into()),
            timeout,
            rollback: None,
        }
    }

    /// Add a rollback command to this attempt
    pub fn with_rollback(mut self, command: impl Into<String>) -> Self {
        self.rollback = Some(command.into());
        self
    }
}

/// The current state of a strategy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyState {
    /// Strategy created but not started
    Ready,
    /// Running checkpoint command to capture state
    Checkpointing,
    /// Executing an attempt
    Trying {
        attempt_index: usize,
        started_at: Instant,
    },
    /// Rolling back after a failed attempt
    RollingBack { attempt_index: usize },
    /// Strategy completed successfully
    Succeeded { attempt_name: String },
    /// All attempts failed
    Exhausted,
    /// Strategy failed with an unrecoverable error
    Failed { reason: String },
}

impl StrategyState {
    /// Check if this state is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StrategyState::Succeeded { .. }
                | StrategyState::Exhausted
                | StrategyState::Failed { .. }
        )
    }

    /// Get the state name for logging/debugging
    pub fn name(&self) -> &'static str {
        match self {
            StrategyState::Ready => "ready",
            StrategyState::Checkpointing => "checkpointing",
            StrategyState::Trying { .. } => "trying",
            StrategyState::RollingBack { .. } => "rolling_back",
            StrategyState::Succeeded { .. } => "succeeded",
            StrategyState::Exhausted => "exhausted",
            StrategyState::Failed { .. } => "failed",
        }
    }
}

/// Events that can change strategy state
#[derive(Debug, Clone)]
pub enum StrategyEvent {
    /// Start executing the strategy
    Start,
    /// Checkpoint command completed successfully
    CheckpointComplete { value: String },
    /// Checkpoint command failed
    CheckpointFailed { reason: String },
    /// Current attempt succeeded
    AttemptSucceeded,
    /// Current attempt failed
    AttemptFailed { reason: String },
    /// Current attempt timed out
    AttemptTimeout,
    /// Rollback command completed
    RollbackComplete,
    /// Rollback command failed
    RollbackFailed { reason: String },
    /// Periodic tick to check timeouts
    Tick,
    /// Task assigned to this strategy attempt
    TaskAssigned { task_id: TaskId },
    /// Task completed successfully
    TaskComplete { task_id: TaskId },
    /// Task failed
    TaskFailed { task_id: TaskId, reason: String },
}

/// Action to take when all attempts are exhausted
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExhaustAction {
    /// Escalate to external handling (e.g., human intervention)
    #[default]
    Escalate,
    /// Mark as failed
    Fail,
    /// Retry after a delay
    Retry { after: Duration },
}

/// Effects produced by strategy state transitions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyEffect {
    /// Run the checkpoint command
    RunCheckpoint { command: String },
    /// Run an attempt's command
    RunAttempt {
        strategy_id: StrategyId,
        attempt_index: usize,
        attempt_name: String,
        command: String,
        timeout: Duration,
    },
    /// Spawn a task for this attempt
    SpawnTask {
        strategy_id: StrategyId,
        attempt_index: usize,
        attempt_name: String,
        task_name: String,
        timeout: Duration,
    },
    /// Run a rollback command
    RunRollback {
        strategy_id: StrategyId,
        attempt_index: usize,
        command: String,
        checkpoint_value: Option<String>,
    },
    /// Set a timeout timer for the current attempt
    SetAttemptTimer {
        strategy_id: StrategyId,
        attempt_index: usize,
        duration: Duration,
    },
    /// Cancel the attempt timer
    CancelAttemptTimer { strategy_id: StrategyId },
    /// Emit a core event
    Emit(Event),
}

/// A strategy representing a fallback chain of approaches
#[derive(Debug, Clone)]
pub struct Strategy {
    /// Unique identifier for this strategy instance
    pub id: StrategyId,
    /// Human-readable name (from definition)
    pub name: String,
    /// Optional checkpoint command to run before first attempt
    pub checkpoint: Option<String>,
    /// The captured checkpoint value (after checkpoint completes)
    pub checkpoint_value: Option<String>,
    /// Ordered list of attempts to try
    pub attempts: Vec<Attempt>,
    /// Current state
    pub state: StrategyState,
    /// Index of current attempt (0-indexed)
    pub current_attempt: usize,
    /// Action to take when all attempts exhausted
    pub on_exhaust: ExhaustAction,
    /// Current task ID if running a task-based attempt
    pub current_task_id: Option<TaskId>,
    /// When this strategy was created
    pub created_at: Instant,
}

impl Strategy {
    /// Create a new strategy
    pub fn new(
        id: impl Into<StrategyId>,
        name: impl Into<String>,
        attempts: Vec<Attempt>,
        clock: &impl Clock,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            checkpoint: None,
            checkpoint_value: None,
            attempts,
            state: StrategyState::Ready,
            current_attempt: 0,
            on_exhaust: ExhaustAction::default(),
            current_task_id: None,
            created_at: clock.now(),
        }
    }

    /// Set a checkpoint command to run before attempting
    pub fn with_checkpoint(mut self, command: impl Into<String>) -> Self {
        self.checkpoint = Some(command.into());
        self
    }

    /// Set the action to take on exhaustion
    pub fn with_on_exhaust(mut self, action: ExhaustAction) -> Self {
        self.on_exhaust = action;
        self
    }

    /// Get the current attempt (if any)
    pub fn current_attempt(&self) -> Option<&Attempt> {
        self.attempts.get(self.current_attempt)
    }

    /// Check if there are more attempts to try after the given index
    fn has_more_attempts_after(&self, index: usize) -> bool {
        index + 1 < self.attempts.len()
    }

    /// Pure transition function - returns new state and effects
    pub fn transition(
        &self,
        event: StrategyEvent,
        clock: &impl Clock,
    ) -> (Strategy, Vec<StrategyEffect>) {
        let now = clock.now();

        match (&self.state, &event) {
            // Ready → Checkpointing (if checkpoint defined) or Trying { 0 }
            (StrategyState::Ready, StrategyEvent::Start) => {
                if let Some(ref checkpoint_cmd) = self.checkpoint {
                    let mut strategy = self.clone();
                    strategy.state = StrategyState::Checkpointing;

                    let effects = vec![
                        StrategyEffect::RunCheckpoint {
                            command: checkpoint_cmd.clone(),
                        },
                        StrategyEffect::Emit(Event::StrategyStarted {
                            id: self.id.0.clone(),
                            name: self.name.clone(),
                        }),
                    ];
                    (strategy, effects)
                } else {
                    self.start_attempt(0, now)
                }
            }

            // Checkpointing → Trying { 0 }
            (StrategyState::Checkpointing, StrategyEvent::CheckpointComplete { value }) => {
                let mut strategy = self.clone();
                strategy.checkpoint_value = Some(value.clone());
                strategy.start_attempt(0, now)
            }

            // Checkpointing → Failed (checkpoint failed)
            (StrategyState::Checkpointing, StrategyEvent::CheckpointFailed { reason }) => {
                let mut strategy = self.clone();
                strategy.state = StrategyState::Failed {
                    reason: reason.clone(),
                };

                let effects = vec![StrategyEffect::Emit(Event::StrategyFailed {
                    id: self.id.0.clone(),
                    reason: format!("Checkpoint failed: {}", reason),
                })];
                (strategy, effects)
            }

            // Trying → Succeeded
            (StrategyState::Trying { attempt_index, .. }, StrategyEvent::AttemptSucceeded)
            | (StrategyState::Trying { attempt_index, .. }, StrategyEvent::TaskComplete { .. }) => {
                let attempt_name = self
                    .attempts
                    .get(*attempt_index)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();

                let mut strategy = self.clone();
                strategy.state = StrategyState::Succeeded {
                    attempt_name: attempt_name.clone(),
                };
                strategy.current_task_id = None;

                let effects = vec![
                    StrategyEffect::CancelAttemptTimer {
                        strategy_id: self.id.clone(),
                    },
                    StrategyEffect::Emit(Event::StrategySucceeded {
                        id: self.id.0.clone(),
                        attempt: attempt_name,
                    }),
                ];
                (strategy, effects)
            }

            // Trying → RollingBack or Trying { n+1 } or Exhausted (attempt failed)
            (
                StrategyState::Trying { attempt_index, .. },
                StrategyEvent::AttemptFailed { reason },
            ) => self.handle_attempt_failure(*attempt_index, reason.clone(), now),

            (
                StrategyState::Trying { attempt_index, .. },
                StrategyEvent::TaskFailed { reason, .. },
            ) => self.handle_attempt_failure(*attempt_index, reason.clone(), now),

            (StrategyState::Trying { attempt_index, .. }, StrategyEvent::AttemptTimeout) => {
                self.handle_attempt_failure(*attempt_index, "timeout".to_string(), now)
            }

            // RollingBack → Trying { n+1 } or Exhausted
            (StrategyState::RollingBack { attempt_index }, StrategyEvent::RollbackComplete) => {
                let effects = vec![StrategyEffect::Emit(Event::StrategyRollbackComplete {
                    id: self.id.0.clone(),
                    attempt: self
                        .attempts
                        .get(*attempt_index)
                        .map(|a| a.name.clone())
                        .unwrap_or_default(),
                })];

                if self.has_more_attempts_after(*attempt_index) {
                    let (new_strategy, mut new_effects) =
                        self.start_attempt(*attempt_index + 1, now);
                    new_effects.splice(0..0, effects);
                    (new_strategy, new_effects)
                } else {
                    let mut strategy = self.clone();
                    strategy.state = StrategyState::Exhausted;
                    let mut all_effects = effects;
                    all_effects.push(StrategyEffect::Emit(Event::StrategyExhausted {
                        id: self.id.0.clone(),
                        action: self.on_exhaust.clone(),
                    }));
                    (strategy, all_effects)
                }
            }

            // RollingBack → Failed (rollback failed is fatal)
            (StrategyState::RollingBack { .. }, StrategyEvent::RollbackFailed { reason }) => {
                let mut strategy = self.clone();
                strategy.state = StrategyState::Failed {
                    reason: reason.clone(),
                };

                let effects = vec![StrategyEffect::Emit(Event::StrategyFailed {
                    id: self.id.0.clone(),
                    reason: format!("Rollback failed: {}", reason),
                })];
                (strategy, effects)
            }

            // Trying: task assigned
            (StrategyState::Trying { .. }, StrategyEvent::TaskAssigned { task_id }) => {
                let mut strategy = self.clone();
                strategy.current_task_id = Some(task_id.clone());
                (strategy, vec![])
            }

            // Trying: periodic tick to check timeout
            (
                StrategyState::Trying {
                    started_at,
                    attempt_index,
                },
                StrategyEvent::Tick,
            ) => {
                if let Some(attempt) = self.attempts.get(*attempt_index) {
                    let elapsed = now.duration_since(*started_at);
                    if elapsed > attempt.timeout {
                        // Timeout - treat as failure
                        return self.transition(StrategyEvent::AttemptTimeout, clock);
                    }
                }
                (self.clone(), vec![])
            }

            // Invalid transitions - no change
            _ => (self.clone(), vec![]),
        }
    }

    /// Handle an attempt failure (timeout, explicit failure, or task failure)
    fn handle_attempt_failure(
        &self,
        attempt_index: usize,
        reason: String,
        now: Instant,
    ) -> (Strategy, Vec<StrategyEffect>) {
        let attempt = self.attempts.get(attempt_index);
        let has_rollback = attempt.and_then(|a| a.rollback.as_ref()).is_some();

        let mut strategy = self.clone();
        strategy.current_task_id = None;

        let mut effects = vec![StrategyEffect::CancelAttemptTimer {
            strategy_id: self.id.clone(),
        }];

        if has_rollback {
            // Go to RollingBack state
            strategy.state = StrategyState::RollingBack { attempt_index };

            let rollback_cmd = attempt
                .and_then(|a| a.rollback.as_ref())
                .cloned()
                .unwrap_or_default();
            effects.push(StrategyEffect::RunRollback {
                strategy_id: self.id.clone(),
                attempt_index,
                command: rollback_cmd,
                checkpoint_value: self.checkpoint_value.clone(),
            });
            effects.push(StrategyEffect::Emit(Event::StrategyAttemptFailed {
                id: self.id.0.clone(),
                attempt: attempt.map(|a| a.name.clone()).unwrap_or_default(),
                reason,
                rolling_back: true,
            }));

            (strategy, effects)
        } else if self.has_more_attempts_after(attempt_index) {
            // Try next attempt directly
            effects.push(StrategyEffect::Emit(Event::StrategyAttemptFailed {
                id: self.id.0.clone(),
                attempt: attempt.map(|a| a.name.clone()).unwrap_or_default(),
                reason,
                rolling_back: false,
            }));
            let (new_strategy, mut new_effects) = self.start_attempt(attempt_index + 1, now);
            new_effects.splice(0..0, effects);
            (new_strategy, new_effects)
        } else {
            // Exhausted
            strategy.state = StrategyState::Exhausted;
            effects.push(StrategyEffect::Emit(Event::StrategyAttemptFailed {
                id: self.id.0.clone(),
                attempt: attempt.map(|a| a.name.clone()).unwrap_or_default(),
                reason,
                rolling_back: false,
            }));
            effects.push(StrategyEffect::Emit(Event::StrategyExhausted {
                id: self.id.0.clone(),
                action: self.on_exhaust.clone(),
            }));
            (strategy, effects)
        }
    }

    /// Helper to start an attempt at the given index
    fn start_attempt(&self, index: usize, now: Instant) -> (Strategy, Vec<StrategyEffect>) {
        let attempt = match self.attempts.get(index) {
            Some(a) => a,
            None => {
                // No more attempts
                let mut strategy = self.clone();
                strategy.state = StrategyState::Exhausted;
                let effects = vec![StrategyEffect::Emit(Event::StrategyExhausted {
                    id: self.id.0.clone(),
                    action: self.on_exhaust.clone(),
                })];
                return (strategy, effects);
            }
        };

        let mut strategy = self.clone();
        strategy.current_attempt = index;
        strategy.state = StrategyState::Trying {
            attempt_index: index,
            started_at: now,
        };

        let mut effects = vec![];

        // Start the attempt (either run command or spawn task)
        if let Some(ref cmd) = attempt.run {
            effects.push(StrategyEffect::RunAttempt {
                strategy_id: self.id.clone(),
                attempt_index: index,
                attempt_name: attempt.name.clone(),
                command: cmd.clone(),
                timeout: attempt.timeout,
            });
        } else if let Some(ref task_name) = attempt.task {
            effects.push(StrategyEffect::SpawnTask {
                strategy_id: self.id.clone(),
                attempt_index: index,
                attempt_name: attempt.name.clone(),
                task_name: task_name.clone(),
                timeout: attempt.timeout,
            });
        }

        // Set timeout timer
        effects.push(StrategyEffect::SetAttemptTimer {
            strategy_id: self.id.clone(),
            attempt_index: index,
            duration: attempt.timeout,
        });

        // Emit event
        if index == 0 && self.checkpoint.is_none() {
            // First attempt and no checkpoint - emit started event
            effects.push(StrategyEffect::Emit(Event::StrategyStarted {
                id: self.id.0.clone(),
                name: self.name.clone(),
            }));
        }

        effects.push(StrategyEffect::Emit(Event::StrategyAttemptStarted {
            id: self.id.0.clone(),
            attempt: attempt.name.clone(),
            index,
        }));

        (strategy, effects)
    }

    /// Check if strategy is in a terminal state
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Check if strategy succeeded
    pub fn succeeded(&self) -> bool {
        matches!(self.state, StrategyState::Succeeded { .. })
    }

    /// Check if strategy is exhausted
    pub fn is_exhausted(&self) -> bool {
        matches!(self.state, StrategyState::Exhausted)
    }

    /// Get the successful attempt name (if succeeded)
    pub fn successful_attempt(&self) -> Option<&str> {
        match &self.state {
            StrategyState::Succeeded { attempt_name } => Some(attempt_name),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "strategy_tests.rs"]
mod tests;

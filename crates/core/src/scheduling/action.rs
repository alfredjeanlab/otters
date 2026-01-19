// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Action state machine with cooldown enforcement
//!
//! An Action is a named operation with cooldown to prevent rapid-fire execution.
//! This is useful for rate-limiting operations like nudging agents, restarting
//! sessions, or sending notifications.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Unique identifier for an action
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(pub String);

impl ActionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ActionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ActionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// What an action does when triggered
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionExecution {
    /// Run a shell command
    Command {
        run: String,
        #[serde(default, with = "humantime_serde::option")]
        timeout: Option<Duration>,
    },
    /// Invoke a task
    Task {
        task: String,
        #[serde(default)]
        inputs: std::collections::BTreeMap<String, String>,
    },
    /// Decision rules evaluated in order
    Rules { rules: Vec<DecisionRule> },
    /// No execution (just state tracking)
    #[default]
    None,
}

/// A decision rule for rule-based action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRule {
    /// Condition to evaluate (if not present, acts as else clause)
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Whether this is an else clause
    #[serde(rename = "else", skip_serializing_if = "Option::is_none")]
    pub is_else: Option<bool>,
    /// Action to take if condition matches
    pub then: String,
    /// Optional delay before executing
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "humantime_serde::option"
    )]
    pub delay: Option<Duration>,
}

impl DecisionRule {
    pub fn new(then: impl Into<String>) -> Self {
        Self {
            condition: None,
            is_else: None,
            then: then.into(),
            delay: None,
        }
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    pub fn as_else(mut self) -> Self {
        self.is_else = Some(true);
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

/// Configuration for creating an action
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionConfig {
    pub name: String,
    #[serde(with = "humantime_serde")]
    pub cooldown: Duration,
    /// What the action does when triggered
    #[serde(default)]
    pub execution: ActionExecution,
}

impl ActionConfig {
    pub fn new(name: impl Into<String>, cooldown: Duration) -> Self {
        Self {
            name: name.into(),
            cooldown,
            execution: ActionExecution::None,
        }
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.execution = ActionExecution::Command {
            run: command.into(),
            timeout: None,
        };
        self
    }

    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.execution = ActionExecution::Task {
            task: task.into(),
            inputs: std::collections::BTreeMap::new(),
        };
        self
    }

    pub fn with_rules(mut self, rules: Vec<DecisionRule>) -> Self {
        self.execution = ActionExecution::Rules { rules };
        self
    }
}

/// A named operation with cooldown to prevent rapid execution
#[derive(Debug, Clone)]
pub struct Action {
    pub id: ActionId,
    pub name: String,
    pub cooldown: Duration,
    pub execution: ActionExecution,
    pub state: ActionState,
    pub last_executed: Option<Instant>,
    pub execution_count: u64,
}

/// The current state of an action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionState {
    /// Action is ready to execute
    Ready,
    /// Action is on cooldown, cannot execute
    Cooling { until: Instant },
    /// Action is currently executing
    Executing,
}

impl fmt::Display for ActionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionState::Ready => write!(f, "ready"),
            ActionState::Cooling { .. } => write!(f, "cooling"),
            ActionState::Executing => write!(f, "executing"),
        }
    }
}

impl ActionState {
    pub fn from_str_with_until(s: &str, until: Option<Instant>) -> Self {
        match s {
            "ready" => ActionState::Ready,
            "cooling" => ActionState::Cooling {
                until: until.unwrap_or_else(Instant::now),
            },
            "executing" => ActionState::Executing,
            _ => ActionState::Ready,
        }
    }
}

/// Events that can transition an action's state
#[derive(Debug, Clone)]
pub enum ActionEvent {
    /// Attempt to trigger the action
    Trigger { source: String },
    /// Execution completed successfully
    Complete,
    /// Execution failed
    Fail { error: String },
    /// Cooldown period elapsed
    CooldownExpired,
}

impl Action {
    /// Create a new action
    pub fn new(id: ActionId, config: ActionConfig) -> Self {
        Self {
            id,
            name: config.name,
            cooldown: config.cooldown,
            execution: config.execution,
            state: ActionState::Ready,
            last_executed: None,
            execution_count: 0,
        }
    }

    /// Get the timer ID for this action's cooldown
    pub fn cooldown_timer_id(&self) -> String {
        format!("action:{}:cooldown", self.id)
    }

    /// Check if action can be triggered now
    pub fn can_trigger(&self) -> bool {
        matches!(self.state, ActionState::Ready)
    }

    /// Pure state transition returning new state and effects
    pub fn transition(&self, event: ActionEvent, clock: &impl Clock) -> (Self, Vec<Effect>) {
        match (&self.state, event) {
            // Trigger when ready - start executing
            (ActionState::Ready, ActionEvent::Trigger { source }) => {
                let new_state = Action {
                    state: ActionState::Executing,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::ActionTriggered {
                    id: self.id.0.clone(),
                    source,
                })];
                (new_state, effects)
            }

            // Execution completed - enter cooldown
            (ActionState::Executing, ActionEvent::Complete) => {
                let until = clock.now() + self.cooldown;
                let new_state = Action {
                    state: ActionState::Cooling { until },
                    last_executed: Some(clock.now()),
                    execution_count: self.execution_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.cooldown_timer_id(),
                        duration: self.cooldown,
                    },
                    Effect::Emit(Event::ActionCompleted {
                        id: self.id.0.clone(),
                    }),
                ];
                (new_state, effects)
            }

            // Execution failed - enter cooldown anyway to prevent rapid retries
            (ActionState::Executing, ActionEvent::Fail { error }) => {
                let until = clock.now() + self.cooldown;
                let new_state = Action {
                    state: ActionState::Cooling { until },
                    last_executed: Some(clock.now()),
                    execution_count: self.execution_count + 1,
                    ..self.clone()
                };
                let effects = vec![
                    Effect::SetTimer {
                        id: self.cooldown_timer_id(),
                        duration: self.cooldown,
                    },
                    Effect::Emit(Event::ActionFailed {
                        id: self.id.0.clone(),
                        error,
                    }),
                ];
                (new_state, effects)
            }

            // Cooldown expired - ready to execute again
            (ActionState::Cooling { .. }, ActionEvent::CooldownExpired) => {
                let new_state = Action {
                    state: ActionState::Ready,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::ActionReady {
                    id: self.id.0.clone(),
                })];
                (new_state, effects)
            }

            // Trigger during cooldown - rejected
            (ActionState::Cooling { until }, ActionEvent::Trigger { source }) => {
                let remaining = until.saturating_duration_since(clock.now());
                let effects = vec![Effect::Emit(Event::ActionRejected {
                    id: self.id.0.clone(),
                    source,
                    reason: format!("cooldown ({}s remaining)", remaining.as_secs()),
                })];
                (self.clone(), effects)
            }

            // Trigger while executing - rejected
            (ActionState::Executing, ActionEvent::Trigger { source }) => {
                let effects = vec![Effect::Emit(Event::ActionRejected {
                    id: self.id.0.clone(),
                    source,
                    reason: "already executing".to_string(),
                })];
                (self.clone(), effects)
            }

            // Invalid transitions are no-ops
            _ => (self.clone(), vec![]),
        }
    }

    /// Check if the action is on cooldown
    pub fn is_on_cooldown(&self) -> bool {
        matches!(self.state, ActionState::Cooling { .. })
    }

    /// Get the remaining cooldown duration, if any
    pub fn remaining_cooldown(&self, clock: &impl Clock) -> Option<Duration> {
        match &self.state {
            ActionState::Cooling { until } => Some(until.saturating_duration_since(clock.now())),
            _ => None,
        }
    }
}

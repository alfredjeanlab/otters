// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Recovery action chains for stuck tasks

use crate::task::{Task, TaskState};
use std::time::{Duration, Instant};

/// Configuration for recovery actions
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum nudge attempts before restart
    pub max_nudges: u32,
    /// Cooldown between nudges
    pub nudge_cooldown: Duration,
    /// Maximum restart attempts before escalation
    pub max_restarts: u32,
    /// Cooldown between restarts
    pub restart_cooldown: Duration,
    /// Nudge message to send
    pub nudge_message: String,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_nudges: 3,
            nudge_cooldown: Duration::from_secs(60),
            max_restarts: 2,
            restart_cooldown: Duration::from_secs(300),
            nudge_message: "Are you still working? Please run `oj done` when finished or `oj done --error 'reason'` if stuck.".to_string(),
        }
    }
}

/// Recovery state tracked per task
#[derive(Debug, Clone, Default)]
pub struct RecoveryState {
    pub nudge_count: u32,
    pub restart_count: u32,
    pub last_nudge: Option<Instant>,
    pub last_restart: Option<Instant>,
    pub escalated: bool,
}

/// Determines the next recovery action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Send nudge message to session
    Nudge,
    /// Kill session and restart task
    Restart,
    /// Escalate to user (notifications, alerts)
    Escalate,
    /// Wait for cooldown
    Wait { until: Instant },
    /// No action needed
    None,
}

impl RecoveryState {
    /// Determine next action based on task state and config
    pub fn next_action(
        &self,
        task: &Task,
        config: &RecoveryConfig,
        now: Instant,
    ) -> RecoveryAction {
        // Only act on stuck tasks
        let TaskState::Stuck { .. } = &task.state else {
            return RecoveryAction::None;
        };

        // Already escalated
        if self.escalated {
            return RecoveryAction::None;
        }

        // Check nudge cooldown
        if self.nudge_count < config.max_nudges {
            if let Some(last) = self.last_nudge {
                if now < last + config.nudge_cooldown {
                    return RecoveryAction::Wait {
                        until: last + config.nudge_cooldown,
                    };
                }
            }
            return RecoveryAction::Nudge;
        }

        // Check restart cooldown
        if self.restart_count < config.max_restarts {
            if let Some(last) = self.last_restart {
                if now < last + config.restart_cooldown {
                    return RecoveryAction::Wait {
                        until: last + config.restart_cooldown,
                    };
                }
            }
            return RecoveryAction::Restart;
        }

        // All options exhausted
        RecoveryAction::Escalate
    }

    /// Record that a nudge was performed
    pub fn record_nudge(&mut self, now: Instant) {
        self.nudge_count += 1;
        self.last_nudge = Some(now);
    }

    /// Record that a restart was performed
    pub fn record_restart(&mut self, now: Instant) {
        self.restart_count += 1;
        self.last_restart = Some(now);
        // Reset nudge count after restart
        self.nudge_count = 0;
        self.last_nudge = None;
    }

    /// Mark as escalated
    pub fn record_escalation(&mut self) {
        self.escalated = true;
    }
}

#[cfg(test)]
mod tests {
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
}

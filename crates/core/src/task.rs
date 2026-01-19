// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Task state machine
//!
//! A task represents a unit of work assigned to a session.
//! Unlike Session (which tracks tmux process state), Task tracks
//! the logical work being performed.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use crate::pipeline::PipelineId;
use crate::session::SessionId;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Unique identifier for a task
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        TaskId(s)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        TaskId(s.to_string())
    }
}

/// The state of a task
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Task created but not yet assigned to a session
    Pending,
    /// Task is actively being worked on
    Running,
    /// Task has not received heartbeat within threshold
    Stuck { since: Instant, nudge_count: u32 },
    /// Task completed successfully
    Done { output: Option<String> },
    /// Task failed
    Failed { reason: String },
}

/// Events that can change task state
#[derive(Clone, Debug)]
pub enum TaskEvent {
    /// Session assigned, begin work
    Start { session_id: SessionId },
    /// Heartbeat received from session.
    ///
    /// **Note**: This is primarily used for recovering from Stuck state.
    /// Stuck detection now uses `session_idle_time` from `TaskEvent::Tick`.
    /// Session owns heartbeat tracking via `Session::last_heartbeat`.
    Heartbeat { timestamp: Instant },
    /// Work completed successfully
    Complete { output: Option<String> },
    /// Work failed
    Fail { reason: String },
    /// Nudge attempt made (for stuck tasks)
    Nudged,
    /// Task restarted after being stuck
    Restart { session_id: SessionId },
    /// Evaluate current state (called periodically)
    /// session_idle_time: Time since session's last heartbeat (from Session::idle_time())
    Tick { session_idle_time: Option<Duration> },
}

/// A task representing a unit of work
#[derive(Clone, Debug)]
pub struct Task {
    pub id: TaskId,
    pub pipeline_id: PipelineId,
    pub phase: String,
    pub state: TaskState,
    pub session_id: Option<SessionId>,
    pub heartbeat_interval: Duration,
    pub stuck_threshold: Duration,
    /// Last heartbeat timestamp on this task.
    ///
    /// **Note**: Session now owns heartbeat tracking for stuck detection.
    /// This field is kept for backward compatibility and is updated on Start/Heartbeat events.
    /// Stuck detection uses `session_idle_time` passed via `TaskEvent::Tick { session_idle_time }`.
    pub last_heartbeat: Option<Instant>,
    pub created_at: Instant,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
}

impl Task {
    /// Create a new task in the Pending state
    pub fn new(
        id: impl Into<TaskId>,
        pipeline_id: PipelineId,
        phase: impl Into<String>,
        heartbeat_interval: Duration,
        stuck_threshold: Duration,
        clock: &impl Clock,
    ) -> Self {
        Task {
            id: id.into(),
            pipeline_id,
            phase: phase.into(),
            state: TaskState::Pending,
            session_id: None,
            heartbeat_interval,
            stuck_threshold,
            last_heartbeat: None,
            created_at: clock.now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Pure transition function - returns new state and effects
    pub fn transition(&self, event: TaskEvent, clock: &impl Clock) -> (Task, Vec<Effect>) {
        let now = clock.now();

        match (&self.state, event) {
            // Pending → Running
            (TaskState::Pending, TaskEvent::Start { session_id }) => {
                let task = Task {
                    state: TaskState::Running,
                    session_id: Some(session_id.clone()),
                    last_heartbeat: Some(now),
                    started_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskStarted {
                    id: self.id.clone(),
                    session_id,
                })];
                (task, effects)
            }

            // Running: heartbeat refreshes timer
            (TaskState::Running, TaskEvent::Heartbeat { timestamp }) => {
                let task = Task {
                    last_heartbeat: Some(timestamp),
                    ..self.clone()
                };
                (task, vec![])
            }

            // Running: tick evaluates if stuck
            // Note: Stuck detection now uses session_idle_time from Session::idle_time()
            // rather than Task's own last_heartbeat. Session owns heartbeat tracking.
            (TaskState::Running, TaskEvent::Tick { session_idle_time }) => {
                if let Some(idle) = session_idle_time {
                    if idle > self.stuck_threshold {
                        let task = Task {
                            state: TaskState::Stuck {
                                since: now,
                                nudge_count: 0,
                            },
                            ..self.clone()
                        };
                        let effects = vec![Effect::Emit(Event::TaskStuck {
                            id: self.id.clone(),
                            since: now,
                        })];
                        return (task, effects);
                    }
                }
                (self.clone(), vec![])
            }

            // Running/Stuck → Done
            (TaskState::Running | TaskState::Stuck { .. }, TaskEvent::Complete { output }) => {
                let task = Task {
                    state: TaskState::Done {
                        output: output.clone(),
                    },
                    completed_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskComplete {
                    id: self.id.clone(),
                    output,
                })];
                (task, effects)
            }

            // Running/Stuck → Failed
            (TaskState::Running | TaskState::Stuck { .. }, TaskEvent::Fail { reason }) => {
                let task = Task {
                    state: TaskState::Failed {
                        reason: reason.clone(),
                    },
                    completed_at: Some(now),
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskFailed {
                    id: self.id.clone(),
                    reason,
                })];
                (task, effects)
            }

            // Stuck: nudge increments counter
            (TaskState::Stuck { since, nudge_count }, TaskEvent::Nudged) => {
                let task = Task {
                    state: TaskState::Stuck {
                        since: *since,
                        nudge_count: nudge_count + 1,
                    },
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskNudged {
                    id: self.id.clone(),
                    count: nudge_count + 1,
                })];
                (task, effects)
            }

            // Stuck: heartbeat can recover from stuck
            (TaskState::Stuck { .. }, TaskEvent::Heartbeat { timestamp }) => {
                let task = Task {
                    state: TaskState::Running,
                    last_heartbeat: Some(timestamp),
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskRecovered {
                    id: self.id.clone(),
                })];
                (task, effects)
            }

            // Stuck: restart with new session
            (TaskState::Stuck { .. }, TaskEvent::Restart { session_id }) => {
                let task = Task {
                    state: TaskState::Running,
                    session_id: Some(session_id.clone()),
                    last_heartbeat: Some(now),
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::TaskRestarted {
                    id: self.id.clone(),
                    session_id,
                })];
                (task, effects)
            }

            // Invalid transitions - no change
            _ => (self.clone(), vec![]),
        }
    }

    /// Check if task is stuck
    pub fn is_stuck(&self) -> bool {
        matches!(self.state, TaskState::Stuck { .. })
    }

    /// Check if task is terminal (done or failed)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            TaskState::Done { .. } | TaskState::Failed { .. }
        )
    }

    /// Check if task is pending
    pub fn is_pending(&self) -> bool {
        matches!(self.state, TaskState::Pending)
    }

    /// Check if task is running
    pub fn is_running(&self) -> bool {
        matches!(self.state, TaskState::Running)
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;

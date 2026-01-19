// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Phase guards for pipeline phase gating
//!
//! Provides pre/post guards that gate pipeline phase transitions.

use super::guard::GuardCondition;
use serde::{Deserialize, Serialize};

/// Type of guard (for events and logging)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardType {
    Pre,
    Post,
}

impl std::fmt::Display for GuardType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardType::Pre => write!(f, "pre"),
            GuardType::Post => write!(f, "post"),
        }
    }
}

/// Guards for a pipeline phase
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhaseGuards {
    /// Guards that must pass before entering this phase
    pub pre: Option<GuardCondition>,
    /// Guards that must pass before leaving this phase
    pub post: Option<GuardCondition>,
    /// Event patterns that should wake guard evaluation
    pub wake_on: Vec<String>,
}

impl PhaseGuards {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pre(mut self, condition: GuardCondition) -> Self {
        self.pre = Some(condition);
        self
    }

    pub fn with_post(mut self, condition: GuardCondition) -> Self {
        self.post = Some(condition);
        self
    }

    pub fn with_wake_on(mut self, patterns: Vec<String>) -> Self {
        self.wake_on = patterns;
        self
    }

    /// Check if this phase has any guards
    pub fn has_guards(&self) -> bool {
        self.pre.is_some() || self.post.is_some()
    }

    /// Get all event patterns that should trigger re-evaluation
    pub fn wake_patterns(&self) -> &[String] {
        &self.wake_on
    }
}

/// Information about a blocked guard
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockedGuard {
    /// Unique identifier for this block
    pub guard_id: String,
    /// The pipeline ID that is blocked
    pub pipeline_id: String,
    /// The phase that is blocked
    pub phase: String,
    /// The guard condition that is blocking
    pub condition: GuardCondition,
    /// Whether this is a pre or post guard
    pub guard_type: GuardType,
    /// Event patterns that should wake guard evaluation
    pub wake_on: Vec<String>,
    /// The reason the guard failed
    pub failure_reason: String,
}

impl BlockedGuard {
    pub fn new(
        guard_id: impl Into<String>,
        pipeline_id: impl Into<String>,
        phase: impl Into<String>,
        condition: GuardCondition,
        guard_type: GuardType,
        failure_reason: impl Into<String>,
    ) -> Self {
        // Auto-generate wake patterns based on condition
        let wake_on = default_wake_patterns(&condition);

        Self {
            guard_id: guard_id.into(),
            pipeline_id: pipeline_id.into(),
            phase: phase.into(),
            condition,
            guard_type,
            wake_on,
            failure_reason: failure_reason.into(),
        }
    }

    pub fn with_wake_on(mut self, patterns: Vec<String>) -> Self {
        self.wake_on = patterns;
        self
    }
}

/// Generate default wake patterns based on guard condition type
fn default_wake_patterns(condition: &GuardCondition) -> Vec<String> {
    match condition {
        GuardCondition::LockFree { .. } | GuardCondition::LockHeldBy { .. } => {
            vec!["lock:".to_string()]
        }
        GuardCondition::SemaphoreAvailable { .. } => {
            vec!["semaphore:".to_string()]
        }
        GuardCondition::BranchExists { .. }
        | GuardCondition::BranchNotExists { .. }
        | GuardCondition::BranchMerged { .. } => {
            // No auto-wake for branch conditions (need external trigger)
            vec![]
        }
        GuardCondition::IssuesComplete { .. } | GuardCondition::IssueInStatus { .. } => {
            // No auto-wake for issue conditions (need external trigger)
            vec![]
        }
        GuardCondition::FileExists { .. } | GuardCondition::FileNotExists { .. } => {
            // No auto-wake for file conditions
            vec![]
        }
        GuardCondition::SessionAlive { .. } => {
            vec!["session:".to_string()]
        }
        GuardCondition::CustomCheck { .. } => {
            // No auto-wake for custom checks
            vec![]
        }
        GuardCondition::All { conditions } | GuardCondition::Any { conditions } => {
            // Combine all child wake patterns
            conditions.iter().flat_map(default_wake_patterns).collect()
        }
        GuardCondition::Not { condition } => default_wake_patterns(condition),
    }
}

/// Configuration for phase guards across a pipeline
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PipelineGuards {
    /// Guards by phase name
    phases: std::collections::HashMap<String, PhaseGuards>,
}

impl PipelineGuards {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set guards for a specific phase
    pub fn set_phase(&mut self, phase: impl Into<String>, guards: PhaseGuards) {
        self.phases.insert(phase.into(), guards);
    }

    /// Get guards for a specific phase
    pub fn get_phase(&self, phase: &str) -> Option<&PhaseGuards> {
        self.phases.get(phase)
    }

    /// Builder method to add guards for a phase
    pub fn with_phase(mut self, phase: impl Into<String>, guards: PhaseGuards) -> Self {
        self.set_phase(phase, guards);
        self
    }
}

#[cfg(test)]
#[path = "phase_guard_tests.rs"]
mod tests;

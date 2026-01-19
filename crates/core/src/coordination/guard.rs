// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Guard conditions for phase gating
//!
//! Provides composable conditions that gate pipeline phase transitions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Result of guard evaluation
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardResult {
    /// Guard condition is satisfied
    Passed,
    /// Guard condition is not satisfied
    Failed { reason: String },
    /// Guard evaluation needs external data
    NeedsInput { input_type: GuardInputType },
}

/// Types of input a guard might need
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardInputType {
    /// Lock state for a specific lock
    LockState { lock_name: String },
    /// Semaphore state for a specific semaphore
    SemaphoreState { semaphore_name: String },
    /// Whether a branch exists
    BranchExists { branch: String },
    /// Whether a branch is merged
    BranchMerged { branch: String, into: String },
    /// Issue status
    IssueStatus { issue_id: String },
    /// All issues for a filter
    IssuesForFilter { filter: String },
    /// File exists check
    FileExists { path: String },
    /// Session is alive
    SessionAlive { session_name: String },
    /// Custom check (shell command)
    CustomCheck { command: String },
}

/// Issue status for guard evaluation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
    Unknown,
}

/// Input data gathered by adapters for guard evaluation
#[derive(Clone, Debug, Default)]
pub struct GuardInputs {
    /// Lock states by name (true = free)
    pub locks: HashMap<String, bool>,
    /// Lock holder by name
    pub lock_holders: BTreeMap<String, String>,
    /// Semaphore availability by name (available slots)
    pub semaphores: HashMap<String, u32>,
    /// Branch existence by name
    pub branches: HashMap<String, bool>,
    /// Branch merge status (branch -> into -> merged)
    pub branch_merged: HashMap<(String, String), bool>,
    /// Issue statuses by ID
    pub issues: HashMap<String, IssueStatus>,
    /// Issues matching filters
    pub issue_lists: HashMap<String, Vec<String>>,
    /// File existence by path
    pub files: HashMap<String, bool>,
    /// Session alive status by name
    pub sessions: HashMap<String, bool>,
    /// Custom check results by command
    pub custom_checks: HashMap<String, bool>,
}

/// A guard condition
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GuardCondition {
    /// Lock must be free
    LockFree { lock_name: String },

    /// Lock must be held by specific holder
    LockHeldBy {
        lock_name: String,
        holder_id: String,
    },

    /// Semaphore must have available slots
    SemaphoreAvailable { semaphore_name: String, weight: u32 },

    /// Git branch must exist
    BranchExists { branch: String },

    /// Git branch must not exist
    BranchNotExists { branch: String },

    /// Git branch must be merged into target
    BranchMerged { branch: String, into: String },

    /// All issues matching filter must be done
    IssuesComplete { filter: String },

    /// Specific issue must be in expected status
    IssueInStatus {
        issue_id: String,
        expected: IssueStatus,
    },

    /// File must exist
    FileExists { path: String },

    /// File must not exist
    FileNotExists { path: String },

    /// Session must be alive
    SessionAlive { session_name: String },

    /// Custom shell command must return success
    CustomCheck {
        command: String,
        description: String,
    },

    /// Composite: all conditions must pass
    All { conditions: Vec<GuardCondition> },

    /// Composite: any condition must pass
    Any { conditions: Vec<GuardCondition> },

    /// Composite: condition must fail
    Not { condition: Box<GuardCondition> },
}

impl GuardCondition {
    /// Get all input types needed to evaluate this guard
    pub fn required_inputs(&self) -> Vec<GuardInputType> {
        match self {
            GuardCondition::LockFree { lock_name }
            | GuardCondition::LockHeldBy { lock_name, .. } => {
                vec![GuardInputType::LockState {
                    lock_name: lock_name.clone(),
                }]
            }
            GuardCondition::SemaphoreAvailable { semaphore_name, .. } => {
                vec![GuardInputType::SemaphoreState {
                    semaphore_name: semaphore_name.clone(),
                }]
            }
            GuardCondition::BranchExists { branch }
            | GuardCondition::BranchNotExists { branch } => {
                vec![GuardInputType::BranchExists {
                    branch: branch.clone(),
                }]
            }
            GuardCondition::BranchMerged { branch, into } => {
                vec![GuardInputType::BranchMerged {
                    branch: branch.clone(),
                    into: into.clone(),
                }]
            }
            GuardCondition::IssuesComplete { filter } => {
                vec![GuardInputType::IssuesForFilter {
                    filter: filter.clone(),
                }]
            }
            GuardCondition::IssueInStatus { issue_id, .. } => {
                vec![GuardInputType::IssueStatus {
                    issue_id: issue_id.clone(),
                }]
            }
            GuardCondition::FileExists { path } | GuardCondition::FileNotExists { path } => {
                vec![GuardInputType::FileExists { path: path.clone() }]
            }
            GuardCondition::SessionAlive { session_name } => {
                vec![GuardInputType::SessionAlive {
                    session_name: session_name.clone(),
                }]
            }
            GuardCondition::CustomCheck { command, .. } => {
                vec![GuardInputType::CustomCheck {
                    command: command.clone(),
                }]
            }
            GuardCondition::All { conditions } | GuardCondition::Any { conditions } => conditions
                .iter()
                .flat_map(|c| c.required_inputs())
                .collect(),
            GuardCondition::Not { condition } => condition.required_inputs(),
        }
    }

    /// Evaluate the guard condition given the inputs (pure function)
    pub fn evaluate(&self, inputs: &GuardInputs) -> GuardResult {
        match self {
            GuardCondition::LockFree { lock_name } => match inputs.locks.get(lock_name) {
                Some(true) => GuardResult::Passed,
                Some(false) => GuardResult::Failed {
                    reason: format!("Lock '{}' is held", lock_name),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::LockState {
                        lock_name: lock_name.clone(),
                    },
                },
            },

            GuardCondition::LockHeldBy {
                lock_name,
                holder_id,
            } => match inputs.lock_holders.get(lock_name) {
                Some(actual_holder) if actual_holder == holder_id => GuardResult::Passed,
                Some(actual_holder) => GuardResult::Failed {
                    reason: format!(
                        "Lock '{}' is held by '{}', not '{}'",
                        lock_name, actual_holder, holder_id
                    ),
                },
                None => {
                    // Check if lock is free
                    match inputs.locks.get(lock_name) {
                        Some(true) => GuardResult::Failed {
                            reason: format!("Lock '{}' is not held by '{}'", lock_name, holder_id),
                        },
                        _ => GuardResult::NeedsInput {
                            input_type: GuardInputType::LockState {
                                lock_name: lock_name.clone(),
                            },
                        },
                    }
                }
            },

            GuardCondition::SemaphoreAvailable {
                semaphore_name,
                weight,
            } => match inputs.semaphores.get(semaphore_name) {
                Some(available) if *available >= *weight => GuardResult::Passed,
                Some(available) => GuardResult::Failed {
                    reason: format!(
                        "Semaphore '{}' has {} slots, need {}",
                        semaphore_name, available, weight
                    ),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::SemaphoreState {
                        semaphore_name: semaphore_name.clone(),
                    },
                },
            },

            GuardCondition::BranchExists { branch } => match inputs.branches.get(branch) {
                Some(true) => GuardResult::Passed,
                Some(false) => GuardResult::Failed {
                    reason: format!("Branch '{}' does not exist", branch),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::BranchExists {
                        branch: branch.clone(),
                    },
                },
            },

            GuardCondition::BranchNotExists { branch } => match inputs.branches.get(branch) {
                Some(false) => GuardResult::Passed,
                Some(true) => GuardResult::Failed {
                    reason: format!("Branch '{}' exists", branch),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::BranchExists {
                        branch: branch.clone(),
                    },
                },
            },

            GuardCondition::BranchMerged { branch, into } => {
                match inputs.branch_merged.get(&(branch.clone(), into.clone())) {
                    Some(true) => GuardResult::Passed,
                    Some(false) => GuardResult::Failed {
                        reason: format!("Branch '{}' is not merged into '{}'", branch, into),
                    },
                    None => GuardResult::NeedsInput {
                        input_type: GuardInputType::BranchMerged {
                            branch: branch.clone(),
                            into: into.clone(),
                        },
                    },
                }
            }

            GuardCondition::IssuesComplete { filter } => match inputs.issue_lists.get(filter) {
                Some(issues) => {
                    let incomplete: Vec<_> = issues
                        .iter()
                        .filter(|id| inputs.issues.get(*id) != Some(&IssueStatus::Done))
                        .cloned()
                        .collect();
                    if incomplete.is_empty() {
                        GuardResult::Passed
                    } else {
                        GuardResult::Failed {
                            reason: format!(
                                "Issues not complete: {}",
                                incomplete
                                    .iter()
                                    .take(3)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        }
                    }
                }
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::IssuesForFilter {
                        filter: filter.clone(),
                    },
                },
            },

            GuardCondition::IssueInStatus { issue_id, expected } => {
                match inputs.issues.get(issue_id) {
                    Some(status) if status == expected => GuardResult::Passed,
                    Some(status) => GuardResult::Failed {
                        reason: format!(
                            "Issue '{}' is {:?}, expected {:?}",
                            issue_id, status, expected
                        ),
                    },
                    None => GuardResult::NeedsInput {
                        input_type: GuardInputType::IssueStatus {
                            issue_id: issue_id.clone(),
                        },
                    },
                }
            }

            GuardCondition::FileExists { path } => match inputs.files.get(path) {
                Some(true) => GuardResult::Passed,
                Some(false) => GuardResult::Failed {
                    reason: format!("File '{}' does not exist", path),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::FileExists { path: path.clone() },
                },
            },

            GuardCondition::FileNotExists { path } => match inputs.files.get(path) {
                Some(false) => GuardResult::Passed,
                Some(true) => GuardResult::Failed {
                    reason: format!("File '{}' exists", path),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::FileExists { path: path.clone() },
                },
            },

            GuardCondition::SessionAlive { session_name } => {
                match inputs.sessions.get(session_name) {
                    Some(true) => GuardResult::Passed,
                    Some(false) => GuardResult::Failed {
                        reason: format!("Session '{}' is not alive", session_name),
                    },
                    None => GuardResult::NeedsInput {
                        input_type: GuardInputType::SessionAlive {
                            session_name: session_name.clone(),
                        },
                    },
                }
            }

            GuardCondition::CustomCheck {
                command,
                description,
            } => match inputs.custom_checks.get(command) {
                Some(true) => GuardResult::Passed,
                Some(false) => GuardResult::Failed {
                    reason: format!("Custom check failed: {}", description),
                },
                None => GuardResult::NeedsInput {
                    input_type: GuardInputType::CustomCheck {
                        command: command.clone(),
                    },
                },
            },

            GuardCondition::All { conditions } => {
                for condition in conditions {
                    match condition.evaluate(inputs) {
                        GuardResult::Passed => continue,
                        other => return other,
                    }
                }
                GuardResult::Passed
            }

            GuardCondition::Any { conditions } => {
                let mut last_failure = None;
                for condition in conditions {
                    match condition.evaluate(inputs) {
                        GuardResult::Passed => return GuardResult::Passed,
                        GuardResult::NeedsInput { input_type } => {
                            return GuardResult::NeedsInput { input_type };
                        }
                        GuardResult::Failed { reason } => {
                            last_failure = Some(reason);
                        }
                    }
                }
                GuardResult::Failed {
                    reason: last_failure.unwrap_or_else(|| "No conditions".to_string()),
                }
            }

            GuardCondition::Not { condition } => match condition.evaluate(inputs) {
                GuardResult::Passed => GuardResult::Failed {
                    reason: "Condition should have failed".to_string(),
                },
                GuardResult::Failed { .. } => GuardResult::Passed,
                GuardResult::NeedsInput { input_type } => GuardResult::NeedsInput { input_type },
            },
        }
    }
}

// Convenience constructors
impl GuardCondition {
    pub fn lock_free(name: impl Into<String>) -> Self {
        GuardCondition::LockFree {
            lock_name: name.into(),
        }
    }

    pub fn lock_held_by(name: impl Into<String>, holder: impl Into<String>) -> Self {
        GuardCondition::LockHeldBy {
            lock_name: name.into(),
            holder_id: holder.into(),
        }
    }

    pub fn semaphore_available(name: impl Into<String>, weight: u32) -> Self {
        GuardCondition::SemaphoreAvailable {
            semaphore_name: name.into(),
            weight,
        }
    }

    pub fn branch_exists(branch: impl Into<String>) -> Self {
        GuardCondition::BranchExists {
            branch: branch.into(),
        }
    }

    pub fn branch_not_exists(branch: impl Into<String>) -> Self {
        GuardCondition::BranchNotExists {
            branch: branch.into(),
        }
    }

    pub fn branch_merged(branch: impl Into<String>, into: impl Into<String>) -> Self {
        GuardCondition::BranchMerged {
            branch: branch.into(),
            into: into.into(),
        }
    }

    pub fn issues_complete(filter: impl Into<String>) -> Self {
        GuardCondition::IssuesComplete {
            filter: filter.into(),
        }
    }

    pub fn issue_in_status(issue_id: impl Into<String>, expected: IssueStatus) -> Self {
        GuardCondition::IssueInStatus {
            issue_id: issue_id.into(),
            expected,
        }
    }

    pub fn file_exists(path: impl Into<String>) -> Self {
        GuardCondition::FileExists { path: path.into() }
    }

    pub fn file_not_exists(path: impl Into<String>) -> Self {
        GuardCondition::FileNotExists { path: path.into() }
    }

    pub fn session_alive(name: impl Into<String>) -> Self {
        GuardCondition::SessionAlive {
            session_name: name.into(),
        }
    }

    pub fn custom_check(command: impl Into<String>, description: impl Into<String>) -> Self {
        GuardCondition::CustomCheck {
            command: command.into(),
            description: description.into(),
        }
    }

    pub fn all(conditions: Vec<GuardCondition>) -> Self {
        GuardCondition::All { conditions }
    }

    pub fn any(conditions: Vec<GuardCondition>) -> Self {
        GuardCondition::Any { conditions }
    }

    pub fn negate(condition: GuardCondition) -> Self {
        GuardCondition::Not {
            condition: Box::new(condition),
        }
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;

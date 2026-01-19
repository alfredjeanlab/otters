// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Production resource scanner for discovering resources.

use super::{ResourceInfo, ResourceScanner, ScanError, ScannerSource};
use crate::clock::Clock;
use crate::storage::wal::MaterializedState;
use chrono::Utc;
use std::time::Duration;

/// Production resource scanner that discovers real resources
pub struct DefaultResourceScanner<'a, C: Clock> {
    state: &'a MaterializedState,
    clock: &'a C,
}

impl<'a, C: Clock> DefaultResourceScanner<'a, C> {
    pub fn new(state: &'a MaterializedState, clock: &'a C) -> Self {
        Self { state, clock }
    }

    fn parse_command_output(
        &self,
        output: &std::process::Output,
    ) -> Result<Vec<ResourceInfo>, ScanError> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScanError::CommandFailed {
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Try parsing as JSON array
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout) {
            let resources = items
                .into_iter()
                .filter_map(|item| self.json_to_resource_info(&item))
                .collect();
            return Ok(resources);
        }

        // Try parsing as newline-delimited resource IDs
        let resources = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| ResourceInfo::new(line.trim()))
            .collect();

        Ok(resources)
    }

    fn json_to_resource_info(&self, json: &serde_json::Value) -> Option<ResourceInfo> {
        let obj = json.as_object()?;

        let id = obj
            .get("id")
            .or_else(|| obj.get("name"))
            .or_else(|| obj.get("resource_id"))
            .and_then(|v| v.as_str())?;

        let mut info = ResourceInfo::new(id);

        if let Some(age_secs) = obj.get("age_seconds").and_then(|v| v.as_u64()) {
            info = info.with_age(Duration::from_secs(age_secs));
        }

        if let Some(attempts) = obj.get("attempts").and_then(|v| v.as_u64()) {
            info = info.with_attempts(attempts as u32);
        }

        if let Some(holder) = obj.get("holder").and_then(|v| v.as_str()) {
            info = info.with_holder(holder);
        }

        // Copy any other fields as metadata
        for (key, value) in obj {
            if ![
                "id",
                "name",
                "resource_id",
                "age_seconds",
                "attempts",
                "holder",
            ]
            .contains(&key.as_str())
            {
                if let Some(s) = value.as_str() {
                    info = info.with_metadata(key, s);
                }
            }
        }

        Some(info)
    }
}

impl<C: Clock> ResourceScanner for DefaultResourceScanner<'_, C> {
    fn scan(&self, source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        match source {
            ScannerSource::Locks => {
                let coordination = self.state.coordination();
                let now = self.clock.now();

                let resources = coordination
                    .lock_names()
                    .into_iter()
                    .filter_map(|name| {
                        let lock = coordination.get_lock(&name)?;
                        match &lock.state {
                            crate::coordination::LockState::Held {
                                holder,
                                last_heartbeat,
                                ..
                            } => {
                                let age = now.duration_since(*last_heartbeat);
                                Some(
                                    ResourceInfo::new(format!("lock:{}", name))
                                        .with_age(age)
                                        .with_holder(&holder.0),
                                )
                            }
                            crate::coordination::LockState::Free => None,
                        }
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Semaphores => {
                let coordination = self.state.coordination();
                let now = self.clock.now();

                let resources = coordination
                    .semaphore_names()
                    .into_iter()
                    .flat_map(|name| {
                        let sem = match coordination.get_semaphore(&name) {
                            Some(s) => s,
                            None => return vec![],
                        };

                        sem.holders
                            .iter()
                            .map(|(holder_id, holder)| {
                                let age = now.duration_since(holder.last_heartbeat);
                                ResourceInfo::new(format!("semaphore:{}:{}", name, holder_id))
                                    .with_age(age)
                                    .with_holder(holder_id)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Queue { name } => {
                let queue = match self.state.queue(name) {
                    Some(q) => q,
                    None => {
                        return Err(ScanError::ListFailed {
                            message: format!("queue not found: {}", name),
                        })
                    }
                };

                let now = Utc::now();
                let resources = queue
                    .items
                    .iter()
                    .map(|item| {
                        let age = (now - item.created_at).to_std().unwrap_or(Duration::ZERO);
                        ResourceInfo::new(format!("queue:{}:{}", name, item.id))
                            .with_age(age)
                            .with_attempts(item.attempts)
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Worktrees => {
                // List worktrees using git command directly (blocking)
                let output = std::process::Command::new("git")
                    .args(["worktree", "list", "--porcelain"])
                    .output()
                    .map_err(|e| ScanError::CommandFailed {
                        message: e.to_string(),
                    })?;

                if !output.status.success() {
                    return Err(ScanError::CommandFailed {
                        message: String::from_utf8_lossy(&output.stderr).to_string(),
                    });
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut resources = Vec::new();
                let mut current_path: Option<String> = None;
                let mut current_branch: Option<String> = None;

                for line in stdout.lines() {
                    if let Some(path) = line.strip_prefix("worktree ") {
                        if let Some(prev_path) = current_path.take() {
                            let mut info = ResourceInfo::new(format!("worktree:{}", prev_path));
                            if let Some(ref branch) = current_branch {
                                info = info.with_metadata("branch", branch);
                            }
                            resources.push(info);
                        }
                        current_path = Some(path.to_string());
                        current_branch = None;
                    } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                        current_branch = Some(branch.to_string());
                    }
                }

                // Don't forget the last worktree
                if let Some(prev_path) = current_path {
                    let mut info = ResourceInfo::new(format!("worktree:{}", prev_path));
                    if let Some(ref branch) = current_branch {
                        info = info.with_metadata("branch", branch);
                    }
                    resources.push(info);
                }

                Ok(resources)
            }

            ScannerSource::Pipelines => {
                let now = Utc::now();
                let resources = self
                    .state
                    .all_pipelines()
                    .map(|p| {
                        let age = (now - p.created_at).to_std().unwrap_or(Duration::ZERO);
                        ResourceInfo::new(format!("pipeline:{}", p.id.0))
                            .with_age(age)
                            .with_state(p.phase.name())
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Sessions => {
                let now = self.clock.now();
                let resources = self
                    .state
                    .sessions
                    .values()
                    .map(|s| {
                        let age = s.idle_time(now).unwrap_or(Duration::ZERO);
                        ResourceInfo::new(format!("session:{}", s.id.0))
                            .with_age(age)
                            .with_state(format!("{:?}", s.state))
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Tasks => {
                let now = self.clock.now();
                let resources = self
                    .state
                    .all_tasks()
                    .filter(|t| {
                        // Only include terminal tasks for cleanup consideration
                        matches!(
                            t.state,
                            crate::task::TaskState::Done { .. }
                                | crate::task::TaskState::Failed { .. }
                        )
                    })
                    .map(|t| {
                        // Use completed_at if available, otherwise created_at as fallback
                        let completed = t.completed_at.unwrap_or(t.created_at);
                        let age = now.saturating_duration_since(completed);
                        ResourceInfo::new(format!("task:{}", t.id.0))
                            .with_age(age)
                            .with_state(format!("{:?}", t.state))
                    })
                    .collect();

                Ok(resources)
            }

            ScannerSource::Command { command } => {
                let output = std::process::Command::new("sh")
                    .args(["-c", command])
                    .output()
                    .map_err(|e| ScanError::CommandFailed {
                        message: e.to_string(),
                    })?;

                self.parse_command_output(&output)
            }
        }
    }
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CleanupExecutor - executes scanner cleanup effects
//!
//! The CleanupExecutor handles the actual execution of cleanup operations
//! when scanners find resources that need to be cleaned up.

use crate::effect::{Effect, Event};

/// Error when executing a cleanup operation
#[derive(Debug, Clone)]
pub enum CleanupError {
    /// Unknown resource type
    UnknownResourceType { resource_id: String },
    /// Resource not found
    ResourceNotFound { resource_id: String },
    /// Operation failed
    OperationFailed { message: String },
    /// Archive destination not found
    ArchiveDestinationNotFound { destination: String },
}

impl std::fmt::Display for CleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CleanupError::UnknownResourceType { resource_id } => {
                write!(f, "unknown resource type: {}", resource_id)
            }
            CleanupError::ResourceNotFound { resource_id } => {
                write!(f, "resource not found: {}", resource_id)
            }
            CleanupError::OperationFailed { message } => {
                write!(f, "operation failed: {}", message)
            }
            CleanupError::ArchiveDestinationNotFound { destination } => {
                write!(f, "archive destination not found: {}", destination)
            }
        }
    }
}

impl std::error::Error for CleanupError {}

/// Result of a cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub resource_id: String,
    pub action: String,
    pub success: bool,
    pub message: Option<String>,
}

/// Trait for executing cleanup operations on coordination resources
pub trait CoordinationCleanup: Send + Sync {
    /// Force release a lock
    fn force_release_lock(&self, lock_name: &str) -> Result<(), String>;

    /// Release a semaphore slot
    fn release_semaphore_slot(&self, semaphore_name: &str, holder_id: &str) -> Result<(), String>;
}

/// Trait for executing cleanup operations on sessions
pub trait SessionCleanup: Send + Sync {
    /// Kill a session
    fn kill_session(&self, session_name: &str) -> Result<(), String>;
}

/// Trait for executing cleanup operations on worktrees
pub trait WorktreeCleanup: Send + Sync {
    /// Remove a worktree
    fn remove_worktree(&self, path: &str) -> Result<(), String>;
}

/// Trait for executing cleanup operations on storage items
pub trait StorageCleanup: Send + Sync {
    /// Delete a queue item
    fn delete_queue_item(&self, queue_name: &str, item_id: &str) -> Result<(), String>;

    /// Move an item to dead letter queue
    fn dead_letter_item(&self, queue_name: &str, item_id: &str) -> Result<(), String>;

    /// Fail a queue item
    fn fail_item(&self, queue_name: &str, item_id: &str, reason: &str) -> Result<(), String>;

    /// Archive a resource
    fn archive_resource(&self, resource_id: &str, destination: &str) -> Result<(), String>;
}

/// No-op coordination cleanup for testing
pub struct NoOpCoordinationCleanup;

impl CoordinationCleanup for NoOpCoordinationCleanup {
    fn force_release_lock(&self, _lock_name: &str) -> Result<(), String> {
        Ok(())
    }

    fn release_semaphore_slot(
        &self,
        _semaphore_name: &str,
        _holder_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// No-op session cleanup for testing
pub struct NoOpSessionCleanup;

impl SessionCleanup for NoOpSessionCleanup {
    fn kill_session(&self, _session_name: &str) -> Result<(), String> {
        Ok(())
    }
}

/// No-op worktree cleanup for testing
pub struct NoOpWorktreeCleanup;

impl WorktreeCleanup for NoOpWorktreeCleanup {
    fn remove_worktree(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}

/// No-op storage cleanup for testing
pub struct NoOpStorageCleanup;

impl StorageCleanup for NoOpStorageCleanup {
    fn delete_queue_item(&self, _queue_name: &str, _item_id: &str) -> Result<(), String> {
        Ok(())
    }

    fn dead_letter_item(&self, _queue_name: &str, _item_id: &str) -> Result<(), String> {
        Ok(())
    }

    fn fail_item(&self, _queue_name: &str, _item_id: &str, _reason: &str) -> Result<(), String> {
        Ok(())
    }

    fn archive_resource(&self, _resource_id: &str, _destination: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Executes cleanup effects from scanners
pub struct CleanupExecutor<'a> {
    coordination: &'a dyn CoordinationCleanup,
    session: &'a dyn SessionCleanup,
    worktree: &'a dyn WorktreeCleanup,
    storage: &'a dyn StorageCleanup,
}

impl<'a> CleanupExecutor<'a> {
    /// Create a new CleanupExecutor
    pub fn new(
        coordination: &'a dyn CoordinationCleanup,
        session: &'a dyn SessionCleanup,
        worktree: &'a dyn WorktreeCleanup,
        storage: &'a dyn StorageCleanup,
    ) -> Self {
        Self {
            coordination,
            session,
            worktree,
            storage,
        }
    }

    /// Execute a cleanup effect
    pub fn execute(&self, effect: &Effect) -> Result<CleanupResult, CleanupError> {
        match effect {
            Effect::Emit(Event::ScannerDeleteResource {
                scanner_id: _,
                resource_id,
            }) => self.delete_resource(resource_id),

            Effect::Emit(Event::ScannerReleaseResource {
                scanner_id: _,
                resource_id,
            }) => self.release_resource(resource_id),

            Effect::Emit(Event::ScannerArchiveResource {
                scanner_id: _,
                resource_id,
                destination,
            }) => self.archive_resource(resource_id, destination),

            Effect::Emit(Event::ScannerFailResource {
                scanner_id: _,
                resource_id,
                reason,
            }) => self.fail_resource(resource_id, reason),

            Effect::Emit(Event::ScannerDeadLetterResource {
                scanner_id: _,
                resource_id,
            }) => self.dead_letter_resource(resource_id),

            _ => Ok(CleanupResult {
                resource_id: String::new(),
                action: "ignored".to_string(),
                success: true,
                message: Some("Not a cleanup effect".to_string()),
            }),
        }
    }

    /// Delete a resource based on its ID prefix
    fn delete_resource(&self, resource_id: &str) -> Result<CleanupResult, CleanupError> {
        let (resource_type, id) = self.parse_resource_id(resource_id)?;

        let result = match resource_type.as_str() {
            "lock" => self.coordination.force_release_lock(&id),
            "worktree" => self.worktree.remove_worktree(&id),
            "session" => self.session.kill_session(&id),
            "queue" => {
                // Queue items are in format queue:queue_name:item_id
                let parts: Vec<&str> = id.splitn(2, ':').collect();
                if parts.len() == 2 {
                    self.storage.delete_queue_item(parts[0], parts[1])
                } else {
                    Err(format!("invalid queue resource format: {}", id))
                }
            }
            _ => {
                return Err(CleanupError::UnknownResourceType {
                    resource_id: resource_id.to_string(),
                })
            }
        };

        match result {
            Ok(()) => Ok(CleanupResult {
                resource_id: resource_id.to_string(),
                action: "delete".to_string(),
                success: true,
                message: None,
            }),
            Err(e) => Err(CleanupError::OperationFailed { message: e }),
        }
    }

    /// Release a resource (for locks/semaphores)
    fn release_resource(&self, resource_id: &str) -> Result<CleanupResult, CleanupError> {
        let (resource_type, id) = self.parse_resource_id(resource_id)?;

        let result = match resource_type.as_str() {
            "lock" => self.coordination.force_release_lock(&id),
            "semaphore" => {
                // Semaphore slots are in format semaphore:name:holder_id
                let parts: Vec<&str> = id.splitn(2, ':').collect();
                if parts.len() == 2 {
                    self.coordination.release_semaphore_slot(parts[0], parts[1])
                } else {
                    Err(format!("invalid semaphore resource format: {}", id))
                }
            }
            _ => {
                return Err(CleanupError::UnknownResourceType {
                    resource_id: resource_id.to_string(),
                })
            }
        };

        match result {
            Ok(()) => Ok(CleanupResult {
                resource_id: resource_id.to_string(),
                action: "release".to_string(),
                success: true,
                message: None,
            }),
            Err(e) => Err(CleanupError::OperationFailed { message: e }),
        }
    }

    /// Archive a resource
    fn archive_resource(
        &self,
        resource_id: &str,
        destination: &str,
    ) -> Result<CleanupResult, CleanupError> {
        match self.storage.archive_resource(resource_id, destination) {
            Ok(()) => Ok(CleanupResult {
                resource_id: resource_id.to_string(),
                action: "archive".to_string(),
                success: true,
                message: Some(format!("archived to {}", destination)),
            }),
            Err(e) => Err(CleanupError::OperationFailed { message: e }),
        }
    }

    /// Fail a resource
    fn fail_resource(
        &self,
        resource_id: &str,
        reason: &str,
    ) -> Result<CleanupResult, CleanupError> {
        let (resource_type, id) = self.parse_resource_id(resource_id)?;

        let result = match resource_type.as_str() {
            "queue" => {
                let parts: Vec<&str> = id.splitn(2, ':').collect();
                if parts.len() == 2 {
                    self.storage.fail_item(parts[0], parts[1], reason)
                } else {
                    Err(format!("invalid queue resource format: {}", id))
                }
            }
            _ => {
                return Err(CleanupError::UnknownResourceType {
                    resource_id: resource_id.to_string(),
                })
            }
        };

        match result {
            Ok(()) => Ok(CleanupResult {
                resource_id: resource_id.to_string(),
                action: "fail".to_string(),
                success: true,
                message: Some(reason.to_string()),
            }),
            Err(e) => Err(CleanupError::OperationFailed { message: e }),
        }
    }

    /// Move a resource to dead letter
    fn dead_letter_resource(&self, resource_id: &str) -> Result<CleanupResult, CleanupError> {
        let (resource_type, id) = self.parse_resource_id(resource_id)?;

        let result = match resource_type.as_str() {
            "queue" => {
                let parts: Vec<&str> = id.splitn(2, ':').collect();
                if parts.len() == 2 {
                    self.storage.dead_letter_item(parts[0], parts[1])
                } else {
                    Err(format!("invalid queue resource format: {}", id))
                }
            }
            _ => {
                return Err(CleanupError::UnknownResourceType {
                    resource_id: resource_id.to_string(),
                })
            }
        };

        match result {
            Ok(()) => Ok(CleanupResult {
                resource_id: resource_id.to_string(),
                action: "dead_letter".to_string(),
                success: true,
                message: None,
            }),
            Err(e) => Err(CleanupError::OperationFailed { message: e }),
        }
    }

    /// Parse a resource ID into (type, id) tuple
    fn parse_resource_id(&self, resource_id: &str) -> Result<(String, String), CleanupError> {
        if let Some((resource_type, id)) = resource_id.split_once(':') {
            Ok((resource_type.to_string(), id.to_string()))
        } else {
            Err(CleanupError::UnknownResourceType {
                resource_id: resource_id.to_string(),
            })
        }
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;

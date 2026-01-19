// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::sync::RwLock;

/// Fake coordination cleanup that records operations
struct FakeCoordinationCleanup {
    released_locks: RwLock<Vec<String>>,
    released_semaphores: RwLock<Vec<(String, String)>>,
}

impl FakeCoordinationCleanup {
    fn new() -> Self {
        Self {
            released_locks: RwLock::new(vec![]),
            released_semaphores: RwLock::new(vec![]),
        }
    }

    fn released_locks(&self) -> Vec<String> {
        self.released_locks.read().unwrap().clone()
    }

    fn released_semaphores(&self) -> Vec<(String, String)> {
        self.released_semaphores.read().unwrap().clone()
    }
}

impl CoordinationCleanup for FakeCoordinationCleanup {
    fn force_release_lock(&self, lock_name: &str) -> Result<(), String> {
        self.released_locks
            .write()
            .unwrap()
            .push(lock_name.to_string());
        Ok(())
    }

    fn release_semaphore_slot(&self, semaphore_name: &str, holder_id: &str) -> Result<(), String> {
        self.released_semaphores
            .write()
            .unwrap()
            .push((semaphore_name.to_string(), holder_id.to_string()));
        Ok(())
    }
}

/// Fake session cleanup that records operations
struct FakeSessionCleanup {
    killed_sessions: RwLock<Vec<String>>,
}

impl FakeSessionCleanup {
    fn new() -> Self {
        Self {
            killed_sessions: RwLock::new(vec![]),
        }
    }

    fn killed_sessions(&self) -> Vec<String> {
        self.killed_sessions.read().unwrap().clone()
    }
}

impl SessionCleanup for FakeSessionCleanup {
    fn kill_session(&self, session_name: &str) -> Result<(), String> {
        self.killed_sessions
            .write()
            .unwrap()
            .push(session_name.to_string());
        Ok(())
    }
}

/// Fake worktree cleanup that records operations
struct FakeWorktreeCleanup {
    removed_worktrees: RwLock<Vec<String>>,
}

impl FakeWorktreeCleanup {
    fn new() -> Self {
        Self {
            removed_worktrees: RwLock::new(vec![]),
        }
    }

    fn removed_worktrees(&self) -> Vec<String> {
        self.removed_worktrees.read().unwrap().clone()
    }
}

impl WorktreeCleanup for FakeWorktreeCleanup {
    fn remove_worktree(&self, path: &str) -> Result<(), String> {
        self.removed_worktrees
            .write()
            .unwrap()
            .push(path.to_string());
        Ok(())
    }
}

/// Fake storage cleanup that records operations
struct FakeStorageCleanup {
    deleted_items: RwLock<Vec<(String, String)>>,
    dead_lettered: RwLock<Vec<(String, String)>>,
    failed_items: RwLock<Vec<(String, String, String)>>,
    archived: RwLock<Vec<(String, String)>>,
}

impl FakeStorageCleanup {
    fn new() -> Self {
        Self {
            deleted_items: RwLock::new(vec![]),
            dead_lettered: RwLock::new(vec![]),
            failed_items: RwLock::new(vec![]),
            archived: RwLock::new(vec![]),
        }
    }

    fn deleted_items(&self) -> Vec<(String, String)> {
        self.deleted_items.read().unwrap().clone()
    }

    fn dead_lettered(&self) -> Vec<(String, String)> {
        self.dead_lettered.read().unwrap().clone()
    }

    fn failed_items(&self) -> Vec<(String, String, String)> {
        self.failed_items.read().unwrap().clone()
    }

    fn archived(&self) -> Vec<(String, String)> {
        self.archived.read().unwrap().clone()
    }
}

impl StorageCleanup for FakeStorageCleanup {
    fn delete_queue_item(&self, queue_name: &str, item_id: &str) -> Result<(), String> {
        self.deleted_items
            .write()
            .unwrap()
            .push((queue_name.to_string(), item_id.to_string()));
        Ok(())
    }

    fn dead_letter_item(&self, queue_name: &str, item_id: &str) -> Result<(), String> {
        self.dead_lettered
            .write()
            .unwrap()
            .push((queue_name.to_string(), item_id.to_string()));
        Ok(())
    }

    fn fail_item(&self, queue_name: &str, item_id: &str, reason: &str) -> Result<(), String> {
        self.failed_items.write().unwrap().push((
            queue_name.to_string(),
            item_id.to_string(),
            reason.to_string(),
        ));
        Ok(())
    }

    fn archive_resource(&self, resource_id: &str, destination: &str) -> Result<(), String> {
        self.archived
            .write()
            .unwrap()
            .push((resource_id.to_string(), destination.to_string()));
        Ok(())
    }
}

fn create_executor<'a>(
    coordination: &'a dyn CoordinationCleanup,
    session: &'a dyn SessionCleanup,
    worktree: &'a dyn WorktreeCleanup,
    storage: &'a dyn StorageCleanup,
) -> CleanupExecutor<'a> {
    CleanupExecutor::new(coordination, session, worktree, storage)
}

#[test]
fn delete_lock_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeleteResource {
        scanner_id: "test".to_string(),
        resource_id: "lock:my-lock".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "delete");

    let locks = coordination.released_locks();
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0], "my-lock");
}

#[test]
fn delete_session_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeleteResource {
        scanner_id: "test".to_string(),
        resource_id: "session:agent-1".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);

    let sessions = session.killed_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0], "agent-1");
}

#[test]
fn delete_worktree_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeleteResource {
        scanner_id: "test".to_string(),
        resource_id: "worktree:/path/to/worktree".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);

    let trees = worktree.removed_worktrees();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0], "/path/to/worktree");
}

#[test]
fn release_lock_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerReleaseResource {
        scanner_id: "test".to_string(),
        resource_id: "lock:my-lock".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "release");

    let locks = coordination.released_locks();
    assert_eq!(locks.len(), 1);
}

#[test]
fn release_semaphore_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerReleaseResource {
        scanner_id: "test".to_string(),
        resource_id: "semaphore:my-sem:holder-1".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);

    let semaphores = coordination.released_semaphores();
    assert_eq!(semaphores.len(), 1);
    assert_eq!(semaphores[0].0, "my-sem");
    assert_eq!(semaphores[0].1, "holder-1");
}

#[test]
fn archive_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerArchiveResource {
        scanner_id: "test".to_string(),
        resource_id: "pipeline:old-pipeline".to_string(),
        destination: "/archive/2024".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "archive");

    let archived = storage.archived();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].0, "pipeline:old-pipeline");
    assert_eq!(archived[0].1, "/archive/2024");
}

#[test]
fn fail_queue_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerFailResource {
        scanner_id: "test".to_string(),
        resource_id: "queue:main:item-123".to_string(),
        reason: "exceeded max attempts".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "fail");

    let failed = storage.failed_items();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, "main");
    assert_eq!(failed[0].1, "item-123");
    assert_eq!(failed[0].2, "exceeded max attempts");
}

#[test]
fn dead_letter_queue_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeadLetterResource {
        scanner_id: "test".to_string(),
        resource_id: "queue:main:item-456".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "dead_letter");

    let dl = storage.dead_lettered();
    assert_eq!(dl.len(), 1);
    assert_eq!(dl[0].0, "main");
    assert_eq!(dl[0].1, "item-456");
}

#[test]
fn delete_queue_resource() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeleteResource {
        scanner_id: "test".to_string(),
        resource_id: "queue:main:item-789".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);

    let deleted = storage.deleted_items();
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].0, "main");
    assert_eq!(deleted[0].1, "item-789");
}

#[test]
fn unknown_resource_type_returns_error() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::ScannerDeleteResource {
        scanner_id: "test".to_string(),
        resource_id: "unknown:some-id".to_string(),
    });

    let result = executor.execute(&effect);
    assert!(matches!(
        result,
        Err(CleanupError::UnknownResourceType { .. })
    ));
}

#[test]
fn non_cleanup_effect_is_ignored() {
    let coordination = FakeCoordinationCleanup::new();
    let session = FakeSessionCleanup::new();
    let worktree = FakeWorktreeCleanup::new();
    let storage = FakeStorageCleanup::new();

    let executor = create_executor(&coordination, &session, &worktree, &storage);

    let effect = Effect::Emit(Event::CronTriggered {
        id: "test".to_string(),
    });

    let result = executor.execute(&effect).unwrap();
    assert!(result.success);
    assert_eq!(result.action, "ignored");
}

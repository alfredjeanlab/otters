# Epic 9c: Production Implementations

**Depends on**: None (can be done in parallel with 9a, 9d, 9e)
**Blocks**: Epic 9b (Engine Integration)
**Root Feature:** `otters-6bc5`

## Problem Statement

Only `NoOp*` implementations exist for `SourceFetcher` and `ResourceScanner`. Watchers can't fetch real data and scanners can't discover real resources.

## Goal

Create production adapters that query actual system state for watchers and scanners.

## Implementation

### 1. Create `DefaultSourceFetcher` in `crates/core/src/scheduling/source.rs`

```rust
// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Production source fetcher for watcher condition evaluation.

use crate::adapters::Adapters;
use crate::scheduling::{FetchContext, FetchError, SourceFetcher, SourceValue, WatcherSource};
use crate::storage::WalStore;
use crate::task::TaskId;

/// Production source fetcher that queries real system state
pub struct DefaultSourceFetcher<'a, A: Adapters> {
    adapters: &'a A,
    store: &'a WalStore,
}

impl<'a, A: Adapters> DefaultSourceFetcher<'a, A> {
    pub fn new(adapters: &'a A, store: &'a WalStore) -> Self {
        Self { adapters, store }
    }

    fn interpolate(&self, template: &str, context: &FetchContext) -> Result<String, FetchError> {
        // Replace {key} placeholders with context.variables values
    }

    fn parse_command_output(&self, output: &std::process::Output) -> Result<SourceValue, FetchError> {
        // Check exit status, try JSON -> number -> duration -> text fallback
    }

    fn json_to_source_value(&self, json: &serde_json::Value) -> Result<SourceValue, FetchError> {
        // Map JSON types to SourceValue variants (Number, Bool, String, Object with idle_seconds/count)
    }
}

impl<'a, A: Adapters> SourceFetcher for DefaultSourceFetcher<'a, A> {
    fn fetch(&self, source: &WatcherSource, context: &FetchContext) -> Result<SourceValue, FetchError> {
        // Match source type:
        // - Session: get idle_duration from store
        // - Task: get state/phase from store
        // - Queue: get depth from store
        // - Events: count recent matching events
        // - Command: execute sh -c, parse output
        // - File: read and parse as JSON or text
        // - Http: GET request, parse response
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
```

### 2. Create `DefaultResourceScanner` in `crates/core/src/scheduling/resource.rs`

```rust
// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Production resource scanner for discovering resources.

use crate::adapters::Adapters;
use crate::coordination::CoordinationManager;
use crate::scheduling::{ResourceInfo, ResourceScanner, ScanError, ScannerSource};
use crate::storage::WalStore;
use std::time::Duration;

/// Production resource scanner that discovers real resources
pub struct DefaultResourceScanner<'a, A: Adapters> {
    adapters: &'a A,
    store: &'a WalStore,
    coordination: &'a CoordinationManager,
}

impl<'a, A: Adapters> DefaultResourceScanner<'a, A> {
    pub fn new(adapters: &'a A, store: &'a WalStore, coordination: &'a CoordinationManager) -> Self {
        Self { adapters, store, coordination }
    }

    fn parse_command_output(&self, output: &std::process::Output) -> Result<Vec<ResourceInfo>, ScanError> {
        // Check exit status, try JSON array -> newline-delimited IDs
    }

    fn json_to_resource_info(&self, json: &serde_json::Value) -> Option<ResourceInfo> {
        // Extract id/name/resource_id, age_seconds, attempts, holder, and other metadata
    }
}

impl<'a, A: Adapters> ResourceScanner for DefaultResourceScanner<'a, A> {
    fn scan(&self, source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        // Match source type and return Vec<ResourceInfo>:
        // - Locks: coordination.list_locks() -> "lock:{name}"
        // - Semaphores: coordination.list_semaphore_slots() -> "semaphore:{name}:{holder}"
        // - Queue: store.list_queue_items() -> "queue:{name}:{id}"
        // - Worktrees: adapters.repo().list_worktrees() -> "worktree:{path}"
        // - Pipelines: store.list_pipelines() -> "pipeline:{id}"
        // - Sessions: store.list_sessions() -> "session:{name}"
        // - Tasks: store.list_tasks() (terminal only) -> "task:{id}"
        // - Command: execute sh -c, parse output
    }
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;
```

### 3. Update Exports in `crates/core/src/scheduling/mod.rs`

```rust
mod source;
mod resource;

pub use source::DefaultSourceFetcher;
pub use resource::DefaultResourceScanner;
```

### 4. Add Store Methods for Resource Queries

Add to `crates/core/src/storage/store.rs`:

```rust
impl WalStore {
    pub fn get_session(&self, name: &str) -> Option<&Session> { /* state.sessions.get */ }
    pub fn list_sessions(&self) -> Vec<&Session> { /* state.sessions.values */ }
    pub fn queue_depth(&self, queue_name: &str) -> usize { /* state.queues items.len */ }
    pub fn list_queue_items(&self, queue_name: &str) -> Result<Vec<&QueueItem>, WalStoreError> { /* ... */ }
    pub fn list_pipelines(&self) -> Vec<&Pipeline> { /* state.pipelines.values */ }
    pub fn list_tasks(&self) -> Vec<&Task> { /* state.tasks.values */ }

    pub fn recent_events_matching(&self, pattern: &str, limit: usize) -> Vec<&Event> {
        // Filter event_log by pattern (prefix match if ends with '*'), take limit
    }
}
```

## Files

- `crates/core/src/scheduling/source.rs` - NEW: DefaultSourceFetcher
- `crates/core/src/scheduling/source_tests.rs` - NEW: Tests
- `crates/core/src/scheduling/resource.rs` - NEW: DefaultResourceScanner
- `crates/core/src/scheduling/resource_tests.rs` - NEW: Tests
- `crates/core/src/scheduling/mod.rs` - Export new types
- `crates/core/src/storage/store.rs` - Add query methods

## Tests

```rust
// source_tests.rs
#[test]
fn fetch_session_idle_time() {
    let adapters = FakeAdapters::new();
    let mut store = WalStore::new_in_memory();
    store.create_session("agent-1").unwrap();
    store.update_session_activity("agent-1", Instant::now() - Duration::from_secs(300)).unwrap();

    let fetcher = DefaultSourceFetcher::new(&adapters, &store);
    let result = fetcher.fetch(
        &WatcherSource::Session { name: "agent-1".into() },
        &FetchContext::default(),
    ).unwrap();

    match result {
        SourceValue::Idle { duration } => {
            assert!(duration >= Duration::from_secs(295));
        }
        _ => panic!("expected Idle"),
    }
}

#[test]
fn fetch_queue_depth() {
    let adapters = FakeAdapters::new();
    let mut store = WalStore::new_in_memory();
    store.create_queue("work").unwrap();
    store.enqueue("work", "item-1", b"{}").unwrap();
    store.enqueue("work", "item-2", b"{}").unwrap();

    let fetcher = DefaultSourceFetcher::new(&adapters, &store);
    let result = fetcher.fetch(
        &WatcherSource::Queue { name: "work".into() },
        &FetchContext::default(),
    ).unwrap();

    assert_eq!(result, SourceValue::Numeric { value: 2 });
}

#[test]
fn fetch_command_output() {
    let adapters = FakeAdapters::new();
    let store = WalStore::new_in_memory();

    let fetcher = DefaultSourceFetcher::new(&adapters, &store);
    let result = fetcher.fetch(
        &WatcherSource::Command { command: "echo 42".into() },
        &FetchContext::default(),
    ).unwrap();

    assert_eq!(result, SourceValue::Numeric { value: 42 });
}

// resource_tests.rs
#[test]
fn scan_locks() {
    let adapters = FakeAdapters::new();
    let store = WalStore::new_in_memory();
    let mut coordination = CoordinationManager::new();
    coordination.acquire_lock("deploy", "agent-1").unwrap();

    let scanner = DefaultResourceScanner::new(&adapters, &store, &coordination);
    let resources = scanner.scan(&ScannerSource::Locks).unwrap();

    assert_eq!(resources.len(), 1);
    assert!(resources[0].id.contains("lock:deploy"));
    assert_eq!(resources[0].holder.as_deref(), Some("agent-1"));
}

#[test]
fn scan_queue_items() {
    let adapters = FakeAdapters::new();
    let mut store = WalStore::new_in_memory();
    store.create_queue("work").unwrap();
    store.enqueue("work", "item-1", b"{}").unwrap();

    let coordination = CoordinationManager::new();

    let scanner = DefaultResourceScanner::new(&adapters, &store, &coordination);
    let resources = scanner.scan(&ScannerSource::Queue { name: "work".into() }).unwrap();

    assert_eq!(resources.len(), 1);
    assert!(resources[0].id.contains("queue:work:item-1"));
}

#[test]
fn scan_command_output_json() {
    let adapters = FakeAdapters::new();
    let store = WalStore::new_in_memory();
    let coordination = CoordinationManager::new();

    let scanner = DefaultResourceScanner::new(&adapters, &store, &coordination);
    let resources = scanner.scan(&ScannerSource::Command {
        command: r#"echo '[{"id":"res-1","age_seconds":100},{"id":"res-2","age_seconds":200}]'"#.into(),
    }).unwrap();

    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].id, "res-1");
    assert_eq!(resources[0].age, Some(Duration::from_secs(100)));
}
```

## Dependencies

Add to `crates/core/Cargo.toml` if not present:

```toml
[dependencies]
ureq = { version = "2", features = ["json"] }  # For HTTP fetching
```

## Landing Checklist

- [ ] `DefaultSourceFetcher` fetches from all source types
- [ ] `DefaultResourceScanner` scans all resource types
- [ ] Store query methods return correct data
- [ ] Command output parsing handles JSON and plain text
- [ ] HTTP fetching works for URL sources
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

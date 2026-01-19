# Epic 09g: Production Adapter Wiring

**Epic ID:** 09g
**Status:** Planning
**Depends on:** 09b, 09c

## Problem Statement

The engine currently uses `NoOp*` adapters during `tick_scheduling()` and related methods:

```rust
// Current code in engine/runtime.rs
let source_fetcher = NoOpSourceFetcher;
let resource_scanner = NoOpResourceScanner;

let mut controller = CronController::new(
    &mut self.scheduling_manager,  // &mut through self
    &source_fetcher,
    &resource_scanner,
    &self.clock,
);
```

This happens because:
1. `DefaultSourceFetcher::new()` needs `&MaterializedState` (from `self.store.state()`)
2. `DefaultResourceScanner::new()` needs `&MaterializedState` and `&Clock`
3. `CronController::new()` needs `&mut self.scheduling_manager`
4. All access `self`, causing borrow conflicts

The result is that watchers and scanners cannot actually query real system state during scheduling ticks.

## Solution: Two-Phase Execution with Fetch Requests

Refactor the scheduling flow into two phases:
1. **Planning phase**: Determine what needs to be fetched (produces `FetchRequest`s)
2. **Execution phase**: Fetch values, evaluate conditions, produce effects

This cleanly separates the `&mut SchedulingManager` access from the `&MaterializedState` access.

## Implementation Plan

### Phase 1: Define Fetch Request Types

**File:** `crates/core/src/scheduling/fetch.rs` (new file)

```rust
/// Request to fetch a source value
#[derive(Debug, Clone)]
pub enum FetchRequest {
    /// Fetch watcher source for condition evaluation
    WatcherSource {
        watcher_id: WatcherId,
        source: WatcherSource,
        context: FetchContext,
    },
    /// Scan resources for a scanner
    ScannerResources {
        scanner_id: ScannerId,
        source: ScannerSource,
    },
}

/// Result of a fetch request
#[derive(Debug, Clone)]
pub enum FetchResult {
    /// Watcher source value fetched
    WatcherValue {
        watcher_id: WatcherId,
        value: Result<SourceValue, FetchError>,
    },
    /// Scanner resources discovered
    ScannerResources {
        scanner_id: ScannerId,
        resources: Result<Vec<ResourceInfo>, ScanError>,
    },
}

/// Batch of fetch requests to execute
#[derive(Debug, Default)]
pub struct FetchBatch {
    requests: Vec<FetchRequest>,
}

impl FetchBatch {
    pub fn add(&mut self, request: FetchRequest) { ... }
    pub fn is_empty(&self) -> bool { ... }
    pub fn into_iter(self) -> impl Iterator<Item = FetchRequest> { ... }
}

/// Batch of fetch results
#[derive(Debug, Default)]
pub struct FetchResults {
    results: Vec<FetchResult>,
}

impl FetchResults {
    pub fn add(&mut self, result: FetchResult) { ... }
    pub fn watcher_value(&self, id: &WatcherId) -> Option<&Result<SourceValue, FetchError>> { ... }
    pub fn scanner_resources(&self, id: &ScannerId) -> Option<&Result<Vec<ResourceInfo>, ScanError>> { ... }
}
```

### Phase 2: Modify CronController for Two-Phase

**File:** `crates/core/src/scheduling/controller.rs`

Add methods that separate planning from execution:

```rust
impl<'a, S, R, C> CronController<'a, S, R, C>
where
    S: SourceFetcher,
    R: ResourceScanner,
    C: Clock,
{
    /// Plan what needs to be fetched for a cron tick (phase 1)
    /// Does NOT require SourceFetcher or ResourceScanner
    pub fn plan_cron_tick(&self, cron_id: &CronId) -> FetchBatch {
        let mut batch = FetchBatch::default();

        // Find linked watchers
        for watcher in self.manager.watchers_for_cron(cron_id) {
            batch.add(FetchRequest::WatcherSource {
                watcher_id: watcher.id.clone(),
                source: watcher.source.clone(),
                context: FetchContext::default(),
            });
        }

        // Find linked scanners
        for scanner in self.manager.scanners_for_cron(cron_id) {
            batch.add(FetchRequest::ScannerResources {
                scanner_id: scanner.id.clone(),
                source: scanner.source.clone(),
            });
        }

        batch
    }

    /// Execute a cron tick with pre-fetched results (phase 2)
    pub fn execute_cron_tick(
        &mut self,
        cron_id: &CronId,
        results: &FetchResults,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Process watchers with fetched values
        for watcher in self.manager.watchers_for_cron(cron_id) {
            if let Some(value_result) = results.watcher_value(&watcher.id) {
                match value_result {
                    Ok(value) => {
                        let watcher_effects = self.evaluate_watcher(&watcher.id, value);
                        effects.extend(watcher_effects);
                    }
                    Err(e) => {
                        tracing::warn!(?watcher.id, ?e, "failed to fetch watcher source");
                    }
                }
            }
        }

        // Process scanners with fetched resources
        for scanner in self.manager.scanners_for_cron(cron_id) {
            if let Some(resources_result) = results.scanner_resources(&scanner.id) {
                match resources_result {
                    Ok(resources) => {
                        let scanner_effects = self.evaluate_scanner(&scanner.id, resources);
                        effects.extend(scanner_effects);
                    }
                    Err(e) => {
                        tracing::warn!(?scanner.id, ?e, "failed to scan resources");
                    }
                }
            }
        }

        effects
    }
}
```

Also add a `plan_watcher_check()` method for event-driven wakes:

```rust
    /// Plan fetch for a single watcher check
    pub fn plan_watcher_check(&self, watcher_id: &WatcherId) -> Option<FetchRequest> {
        let watcher = self.manager.get_watcher(watcher_id)?;
        Some(FetchRequest::WatcherSource {
            watcher_id: watcher_id.clone(),
            source: watcher.source.clone(),
            context: FetchContext::default(),
        })
    }

    /// Execute a watcher check with pre-fetched value
    pub fn execute_watcher_check(
        &mut self,
        watcher_id: &WatcherId,
        value: &SourceValue,
    ) -> Vec<Effect> {
        self.evaluate_watcher(watcher_id, value)
    }
```

### Phase 3: Add FetchExecutor

**File:** `crates/core/src/scheduling/fetch.rs` (continued)

```rust
/// Executes fetch requests against real system state
pub struct FetchExecutor<'a, S: SourceFetcher, R: ResourceScanner> {
    source_fetcher: &'a S,
    resource_scanner: &'a R,
}

impl<'a, S: SourceFetcher, R: ResourceScanner> FetchExecutor<'a, S, R> {
    pub fn new(source_fetcher: &'a S, resource_scanner: &'a R) -> Self {
        Self { source_fetcher, resource_scanner }
    }

    /// Execute a batch of fetch requests
    pub fn execute(&self, batch: FetchBatch) -> FetchResults {
        let mut results = FetchResults::default();

        for request in batch {
            match request {
                FetchRequest::WatcherSource { watcher_id, source, context } => {
                    let value = self.source_fetcher.fetch(&source, &context);
                    results.add(FetchResult::WatcherValue { watcher_id, value });
                }
                FetchRequest::ScannerResources { scanner_id, source } => {
                    let resources = self.resource_scanner.scan(&source);
                    results.add(FetchResult::ScannerResources { scanner_id, resources });
                }
            }
        }

        results
    }
}
```

### Phase 4: Update Engine Integration

**File:** `crates/core/src/engine/runtime.rs`

Refactor `on_cron_tick()` to use two-phase approach:

```rust
/// Handle a cron tick with full integration (two-phase)
fn on_cron_tick(&mut self, cron_id: &CronId) -> Vec<Effect> {
    // Phase 0: Tick the cron state machine
    let cron_effects = self.scheduling_manager.tick_cron(cron_id, &self.clock);

    // Phase 1: Plan what needs to be fetched (no state access needed)
    let fetch_batch = {
        let controller = CronController::new_readonly(&self.scheduling_manager);
        controller.plan_cron_tick(cron_id)
    };

    // Phase 2: Execute fetches using production adapters
    let fetch_results = if !fetch_batch.is_empty() {
        let state = self.store.state();
        let source_fetcher = DefaultSourceFetcher::new(state);
        let resource_scanner = DefaultResourceScanner::new(state, &self.clock);
        let executor = FetchExecutor::new(&source_fetcher, &resource_scanner);
        executor.execute(fetch_batch)
    } else {
        FetchResults::default()
    };

    // Phase 3: Execute with fetched results (mutable access to scheduling_manager)
    let orchestration_effects = {
        // Use a lightweight controller that doesn't need fetcher/scanner
        self.scheduling_manager.execute_cron_tick_with_results(cron_id, &fetch_results, &self.clock)
    };

    // Phase 4: Complete the cron
    let completion_effects = self.scheduling_manager.complete_cron(cron_id, &self.clock);

    [cron_effects, orchestration_effects, completion_effects].concat()
}
```

Similarly update `check_watcher_immediate()`:

```rust
/// Check a watcher immediately (event-driven wake)
fn check_watcher_immediate(&mut self, watcher_id: &WatcherId) -> Vec<Effect> {
    // Phase 1: Plan fetch
    let fetch_request = {
        let controller = CronController::new_readonly(&self.scheduling_manager);
        controller.plan_watcher_check(watcher_id)
    };

    let Some(request) = fetch_request else {
        return Vec::new();
    };

    // Phase 2: Execute fetch
    let value = {
        let state = self.store.state();
        let source_fetcher = DefaultSourceFetcher::new(state);
        match &request {
            FetchRequest::WatcherSource { source, context, .. } => {
                source_fetcher.fetch(source, context)
            }
            _ => return Vec::new(),
        }
    };

    // Phase 3: Evaluate with result
    match value {
        Ok(v) => self.scheduling_manager.evaluate_watcher_with_value(watcher_id, &v, &self.clock),
        Err(e) => {
            tracing::warn!(?watcher_id, ?e, "failed to fetch watcher source");
            Vec::new()
        }
    }
}
```

### Phase 5: Add Readonly Controller Constructor

**File:** `crates/core/src/scheduling/controller.rs`

```rust
/// Readonly controller for planning phase (no fetcher/scanner needed)
pub struct CronControllerReadonly<'a> {
    manager: &'a SchedulingManager,
}

impl<'a> CronControllerReadonly<'a> {
    pub fn new(manager: &'a SchedulingManager) -> Self {
        Self { manager }
    }

    pub fn plan_cron_tick(&self, cron_id: &CronId) -> FetchBatch { ... }
    pub fn plan_watcher_check(&self, watcher_id: &WatcherId) -> Option<FetchRequest> { ... }
}

// Add to CronController
impl<'a, S, R, C> CronController<'a, S, R, C> {
    pub fn new_readonly(manager: &'a SchedulingManager) -> CronControllerReadonly<'a> {
        CronControllerReadonly::new(manager)
    }
}
```

### Phase 6: Add SchedulingManager Methods

**File:** `crates/core/src/scheduling/manager.rs`

```rust
impl SchedulingManager {
    /// Execute cron tick with pre-fetched results
    pub fn execute_cron_tick_with_results(
        &mut self,
        cron_id: &CronId,
        results: &FetchResults,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Get linked watchers/scanners
        let watcher_ids: Vec<_> = self.watchers_for_cron(cron_id)
            .map(|w| w.id.clone())
            .collect();
        let scanner_ids: Vec<_> = self.scanners_for_cron(cron_id)
            .map(|s| s.id.clone())
            .collect();

        // Evaluate watchers
        for watcher_id in watcher_ids {
            if let Some(Ok(value)) = results.watcher_value(&watcher_id) {
                let watcher_effects = self.evaluate_watcher_with_value(&watcher_id, value, clock);
                effects.extend(watcher_effects);
            }
        }

        // Evaluate scanners
        for scanner_id in scanner_ids {
            if let Some(Ok(resources)) = results.scanner_resources(&scanner_id) {
                let scanner_effects = self.evaluate_scanner_with_resources(&scanner_id, resources, clock);
                effects.extend(scanner_effects);
            }
        }

        effects
    }

    /// Evaluate a watcher with a pre-fetched value
    pub fn evaluate_watcher_with_value(
        &mut self,
        watcher_id: &WatcherId,
        value: &SourceValue,
        clock: &impl Clock,
    ) -> Vec<Effect> { ... }

    /// Evaluate a scanner with pre-fetched resources
    pub fn evaluate_scanner_with_resources(
        &mut self,
        scanner_id: &ScannerId,
        resources: &[ResourceInfo],
        clock: &impl Clock,
    ) -> Vec<Effect> { ... }
}
```

### Phase 7: Remove NoOp Usage in Engine

After all the above is in place, remove the `NoOp*` imports and usage from `engine/runtime.rs`:

```rust
// REMOVE these imports
use crate::scheduling::{
    NoOpCommandRunner, NoOpCoordinationCleanup, NoOpResourceScanner,
    NoOpSessionCleanup, NoOpSourceFetcher, NoOpStorageCleanup,
    NoOpTaskStarter, NoOpWorktreeCleanup,
};

// KEEP NoOp* types for testing only (move to test modules)
```

## File Changes Summary

| File | Change |
|------|--------|
| `scheduling/fetch.rs` | **New** - FetchRequest, FetchResult, FetchBatch, FetchResults, FetchExecutor |
| `scheduling/controller.rs` | Add CronControllerReadonly, plan_* methods, execute_*_with_results methods |
| `scheduling/manager.rs` | Add execute_cron_tick_with_results, evaluate_*_with_* methods |
| `scheduling/mod.rs` | Export new types |
| `engine/runtime.rs` | Refactor on_cron_tick, check_watcher_immediate to two-phase |

## Testing Strategy

1. **Unit tests for FetchBatch/FetchResults**
   - Add/retrieve requests and results
   - Empty batch handling

2. **Unit tests for FetchExecutor**
   - Mock fetcher/scanner to verify request execution
   - Error propagation

3. **Integration tests for two-phase execution**
   - Test that CronController planning produces correct requests
   - Test that execution with results produces correct effects

4. **Engine integration tests**
   - Mock MaterializedState with specific values
   - Verify watchers/scanners query real state
   - Verify effects are produced correctly

## Verification Checklist

- [ ] `FetchRequest` and `FetchResult` enums defined
- [ ] `FetchBatch` and `FetchResults` collection types work
- [ ] `FetchExecutor` executes batches correctly
- [ ] `CronControllerReadonly` can plan without fetcher/scanner
- [ ] `SchedulingManager` can execute with pre-fetched results
- [ ] `Engine::on_cron_tick` uses two-phase approach
- [ ] `Engine::check_watcher_immediate` uses two-phase approach
- [ ] No `NoOp*` usage in production engine code paths
- [ ] `./checks/lint.sh` passes
- [ ] `make check` passes

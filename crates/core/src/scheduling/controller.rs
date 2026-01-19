// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CronController - orchestrates cron execution with watcher/scanner integration
//!
//! The CronController connects cron ticks to watcher checks and scanner runs,
//! bridging the pure state machines with the I/O layer.

use super::fetch::{FetchBatch, FetchRequest, FetchResults};
use super::{
    CronId, ResourceInfo, ScannerId, SchedulingManager, SourceValue, WatcherId, WatcherSource,
};
use crate::clock::Clock;
use crate::effect::Effect;
use std::collections::BTreeMap;

/// Context for template interpolation in source fetching
#[derive(Debug, Clone, Default)]
pub struct FetchContext {
    pub variables: BTreeMap<String, String>,
}

impl FetchContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }
}

/// Error when fetching a source value
#[derive(Debug, Clone)]
pub enum FetchError {
    /// Command execution failed
    CommandFailed { message: String },
    /// Session not found
    SessionNotFound { name: String },
    /// Task not found
    TaskNotFound { id: String },
    /// Failed to parse output
    ParseError { message: String },
    /// Generic error
    Other { message: String },
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::CommandFailed { message } => write!(f, "command failed: {}", message),
            FetchError::SessionNotFound { name } => write!(f, "session not found: {}", name),
            FetchError::TaskNotFound { id } => write!(f, "task not found: {}", id),
            FetchError::ParseError { message } => write!(f, "parse error: {}", message),
            FetchError::Other { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for FetchError {}

/// Fetches source values for watcher condition evaluation
pub trait SourceFetcher: Send + Sync {
    /// Fetch the current value of a watcher source
    fn fetch(
        &self,
        source: &WatcherSource,
        context: &FetchContext,
    ) -> Result<SourceValue, FetchError>;
}

/// Error when scanning for resources
#[derive(Debug, Clone)]
pub enum ScanError {
    /// Failed to list resources
    ListFailed { message: String },
    /// Command execution failed
    CommandFailed { message: String },
    /// Generic error
    Other { message: String },
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::ListFailed { message } => write!(f, "list failed: {}", message),
            ScanError::CommandFailed { message } => write!(f, "command failed: {}", message),
            ScanError::Other { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for ScanError {}

/// Discovers resources for scanner condition evaluation
pub trait ResourceScanner: Send + Sync {
    /// Scan for resources matching the source type
    fn scan(&self, source: &super::ScannerSource) -> Result<Vec<ResourceInfo>, ScanError>;
}

/// No-op source fetcher for testing
pub struct NoOpSourceFetcher;

impl SourceFetcher for NoOpSourceFetcher {
    fn fetch(
        &self,
        _source: &WatcherSource,
        _context: &FetchContext,
    ) -> Result<SourceValue, FetchError> {
        Ok(SourceValue::Numeric { value: 0 })
    }
}

/// No-op resource scanner for testing
pub struct NoOpResourceScanner;

impl ResourceScanner for NoOpResourceScanner {
    fn scan(&self, _source: &super::ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        Ok(vec![])
    }
}

/// Orchestrates cron execution - connects cron ticks to watcher/scanner checks
pub struct CronController<'a, C: Clock> {
    manager: &'a mut SchedulingManager,
    source_fetcher: Option<&'a dyn SourceFetcher>,
    resource_scanner: Option<&'a dyn ResourceScanner>,
    clock: &'a C,
}

impl<'a, C: Clock> CronController<'a, C> {
    /// Create a new CronController with fetcher and scanner for legacy API
    pub fn new(
        manager: &'a mut SchedulingManager,
        source_fetcher: &'a dyn SourceFetcher,
        resource_scanner: &'a dyn ResourceScanner,
        clock: &'a C,
    ) -> Self {
        Self {
            manager,
            source_fetcher: Some(source_fetcher),
            resource_scanner: Some(resource_scanner),
            clock,
        }
    }

    /// Create a CronController for execution phase (no fetcher/scanner needed)
    ///
    /// Use this constructor when you have pre-fetched values and only need
    /// to execute with results via `execute_cron_tick_with_results`.
    pub fn new_for_execution(manager: &'a mut SchedulingManager, clock: &'a C) -> Self {
        Self {
            manager,
            source_fetcher: None,
            resource_scanner: None,
            clock,
        }
    }

    /// Called when a cron timer fires
    pub fn on_cron_tick(&mut self, cron_id: &CronId) -> Vec<Effect> {
        let mut effects = self.manager.tick_cron(cron_id, self.clock);

        // Get cron config to find linked watchers/scanners
        let (watchers, scanners) = if let Some(cron) = self.manager.get_cron(cron_id) {
            (cron.watchers.clone(), cron.scanners.clone())
        } else {
            return effects;
        };

        // Trigger each linked watcher
        for watcher_id in &watchers {
            let source_effects = self.check_watcher(watcher_id);
            effects.extend(source_effects);
        }

        // Trigger each linked scanner
        for scanner_id in &scanners {
            let scan_effects = self.run_scanner(scanner_id);
            effects.extend(scan_effects);
        }

        effects
    }

    /// Check a watcher by fetching its source value
    ///
    /// Note: Requires `source_fetcher` to be set. Returns empty effects if not set.
    /// Prefer using `execute_watcher_check` with pre-fetched values.
    pub fn check_watcher(&mut self, watcher_id: &WatcherId) -> Vec<Effect> {
        let Some(source_fetcher) = self.source_fetcher else {
            tracing::warn!("check_watcher called without source_fetcher");
            return vec![];
        };

        // Clone source to avoid borrow issues
        let source = {
            let Some(watcher) = self.manager.get_watcher(watcher_id) else {
                return vec![];
            };
            watcher.source.clone()
        };

        // Build context from watcher
        let context = self.build_context(watcher_id);

        // Fetch source value
        let source_value = match source_fetcher.fetch(&source, &context) {
            Ok(value) => value,
            Err(e) => SourceValue::Error {
                message: e.to_string(),
            },
        };

        // Check watcher with fetched value
        self.manager
            .check_watcher(watcher_id, source_value, self.clock)
    }

    /// Run a scanner by fetching resources
    ///
    /// Note: Requires `resource_scanner` to be set. Returns empty effects if not set.
    /// Prefer using `execute_cron_tick_with_results` with pre-fetched resources.
    pub fn run_scanner(&mut self, scanner_id: &ScannerId) -> Vec<Effect> {
        let Some(resource_scanner) = self.resource_scanner else {
            tracing::warn!("run_scanner called without resource_scanner");
            return vec![];
        };

        // Clone source to avoid borrow issues
        let source = {
            let Some(scanner) = self.manager.get_scanner(scanner_id) else {
                return vec![];
            };
            scanner.source.clone()
        };

        // Start scanning
        let mut effects = self.manager.tick_scanner(scanner_id, self.clock);

        // Fetch resources
        let resources = match resource_scanner.scan(&source) {
            Ok(r) => r,
            Err(e) => {
                // Fail the scan
                return self
                    .manager
                    .cleanup_failed(scanner_id, e.to_string(), self.clock);
            }
        };

        // Complete scan with discovered resources
        effects.extend(
            self.manager
                .scan_complete(scanner_id, resources, self.clock),
        );

        effects
    }

    /// Build context for template interpolation
    fn build_context(&self, watcher_id: &WatcherId) -> FetchContext {
        FetchContext::new().with_variable("watcher_id", watcher_id.0.clone())
    }
}

// ==================== Readonly Controller for Planning Phase ====================

/// Readonly controller for the planning phase (no fetcher/scanner needed)
///
/// This controller can plan what needs to be fetched without requiring
/// access to `SourceFetcher` or `ResourceScanner`. It only reads from
/// the `SchedulingManager` to determine which watchers and scanners
/// are linked to a cron.
pub struct CronControllerReadonly<'a> {
    manager: &'a SchedulingManager,
}

impl<'a> CronControllerReadonly<'a> {
    /// Create a new readonly controller
    pub fn new(manager: &'a SchedulingManager) -> Self {
        Self { manager }
    }

    /// Plan what needs to be fetched for a cron tick (phase 1)
    ///
    /// This method determines which watchers and scanners are linked to the cron
    /// and generates fetch requests for them. It does NOT require `SourceFetcher`
    /// or `ResourceScanner`.
    pub fn plan_cron_tick(&self, cron_id: &CronId) -> FetchBatch {
        let mut batch = FetchBatch::default();

        // Get cron config to find linked watchers/scanners
        let Some(cron) = self.manager.get_cron(cron_id) else {
            return batch;
        };

        // Plan fetches for linked watchers
        for watcher_id in &cron.watchers {
            if let Some(watcher) = self.manager.get_watcher(watcher_id) {
                batch.add(FetchRequest::WatcherSource {
                    watcher_id: watcher_id.clone(),
                    source: watcher.source.clone(),
                    context: FetchContext::new().with_variable("watcher_id", watcher_id.0.clone()),
                });
            }
        }

        // Plan fetches for linked scanners
        for scanner_id in &cron.scanners {
            if let Some(scanner) = self.manager.get_scanner(scanner_id) {
                batch.add(FetchRequest::ScannerResources {
                    scanner_id: scanner_id.clone(),
                    source: scanner.source.clone(),
                });
            }
        }

        batch
    }

    /// Plan fetch for a single watcher check
    ///
    /// Returns a fetch request for the watcher's source, or None if the watcher
    /// doesn't exist.
    pub fn plan_watcher_check(&self, watcher_id: &WatcherId) -> Option<FetchRequest> {
        let watcher = self.manager.get_watcher(watcher_id)?;
        Some(FetchRequest::WatcherSource {
            watcher_id: watcher_id.clone(),
            source: watcher.source.clone(),
            context: FetchContext::new().with_variable("watcher_id", watcher_id.0.clone()),
        })
    }

    /// Get the manager reference
    pub fn manager(&self) -> &SchedulingManager {
        self.manager
    }
}

impl<'a, C: Clock> CronController<'a, C> {
    /// Create a readonly controller for the planning phase
    pub fn new_readonly(manager: &'a SchedulingManager) -> CronControllerReadonly<'a> {
        CronControllerReadonly::new(manager)
    }

    /// Execute a cron tick with pre-fetched results (phase 2)
    ///
    /// This method processes the watchers and scanners linked to the cron
    /// using the pre-fetched values from `FetchResults`.
    pub fn execute_cron_tick_with_results(
        &mut self,
        cron_id: &CronId,
        results: &FetchResults,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();

        // Get cron config to find linked watchers/scanners
        let Some(cron) = self.manager.get_cron(cron_id) else {
            return effects;
        };

        let watcher_ids = cron.watchers.clone();
        let scanner_ids = cron.scanners.clone();

        // Process watchers with fetched values
        for watcher_id in &watcher_ids {
            if let Some(value_result) = results.watcher_value(watcher_id) {
                match value_result {
                    Ok(value) => {
                        let watcher_effects =
                            self.manager
                                .check_watcher(watcher_id, value.clone(), self.clock);
                        effects.extend(watcher_effects);
                    }
                    Err(e) => {
                        tracing::warn!(?watcher_id, error = %e, "failed to fetch watcher source");
                        // Check with error value so watcher can track consecutive failures
                        let watcher_effects = self.manager.check_watcher(
                            watcher_id,
                            SourceValue::Error {
                                message: e.to_string(),
                            },
                            self.clock,
                        );
                        effects.extend(watcher_effects);
                    }
                }
            }
        }

        // Process scanners with fetched resources
        for scanner_id in &scanner_ids {
            if let Some(resources_result) = results.scanner_resources(scanner_id) {
                match resources_result {
                    Ok(resources) => {
                        // Start scanning
                        let scan_effects = self.manager.tick_scanner(scanner_id, self.clock);
                        effects.extend(scan_effects);

                        // Complete scan with discovered resources
                        let complete_effects =
                            self.manager
                                .scan_complete(scanner_id, resources.clone(), self.clock);
                        effects.extend(complete_effects);
                    }
                    Err(e) => {
                        tracing::warn!(?scanner_id, error = %e, "failed to scan resources");
                        let failure_effects =
                            self.manager
                                .cleanup_failed(scanner_id, e.to_string(), self.clock);
                        effects.extend(failure_effects);
                    }
                }
            }
        }

        effects
    }

    /// Execute a watcher check with a pre-fetched value
    pub fn execute_watcher_check(
        &mut self,
        watcher_id: &WatcherId,
        value: &SourceValue,
    ) -> Vec<Effect> {
        self.manager
            .check_watcher(watcher_id, value.clone(), self.clock)
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;

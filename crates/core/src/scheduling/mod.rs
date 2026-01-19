// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Scheduling primitives for time-driven execution and monitoring
//!
//! This module provides:
//! - **Cron**: Named scheduled tasks running at fixed intervals
//! - **Action**: Named operations with cooldown enforcement
//! - **Watcher**: Condition monitoring with response chains
//! - **Scanner**: Resource scanning with condition-based cleanup
//! - **SchedulingManager**: Unified interface for all scheduling primitives

mod action;
mod bridge;
mod cleanup;
mod controller;
mod cron;
mod executor;
mod fetch;
mod manager;
mod resource;
mod scanner;
mod source;
mod watcher;

#[cfg(test)]
#[path = "action_tests.rs"]
mod action_tests;

#[cfg(test)]
#[path = "cron_tests.rs"]
mod cron_tests;

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod scanner_tests;

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod watcher_tests;

pub use action::{
    Action, ActionConfig, ActionEvent, ActionExecution, ActionId, ActionState, DecisionRule,
};
pub use bridge::{EventPattern, WatcherEventBridge};
pub use cleanup::{
    CleanupError, CleanupExecutor, CleanupResult, CoordinationCleanup, NoOpCoordinationCleanup,
    NoOpSessionCleanup, NoOpStorageCleanup, NoOpWorktreeCleanup, SessionCleanup, StorageCleanup,
    WorktreeCleanup,
};
pub use controller::{
    CronController, CronControllerReadonly, FetchContext, FetchError, NoOpResourceScanner,
    NoOpSourceFetcher, ResourceScanner, ScanError, SourceFetcher,
};
pub use cron::{Cron, CronConfig, CronEvent, CronId, CronState};
pub use executor::{
    ActionExecutor, ActionResult, AlwaysTrueEvaluator, CommandOutput, CommandRunner,
    ConditionEvaluator, ExecutionContext, ExecutionError, NoOpCommandRunner, NoOpTaskStarter,
    TaskStarter,
};
pub use scanner::{
    CleanupAction, ResourceInfo, Scanner, ScannerCondition, ScannerConfig, ScannerEvent, ScannerId,
    ScannerSource, ScannerState,
};
pub use watcher::{
    SourceValue, Watcher, WatcherCondition, WatcherConfig, WatcherEvent, WatcherId,
    WatcherResponse, WatcherSource, WatcherState,
};

pub use fetch::{FetchBatch, FetchExecutor, FetchRequest, FetchResult, FetchResults};
pub use manager::{SchedulingManager, SchedulingStats};
pub use resource::DefaultResourceScanner;
pub use source::DefaultSourceFetcher;

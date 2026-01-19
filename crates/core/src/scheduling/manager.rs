// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! SchedulingManager - unified interface for all scheduling primitives
//!
//! The SchedulingManager provides a single point of access for managing:
//! - Crons: Fixed-interval scheduled tasks
//! - Actions: Named operations with cooldown enforcement
//! - Watchers: Condition monitoring with response chains
//! - Scanners: Resource cleanup scanning

use super::{
    Action, ActionConfig, ActionEvent, ActionId, Cron, CronConfig, CronEvent, CronId, CronState,
    ResourceInfo, Scanner, ScannerConfig, ScannerEvent, ScannerId, ScannerState, SourceValue,
    Watcher, WatcherConfig, WatcherEvent, WatcherId, WatcherState,
};
use crate::clock::Clock;
use crate::effect::Effect;
use std::collections::HashMap;

/// Statistics about the scheduling system
#[derive(Debug, Clone, Default)]
pub struct SchedulingStats {
    /// Total number of crons
    pub total_crons: usize,
    /// Number of enabled crons
    pub enabled_crons: usize,
    /// Number of running crons
    pub running_crons: usize,
    /// Total number of actions
    pub total_actions: usize,
    /// Number of actions currently on cooldown
    pub actions_on_cooldown: usize,
    /// Total number of watchers
    pub total_watchers: usize,
    /// Number of active watchers
    pub active_watchers: usize,
    /// Number of triggered watchers
    pub triggered_watchers: usize,
    /// Total number of scanners
    pub total_scanners: usize,
    /// Number of scanning scanners
    pub scanning_scanners: usize,
    /// Total resources cleaned by all scanners
    pub total_resources_cleaned: u64,
}

/// Unified manager for all scheduling primitives
#[derive(Debug, Clone, Default)]
pub struct SchedulingManager {
    crons: HashMap<CronId, Cron>,
    actions: HashMap<ActionId, Action>,
    watchers: HashMap<WatcherId, Watcher>,
    scanners: HashMap<ScannerId, Scanner>,
}

impl SchedulingManager {
    /// Create a new empty scheduling manager
    pub fn new() -> Self {
        Self::default()
    }

    // ==================== Cron Operations ====================

    /// Add a cron job
    pub fn add_cron(&mut self, id: CronId, config: CronConfig, clock: &impl Clock) -> Vec<Effect> {
        let cron = Cron::new(id.clone(), config, clock);
        let effects = if cron.state == CronState::Enabled {
            vec![Effect::SetTimer {
                id: cron.timer_id(),
                duration: cron.interval,
            }]
        } else {
            vec![]
        };
        self.crons.insert(id, cron);
        effects
    }

    /// Get a cron by ID
    pub fn get_cron(&self, id: &CronId) -> Option<&Cron> {
        self.crons.get(id)
    }

    /// Remove a cron
    pub fn remove_cron(&mut self, id: &CronId) -> Option<Cron> {
        self.crons.remove(id)
    }

    /// Enable a cron
    pub fn enable_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(cron) = self.crons.get(id) {
            let (new_cron, effects) = cron.transition(CronEvent::Enable, clock);
            self.crons.insert(id.clone(), new_cron);
            effects
        } else {
            vec![]
        }
    }

    /// Disable a cron
    pub fn disable_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(cron) = self.crons.get(id) {
            let (new_cron, effects) = cron.transition(CronEvent::Disable, clock);
            self.crons.insert(id.clone(), new_cron);
            effects
        } else {
            vec![]
        }
    }

    /// Process a cron tick (timer fired)
    pub fn tick_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(cron) = self.crons.get(id) {
            let (new_cron, effects) = cron.transition(CronEvent::Tick, clock);
            self.crons.insert(id.clone(), new_cron);
            effects
        } else {
            vec![]
        }
    }

    /// Complete a cron execution
    pub fn complete_cron(&mut self, id: &CronId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(cron) = self.crons.get(id) {
            let (new_cron, effects) = cron.transition(CronEvent::Complete, clock);
            self.crons.insert(id.clone(), new_cron);
            effects
        } else {
            vec![]
        }
    }

    /// Fail a cron execution
    pub fn fail_cron(&mut self, id: &CronId, error: String, clock: &impl Clock) -> Vec<Effect> {
        if let Some(cron) = self.crons.get(id) {
            let (new_cron, effects) = cron.transition(CronEvent::Fail { error }, clock);
            self.crons.insert(id.clone(), new_cron);
            effects
        } else {
            vec![]
        }
    }

    /// Iterate over all crons
    pub fn crons(&self) -> impl Iterator<Item = &Cron> {
        self.crons.values()
    }

    // ==================== Action Operations ====================

    /// Add an action
    pub fn add_action(&mut self, id: ActionId, config: ActionConfig) {
        let action = Action::new(id.clone(), config);
        self.actions.insert(id, action);
    }

    /// Get an action by ID
    pub fn get_action(&self, id: &ActionId) -> Option<&Action> {
        self.actions.get(id)
    }

    /// Remove an action
    pub fn remove_action(&mut self, id: &ActionId) -> Option<Action> {
        self.actions.remove(id)
    }

    /// Trigger an action
    pub fn trigger_action(
        &mut self,
        id: &ActionId,
        source: &str,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(action) = self.actions.get(id) {
            let (new_action, effects) = action.transition(
                ActionEvent::Trigger {
                    source: source.to_string(),
                },
                clock,
            );
            self.actions.insert(id.clone(), new_action);
            effects
        } else {
            vec![]
        }
    }

    /// Complete an action
    pub fn complete_action(&mut self, id: &ActionId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(action) = self.actions.get(id) {
            let (new_action, effects) = action.transition(ActionEvent::Complete, clock);
            self.actions.insert(id.clone(), new_action);
            effects
        } else {
            vec![]
        }
    }

    /// Fail an action
    pub fn fail_action(&mut self, id: &ActionId, error: String, clock: &impl Clock) -> Vec<Effect> {
        if let Some(action) = self.actions.get(id) {
            let (new_action, effects) = action.transition(ActionEvent::Fail { error }, clock);
            self.actions.insert(id.clone(), new_action);
            effects
        } else {
            vec![]
        }
    }

    /// Process cooldown expired event
    pub fn cooldown_expired(&mut self, id: &ActionId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(action) = self.actions.get(id) {
            let (new_action, effects) = action.transition(ActionEvent::CooldownExpired, clock);
            self.actions.insert(id.clone(), new_action);
            effects
        } else {
            vec![]
        }
    }

    /// Check if an action can be triggered
    pub fn can_trigger_action(&self, id: &ActionId) -> bool {
        self.actions.get(id).is_some_and(|a| a.can_trigger())
    }

    /// Iterate over all actions
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    // ==================== Watcher Operations ====================

    /// Add a watcher
    pub fn add_watcher(
        &mut self,
        id: WatcherId,
        config: WatcherConfig,
        _clock: &impl Clock,
    ) -> Vec<Effect> {
        let watcher = Watcher::new(id.clone(), config);
        // Schedule initial check
        let effects = vec![Effect::SetTimer {
            id: watcher.check_timer_id(),
            duration: watcher.check_interval,
        }];
        self.watchers.insert(id, watcher);
        effects
    }

    /// Get a watcher by ID
    pub fn get_watcher(&self, id: &WatcherId) -> Option<&Watcher> {
        self.watchers.get(id)
    }

    /// Remove a watcher
    pub fn remove_watcher(&mut self, id: &WatcherId) -> Option<Watcher> {
        self.watchers.remove(id)
    }

    /// Check a watcher with a source value
    pub fn check_watcher(
        &mut self,
        id: &WatcherId,
        value: SourceValue,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) = watcher.transition(WatcherEvent::Check { value }, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Mark watcher response as succeeded
    pub fn watcher_response_succeeded(
        &mut self,
        id: &WatcherId,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) = watcher.transition(WatcherEvent::ResponseSucceeded, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Mark watcher response as failed
    pub fn watcher_response_failed(&mut self, id: &WatcherId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) = watcher.transition(WatcherEvent::ResponseFailed, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Handle response delay expired
    pub fn watcher_response_delay_expired(
        &mut self,
        id: &WatcherId,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) =
                watcher.transition(WatcherEvent::ResponseDelayExpired, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Pause a watcher
    pub fn pause_watcher(&mut self, id: &WatcherId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) = watcher.transition(WatcherEvent::Pause, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Resume a watcher
    pub fn resume_watcher(&mut self, id: &WatcherId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(watcher) = self.watchers.get(id) {
            let (new_watcher, effects) = watcher.transition(WatcherEvent::Resume, clock);
            self.watchers.insert(id.clone(), new_watcher);
            effects
        } else {
            vec![]
        }
    }

    /// Iterate over all watchers
    pub fn watchers(&self) -> impl Iterator<Item = &Watcher> {
        self.watchers.values()
    }

    // ==================== Scanner Operations ====================

    /// Add a scanner
    pub fn add_scanner(
        &mut self,
        id: ScannerId,
        config: ScannerConfig,
        _clock: &impl Clock,
    ) -> Vec<Effect> {
        let scanner = Scanner::new(id.clone(), config);
        // Schedule initial scan
        let effects = vec![Effect::SetTimer {
            id: scanner.timer_id(),
            duration: scanner.scan_interval,
        }];
        self.scanners.insert(id, scanner);
        effects
    }

    /// Get a scanner by ID
    pub fn get_scanner(&self, id: &ScannerId) -> Option<&Scanner> {
        self.scanners.get(id)
    }

    /// Remove a scanner
    pub fn remove_scanner(&mut self, id: &ScannerId) -> Option<Scanner> {
        self.scanners.remove(id)
    }

    /// Tick a scanner (timer fired)
    pub fn tick_scanner(&mut self, id: &ScannerId, clock: &impl Clock) -> Vec<Effect> {
        if let Some(scanner) = self.scanners.get(id) {
            let (new_scanner, effects) = scanner.transition(ScannerEvent::Tick, clock);
            self.scanners.insert(id.clone(), new_scanner);
            effects
        } else {
            vec![]
        }
    }

    /// Complete a scan with found resources
    pub fn scan_complete(
        &mut self,
        id: &ScannerId,
        resources: Vec<ResourceInfo>,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(scanner) = self.scanners.get(id) {
            let (new_scanner, effects) =
                scanner.transition(ScannerEvent::ScanComplete { resources }, clock);
            self.scanners.insert(id.clone(), new_scanner);
            effects
        } else {
            vec![]
        }
    }

    /// Complete cleanup
    pub fn cleanup_complete(
        &mut self,
        id: &ScannerId,
        count: u64,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(scanner) = self.scanners.get(id) {
            let (new_scanner, effects) =
                scanner.transition(ScannerEvent::CleanupComplete { count }, clock);
            self.scanners.insert(id.clone(), new_scanner);
            effects
        } else {
            vec![]
        }
    }

    /// Cleanup failed
    pub fn cleanup_failed(
        &mut self,
        id: &ScannerId,
        error: String,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(scanner) = self.scanners.get(id) {
            let (new_scanner, effects) =
                scanner.transition(ScannerEvent::CleanupFailed { error }, clock);
            self.scanners.insert(id.clone(), new_scanner);
            effects
        } else {
            vec![]
        }
    }

    /// Iterate over all scanners
    pub fn scanners(&self) -> impl Iterator<Item = &Scanner> {
        self.scanners.values()
    }

    // ==================== Bulk Operations ====================

    /// Get statistics about the scheduling system
    pub fn stats(&self) -> SchedulingStats {
        SchedulingStats {
            total_crons: self.crons.len(),
            enabled_crons: self
                .crons
                .values()
                .filter(|c| c.state == CronState::Enabled)
                .count(),
            running_crons: self
                .crons
                .values()
                .filter(|c| c.state == CronState::Running)
                .count(),
            total_actions: self.actions.len(),
            actions_on_cooldown: self.actions.values().filter(|a| a.is_on_cooldown()).count(),
            total_watchers: self.watchers.len(),
            active_watchers: self
                .watchers
                .values()
                .filter(|w| w.state == WatcherState::Active)
                .count(),
            triggered_watchers: self
                .watchers
                .values()
                .filter(|w| matches!(w.state, WatcherState::Triggered { .. }))
                .count(),
            total_scanners: self.scanners.len(),
            scanning_scanners: self
                .scanners
                .values()
                .filter(|s| s.state == ScannerState::Scanning)
                .count(),
            total_resources_cleaned: self.scanners.values().map(|s| s.total_cleaned).sum(),
        }
    }

    /// Process a timer event by ID
    ///
    /// Returns effects for the timer, or empty if the timer ID isn't recognized.
    pub fn process_timer(&mut self, timer_id: &str, clock: &impl Clock) -> Vec<Effect> {
        // Parse timer ID to determine type
        if let Some(cron_id) = timer_id.strip_prefix("cron:") {
            self.tick_cron(&CronId::new(cron_id), clock)
        } else if let Some(rest) = timer_id.strip_prefix("action:") {
            if let Some(action_id) = rest.strip_suffix(":cooldown") {
                self.cooldown_expired(&ActionId::new(action_id), clock)
            } else {
                vec![]
            }
        } else if let Some(rest) = timer_id.strip_prefix("watcher:") {
            if rest.strip_suffix(":check").is_some() {
                // Check timer fired - but we don't have the source value here
                // The engine needs to fetch the source value and call check_watcher
                vec![]
            } else if let Some(watcher_id) = rest.strip_suffix(":response") {
                self.watcher_response_delay_expired(&WatcherId::new(watcher_id), clock)
            } else {
                vec![]
            }
        } else if let Some(scanner_id) = timer_id.strip_prefix("scanner:") {
            self.tick_scanner(&ScannerId::new(scanner_id), clock)
        } else {
            vec![]
        }
    }

    /// Clear all scheduling primitives
    pub fn clear(&mut self) {
        self.crons.clear();
        self.actions.clear();
        self.watchers.clear();
        self.scanners.clear();
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

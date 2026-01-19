use std::collections::HashMap;
// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use crate::scheduling::{
    CleanupAction, CronConfig, CronId, ResourceInfo, ScannerCondition, ScannerConfig, ScannerId,
    ScannerSource, SchedulingManager, SourceValue, WatcherCondition, WatcherConfig, WatcherId,
    WatcherSource,
};
use std::sync::RwLock;
use std::time::Duration;

/// Fake source fetcher for testing
struct FakeSourceFetcher {
    responses: RwLock<HashMap<String, SourceValue>>,
}

impl FakeSourceFetcher {
    fn new() -> Self {
        Self {
            responses: RwLock::new(HashMap::new()),
        }
    }

    fn set_response(&self, watcher_id: &str, value: SourceValue) {
        self.responses
            .write()
            .unwrap()
            .insert(watcher_id.to_string(), value);
    }
}

impl SourceFetcher for FakeSourceFetcher {
    fn fetch(
        &self,
        _source: &WatcherSource,
        context: &FetchContext,
    ) -> Result<SourceValue, FetchError> {
        let watcher_id = context
            .variables
            .get("watcher_id")
            .cloned()
            .unwrap_or_default();
        Ok(self
            .responses
            .read()
            .unwrap()
            .get(&watcher_id)
            .cloned()
            .unwrap_or(SourceValue::Numeric { value: 0 }))
    }
}

/// Fake resource scanner for testing
struct FakeResourceScanner {
    resources: RwLock<Vec<ResourceInfo>>,
}

impl FakeResourceScanner {
    fn new() -> Self {
        Self {
            resources: RwLock::new(vec![]),
        }
    }

    fn set_resources(&self, resources: Vec<ResourceInfo>) {
        *self.resources.write().unwrap() = resources;
    }
}

impl ResourceScanner for FakeResourceScanner {
    fn scan(&self, _source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        Ok(self.resources.read().unwrap().clone())
    }
}

#[test]
fn cron_tick_triggers_linked_watchers() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Create a cron with a linked watcher
    let watcher_id = WatcherId::new("idle-checker");
    let cron_config = CronConfig::new("watchdog", Duration::from_secs(30))
        .enabled()
        .with_watchers(vec![watcher_id.clone()]);

    let cron_id = CronId::new("watchdog");
    manager.add_cron(cron_id.clone(), cron_config, &clock);

    // Add the watcher
    let watcher_config = WatcherConfig::new(
        "idle-checker",
        WatcherSource::Session {
            name: "agent-1".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );
    manager.add_watcher(watcher_id.clone(), watcher_config, &clock);

    // Set up the fetcher to return an idle value that exceeds threshold
    let fetcher = FakeSourceFetcher::new();
    fetcher.set_response(
        "idle-checker",
        SourceValue::Idle {
            duration: Duration::from_secs(400),
        },
    );

    let scanner = NoOpResourceScanner;

    // Tick the cron
    let mut controller = CronController::new(&mut manager, &fetcher, &scanner, &clock);
    let effects = controller.on_cron_tick(&cron_id);

    // Should have cron triggered event and watcher triggered event
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == "watchdog")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::WatcherTriggered { id, .. }) if id == "idle-checker")
    ));
}

#[test]
fn cron_tick_triggers_linked_scanners() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Create a cron with a linked scanner
    let scanner_id = ScannerId::new("stale-locks");
    let cron_config = CronConfig::new("cleanup", Duration::from_secs(60))
        .enabled()
        .with_scanners(vec![scanner_id.clone()]);

    let cron_id = CronId::new("cleanup");
    manager.add_cron(cron_id.clone(), cron_config, &clock);

    // Add the scanner
    let scanner_config = ScannerConfig::new(
        "stale-locks",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(300),
        },
        CleanupAction::Release,
        Duration::from_secs(60),
    );
    manager.add_scanner(scanner_id.clone(), scanner_config, &clock);

    // Set up the resource scanner to return stale locks
    let fetcher = NoOpSourceFetcher;
    let resource_scanner = FakeResourceScanner::new();
    resource_scanner.set_resources(vec![
        ResourceInfo::new("lock:foo").with_age(Duration::from_secs(400))
    ]);

    // Tick the cron
    let mut controller = CronController::new(&mut manager, &fetcher, &resource_scanner, &clock);
    let effects = controller.on_cron_tick(&cron_id);

    // Should have cron triggered event, scanner started event, and cleanup effect
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == "cleanup")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerStarted { id }) if id == "stale-locks")));
    assert!(effects.iter().any(|e| matches!(e, Effect::Emit(Event::ScannerReleaseResource { resource_id, .. }) if resource_id == "lock:foo")));
}

#[test]
fn watcher_check_without_condition_met_schedules_next_check() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Add a watcher
    let watcher_id = WatcherId::new("idle-checker");
    let watcher_config = WatcherConfig::new(
        "idle-checker",
        WatcherSource::Session {
            name: "agent-1".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );
    manager.add_watcher(watcher_id.clone(), watcher_config, &clock);

    // Set up the fetcher to return an idle value below threshold
    let fetcher = FakeSourceFetcher::new();
    fetcher.set_response(
        "idle-checker",
        SourceValue::Idle {
            duration: Duration::from_secs(100),
        },
    );

    let scanner = NoOpResourceScanner;

    // Check the watcher
    let mut controller = CronController::new(&mut manager, &fetcher, &scanner, &clock);
    let effects = controller.check_watcher(&watcher_id);

    // Should schedule the next check timer, not trigger
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id.contains("idle-checker") && *duration == Duration::from_secs(60))));
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherTriggered { .. }))));
}

#[test]
fn scanner_with_no_matching_resources_goes_back_to_idle() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Add a scanner
    let scanner_id = ScannerId::new("stale-locks");
    let scanner_config = ScannerConfig::new(
        "stale-locks",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(300),
        },
        CleanupAction::Release,
        Duration::from_secs(60),
    );
    manager.add_scanner(scanner_id.clone(), scanner_config, &clock);

    // Set up the resource scanner to return no stale locks
    let fetcher = NoOpSourceFetcher;
    let resource_scanner = FakeResourceScanner::new();
    resource_scanner.set_resources(vec![
        ResourceInfo::new("lock:foo").with_age(Duration::from_secs(100)), // Not stale
    ]);

    // Run the scanner
    let mut controller = CronController::new(&mut manager, &fetcher, &resource_scanner, &clock);
    let effects = controller.run_scanner(&scanner_id);

    // Should have scanner started event and set timer for next scan
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerStarted { id }) if id == "stale-locks")));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. } if id.contains("stale-locks"))));
    // Should NOT have any cleanup effects
    assert!(!effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerReleaseResource { .. }))));
}

#[test]
fn cron_with_multiple_watchers_and_scanners() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Create watchers and scanners
    let watcher1_id = WatcherId::new("watcher-1");
    let watcher2_id = WatcherId::new("watcher-2");
    let scanner1_id = ScannerId::new("scanner-1");

    // Create cron with multiple linked items
    let cron_config = CronConfig::new("multi", Duration::from_secs(30))
        .enabled()
        .with_watchers(vec![watcher1_id.clone(), watcher2_id.clone()])
        .with_scanners(vec![scanner1_id.clone()]);

    let cron_id = CronId::new("multi");
    manager.add_cron(cron_id.clone(), cron_config, &clock);

    // Add watchers
    manager.add_watcher(
        watcher1_id.clone(),
        WatcherConfig::new(
            "watcher-1",
            WatcherSource::Session {
                name: "s1".to_string(),
            },
            WatcherCondition::Idle {
                threshold: Duration::from_secs(300),
            },
            Duration::from_secs(60),
        ),
        &clock,
    );
    manager.add_watcher(
        watcher2_id.clone(),
        WatcherConfig::new(
            "watcher-2",
            WatcherSource::Session {
                name: "s2".to_string(),
            },
            WatcherCondition::Idle {
                threshold: Duration::from_secs(300),
            },
            Duration::from_secs(60),
        ),
        &clock,
    );

    // Add scanner
    manager.add_scanner(
        scanner1_id.clone(),
        ScannerConfig::new(
            "scanner-1",
            ScannerSource::Locks,
            ScannerCondition::Stale {
                threshold: Duration::from_secs(300),
            },
            CleanupAction::Release,
            Duration::from_secs(60),
        ),
        &clock,
    );

    let fetcher = FakeSourceFetcher::new();
    let resource_scanner = FakeResourceScanner::new();

    // Tick the cron
    let mut controller = CronController::new(&mut manager, &fetcher, &resource_scanner, &clock);
    let effects = controller.on_cron_tick(&cron_id);

    // Should have effects from cron, both watchers, and scanner
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == "multi")));
    // Both watchers should set timers (not triggered because value is 0)
    let timer_count = effects
        .iter()
        .filter(|e| matches!(e, Effect::SetTimer { .. }))
        .count();
    assert!(
        timer_count >= 2,
        "Expected at least 2 timers, got {}",
        timer_count
    );
}

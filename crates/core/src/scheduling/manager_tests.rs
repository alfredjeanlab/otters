// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use crate::scheduling::{
    CleanupAction, ScannerCondition, ScannerSource, WatcherCondition, WatcherResponse,
    WatcherSource,
};
use std::time::Duration;

// ==================== Cron Tests ====================

#[test]
fn add_cron_disabled() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60));
    let effects = manager.add_cron(CronId::new("cron-1"), config, &clock);

    // No effects for disabled cron
    assert!(effects.is_empty());
    assert!(manager.get_cron(&CronId::new("cron-1")).is_some());
}

#[test]
fn add_cron_enabled() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    let effects = manager.add_cron(CronId::new("cron-1"), config, &clock);

    // Enabled cron schedules timer
    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], Effect::SetTimer { .. }));
}

#[test]
fn enable_cron() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60));
    manager.add_cron(CronId::new("cron-1"), config, &clock);

    let effects = manager.enable_cron(&CronId::new("cron-1"), &clock);

    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronEnabled { .. }))));

    let cron = manager.get_cron(&CronId::new("cron-1")).unwrap();
    assert_eq!(cron.state, CronState::Enabled);
}

#[test]
fn disable_cron() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    manager.add_cron(CronId::new("cron-1"), config, &clock);

    let effects = manager.disable_cron(&CronId::new("cron-1"), &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CancelTimer { .. })));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronDisabled { .. }))));

    let cron = manager.get_cron(&CronId::new("cron-1")).unwrap();
    assert_eq!(cron.state, CronState::Disabled);
}

#[test]
fn tick_cron() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    manager.add_cron(CronId::new("cron-1"), config, &clock);

    let effects = manager.tick_cron(&CronId::new("cron-1"), &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { .. }))));

    let cron = manager.get_cron(&CronId::new("cron-1")).unwrap();
    assert_eq!(cron.state, CronState::Running);
}

#[test]
fn complete_cron() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    manager.add_cron(CronId::new("cron-1"), config, &clock);
    manager.tick_cron(&CronId::new("cron-1"), &clock);

    let effects = manager.complete_cron(&CronId::new("cron-1"), &clock);

    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronCompleted { .. }))));

    let cron = manager.get_cron(&CronId::new("cron-1")).unwrap();
    assert_eq!(cron.state, CronState::Enabled);
    assert_eq!(cron.run_count, 1);
}

// ==================== Action Tests ====================

#[test]
fn add_action() {
    let mut manager = SchedulingManager::new();

    let config = ActionConfig::new("test-action", Duration::from_secs(30));
    manager.add_action(ActionId::new("action-1"), config);

    assert!(manager.get_action(&ActionId::new("action-1")).is_some());
    assert!(manager.can_trigger_action(&ActionId::new("action-1")));
}

#[test]
fn trigger_action() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ActionConfig::new("test-action", Duration::from_secs(30));
    manager.add_action(ActionId::new("action-1"), config);

    let effects = manager.trigger_action(&ActionId::new("action-1"), "test-source", &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { .. }))));

    assert!(!manager.can_trigger_action(&ActionId::new("action-1")));
}

#[test]
fn complete_action_enters_cooldown() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ActionConfig::new("test-action", Duration::from_secs(30));
    manager.add_action(ActionId::new("action-1"), config);

    manager.trigger_action(&ActionId::new("action-1"), "test", &clock);
    let effects = manager.complete_action(&ActionId::new("action-1"), &clock);

    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionCompleted { .. }))));

    let action = manager.get_action(&ActionId::new("action-1")).unwrap();
    assert!(action.is_on_cooldown());
}

#[test]
fn cooldown_expired_makes_action_ready() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ActionConfig::new("test-action", Duration::from_secs(30));
    manager.add_action(ActionId::new("action-1"), config);

    manager.trigger_action(&ActionId::new("action-1"), "test", &clock);
    manager.complete_action(&ActionId::new("action-1"), &clock);

    clock.advance(Duration::from_secs(31));
    let effects = manager.cooldown_expired(&ActionId::new("action-1"), &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionReady { .. }))));
    assert!(manager.can_trigger_action(&ActionId::new("action-1")));
}

// ==================== Watcher Tests ====================

#[test]
fn add_watcher_schedules_check() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );
    let effects = manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
    assert!(manager.get_watcher(&WatcherId::new("watcher-1")).is_some());
}

#[test]
fn check_watcher_condition_not_met() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );
    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    let effects = manager.check_watcher(
        &WatcherId::new("watcher-1"),
        SourceValue::Idle {
            duration: Duration::from_secs(100),
        },
        &clock,
    );

    // Should reschedule check
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));

    let watcher = manager.get_watcher(&WatcherId::new("watcher-1")).unwrap();
    assert_eq!(watcher.state, WatcherState::Active);
}

#[test]
fn check_watcher_condition_met_triggers() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("nudge")));

    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    let effects = manager.check_watcher(
        &WatcherId::new("watcher-1"),
        SourceValue::Idle {
            duration: Duration::from_secs(400),
        },
        &clock,
    );

    // Should emit trigger event and action trigger
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherTriggered { .. }))));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { .. }))));

    let watcher = manager.get_watcher(&WatcherId::new("watcher-1")).unwrap();
    assert!(matches!(watcher.state, WatcherState::Triggered { .. }));
}

#[test]
fn watcher_response_succeeded_returns_to_active() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("nudge")));

    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    // Trigger the watcher
    manager.check_watcher(
        &WatcherId::new("watcher-1"),
        SourceValue::Idle {
            duration: Duration::from_secs(400),
        },
        &clock,
    );

    // Mark response as succeeded
    let effects = manager.watcher_response_succeeded(&WatcherId::new("watcher-1"), &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::WatcherResolved { .. }))));

    let watcher = manager.get_watcher(&WatcherId::new("watcher-1")).unwrap();
    assert_eq!(watcher.state, WatcherState::Active);
}

#[test]
fn pause_and_resume_watcher() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );
    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    // Pause
    let effects = manager.pause_watcher(&WatcherId::new("watcher-1"), &clock);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CancelTimer { .. })));

    let watcher = manager.get_watcher(&WatcherId::new("watcher-1")).unwrap();
    assert_eq!(watcher.state, WatcherState::Paused);

    // Resume
    let effects = manager.resume_watcher(&WatcherId::new("watcher-1"), &clock);
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));

    let watcher = manager.get_watcher(&WatcherId::new("watcher-1")).unwrap();
    assert_eq!(watcher.state, WatcherState::Active);
}

// ==================== Scanner Tests ====================

#[test]
fn add_scanner_schedules_scan() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    let effects = manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);

    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));
    assert!(manager.get_scanner(&ScannerId::new("scanner-1")).is_some());
}

#[test]
fn tick_scanner_starts_scanning() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);

    let effects = manager.tick_scanner(&ScannerId::new("scanner-1"), &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerStarted { .. }))));

    let scanner = manager.get_scanner(&ScannerId::new("scanner-1")).unwrap();
    assert_eq!(scanner.state, ScannerState::Scanning);
}

#[test]
fn scan_complete_no_matches() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);
    manager.tick_scanner(&ScannerId::new("scanner-1"), &clock);

    // No stale resources
    let resources = vec![ResourceInfo::new("lock-1").with_age(Duration::from_secs(1800))];
    let effects = manager.scan_complete(&ScannerId::new("scanner-1"), resources, &clock);

    // Should reschedule
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));

    let scanner = manager.get_scanner(&ScannerId::new("scanner-1")).unwrap();
    assert_eq!(scanner.state, ScannerState::Idle);
}

#[test]
fn scan_complete_with_matches() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);
    manager.tick_scanner(&ScannerId::new("scanner-1"), &clock);

    // One stale resource
    let resources = vec![
        ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200)),
        ResourceInfo::new("lock-2").with_age(Duration::from_secs(1800)),
    ];
    let effects = manager.scan_complete(&ScannerId::new("scanner-1"), resources, &clock);

    // Should emit found and cleanup effects
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerFound { .. }))));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerReleaseResource { .. }))));

    let scanner = manager.get_scanner(&ScannerId::new("scanner-1")).unwrap();
    assert!(matches!(scanner.state, ScannerState::Cleaning { .. }));
}

#[test]
fn cleanup_complete_updates_stats() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);
    manager.tick_scanner(&ScannerId::new("scanner-1"), &clock);

    let resources = vec![ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200))];
    manager.scan_complete(&ScannerId::new("scanner-1"), resources, &clock);

    let effects = manager.cleanup_complete(&ScannerId::new("scanner-1"), 1, &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerCleaned { .. }))));
    assert!(effects.iter().any(|e| matches!(e, Effect::SetTimer { .. })));

    let scanner = manager.get_scanner(&ScannerId::new("scanner-1")).unwrap();
    assert_eq!(scanner.state, ScannerState::Idle);
    assert_eq!(scanner.total_cleaned, 1);
}

// ==================== Timer Processing Tests ====================

#[test]
fn process_cron_timer() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = CronConfig::new("test-cron", Duration::from_secs(60)).enabled();
    manager.add_cron(CronId::new("cron-1"), config, &clock);

    let effects = manager.process_timer("cron:cron-1", &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { .. }))));
}

#[test]
fn process_action_cooldown_timer() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ActionConfig::new("test-action", Duration::from_secs(30));
    manager.add_action(ActionId::new("action-1"), config);
    manager.trigger_action(&ActionId::new("action-1"), "test", &clock);
    manager.complete_action(&ActionId::new("action-1"), &clock);

    clock.advance(Duration::from_secs(31));
    let effects = manager.process_timer("action:action-1:cooldown", &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionReady { .. }))));
}

#[test]
fn process_scanner_timer() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = ScannerConfig::new(
        "test-scanner",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );
    manager.add_scanner(ScannerId::new("scanner-1"), config, &clock);

    let effects = manager.process_timer("scanner:scanner-1", &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerStarted { .. }))));
}

#[test]
fn process_watcher_response_timer() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    let config = WatcherConfig::new(
        "test-watcher",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_response(
        WatcherResponse::new(ActionId::new("nudge")).with_delay(Duration::from_secs(10)),
    );

    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    // Trigger (goes to waiting state because of delay)
    manager.check_watcher(
        &WatcherId::new("watcher-1"),
        SourceValue::Idle {
            duration: Duration::from_secs(400),
        },
        &clock,
    );

    let effects = manager.process_timer("watcher:watcher-1:response", &clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { .. }))));
}

// ==================== Stats Tests ====================

#[test]
fn stats_tracks_all_primitives() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Add crons
    manager.add_cron(
        CronId::new("cron-1"),
        CronConfig::new("c1", Duration::from_secs(60)).enabled(),
        &clock,
    );
    manager.add_cron(
        CronId::new("cron-2"),
        CronConfig::new("c2", Duration::from_secs(60)),
        &clock,
    );

    // Add actions
    manager.add_action(
        ActionId::new("action-1"),
        ActionConfig::new("a1", Duration::from_secs(30)),
    );
    manager.trigger_action(&ActionId::new("action-1"), "test", &clock);
    manager.complete_action(&ActionId::new("action-1"), &clock);

    // Add watcher
    manager.add_watcher(
        WatcherId::new("watcher-1"),
        WatcherConfig::new(
            "w1",
            WatcherSource::Session {
                name: "test".to_string(),
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
        ScannerId::new("scanner-1"),
        ScannerConfig::new(
            "s1",
            ScannerSource::Locks,
            ScannerCondition::Stale {
                threshold: Duration::from_secs(3600),
            },
            CleanupAction::Release,
            Duration::from_secs(600),
        ),
        &clock,
    );

    let stats = manager.stats();

    assert_eq!(stats.total_crons, 2);
    assert_eq!(stats.enabled_crons, 1);
    assert_eq!(stats.total_actions, 1);
    assert_eq!(stats.actions_on_cooldown, 1);
    assert_eq!(stats.total_watchers, 1);
    assert_eq!(stats.active_watchers, 1);
    assert_eq!(stats.total_scanners, 1);
}

#[test]
fn clear_removes_all() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    manager.add_cron(
        CronId::new("cron-1"),
        CronConfig::new("c1", Duration::from_secs(60)),
        &clock,
    );
    manager.add_action(
        ActionId::new("action-1"),
        ActionConfig::new("a1", Duration::from_secs(30)),
    );

    manager.clear();

    let stats = manager.stats();
    assert_eq!(stats.total_crons, 0);
    assert_eq!(stats.total_actions, 0);
}

// ==================== Integration Tests ====================

#[test]
fn watcher_and_action_integration() {
    let clock = FakeClock::new();
    let mut manager = SchedulingManager::new();

    // Add action with cooldown
    manager.add_action(
        ActionId::new("nudge"),
        ActionConfig::new("nudge", Duration::from_secs(30)),
    );

    // Add watcher using that action
    let config = WatcherConfig::new(
        "agent-idle",
        WatcherSource::Session {
            name: "test".to_string(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_response(WatcherResponse::new(ActionId::new("nudge")));

    manager.add_watcher(WatcherId::new("watcher-1"), config, &clock);

    // Trigger watcher
    let effects = manager.check_watcher(
        &WatcherId::new("watcher-1"),
        SourceValue::Idle {
            duration: Duration::from_secs(400),
        },
        &clock,
    );

    // Watcher should trigger action
    let action_triggered = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { id, .. }) if id == "nudge"));
    assert!(action_triggered);

    // Now trigger the action through the manager
    let effects = manager.trigger_action(&ActionId::new("nudge"), "watcher:agent-idle", &clock);

    // Action should be triggered
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionTriggered { .. }))));
}

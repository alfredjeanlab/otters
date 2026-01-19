// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::clock::FakeClock;
use crate::effect::{Effect, Event};
use std::time::Duration;

fn make_stale_lock_scanner() -> Scanner {
    let config = ScannerConfig::new(
        "stale-locks",
        ScannerSource::Locks,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(3600),
        },
        CleanupAction::Release,
        Duration::from_secs(600),
    );

    Scanner::new(ScannerId::new("test"), config)
}

fn make_dead_queue_scanner() -> Scanner {
    let config = ScannerConfig::new(
        "dead-queue-items",
        ScannerSource::Queue {
            name: "merge".to_string(),
        },
        ScannerCondition::ExceededAttempts { max: 3 },
        CleanupAction::DeadLetter,
        Duration::from_secs(300),
    );

    Scanner::new(ScannerId::new("dead-queue"), config)
}

fn make_orphan_scanner() -> Scanner {
    let config = ScannerConfig::new(
        "orphan-worktrees",
        ScannerSource::Worktrees,
        ScannerCondition::Orphaned,
        CleanupAction::Delete,
        Duration::from_secs(1800),
    );

    Scanner::new(ScannerId::new("orphans"), config)
}

#[test]
fn new_scanner_is_idle() {
    let scanner = make_stale_lock_scanner();

    assert!(matches!(scanner.state, ScannerState::Idle));
    assert!(scanner.last_scan.is_none());
    assert_eq!(scanner.total_cleaned, 0);
}

#[test]
fn tick_starts_scanning() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    let (scanner, effects) = scanner.transition(ScannerEvent::Tick, &clock);

    assert!(matches!(scanner.state, ScannerState::Scanning));

    // Should emit ScannerStarted
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ScannerStarted { id }) if id == "test")));
}

#[test]
fn scan_complete_no_matches_returns_to_idle() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    // Start scanning
    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    assert!(matches!(scanner.state, ScannerState::Scanning));

    // Scan completes with resources that don't match (age < threshold)
    let resources = vec![
        ResourceInfo::new("lock-1").with_age(Duration::from_secs(1800)),
        ResourceInfo::new("lock-2").with_age(Duration::from_secs(2400)),
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(scanner.state, ScannerState::Idle));
    assert!(scanner.last_scan.is_some());

    // Should reschedule scan
    assert_eq!(effects.len(), 1);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, duration }
        if id == "scanner:test" && *duration == Duration::from_secs(600))));
}

#[test]
fn scan_complete_with_matches_starts_cleanup() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    // Start scanning
    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    // Scan completes with one stale lock
    let resources = vec![
        ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200)), // Stale (> 3600s)
        ResourceInfo::new("lock-2").with_age(Duration::from_secs(1800)), // Fresh
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 1 }
    ));

    // Should emit release effect and ScannerFound event
    assert!(effects.len() >= 2);
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerReleaseResource { scanner_id, resource_id })
        if scanner_id == "test" && resource_id == "lock-1")
    ));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerFound { id, count })
        if id == "test" && *count == 1)
    ));
}

#[test]
fn cleanup_complete_updates_stats() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    // Get to cleaning state
    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    let resources = vec![
        ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200)),
        ResourceInfo::new("lock-2").with_age(Duration::from_secs(8000)),
    ];
    let (scanner, _) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);
    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 2 }
    ));

    // Cleanup completes
    let (scanner, effects) = scanner.transition(ScannerEvent::CleanupComplete { count: 2 }, &clock);

    assert!(matches!(scanner.state, ScannerState::Idle));
    assert_eq!(scanner.total_cleaned, 2);

    // Should reschedule and emit ScannerCleaned
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "scanner:test")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerCleaned { id, count, total })
        if id == "test" && *count == 2 && *total == 2)
    ));
}

#[test]
fn cleanup_failed_returns_to_idle() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    // Get to cleaning state
    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    let resources = vec![ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200))];
    let (scanner, _) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    // Cleanup fails
    let (scanner, effects) = scanner.transition(
        ScannerEvent::CleanupFailed {
            error: "permission denied".to_string(),
        },
        &clock,
    );

    assert!(matches!(scanner.state, ScannerState::Idle));
    assert_eq!(scanner.total_cleaned, 0); // Not incremented

    // Should reschedule and emit ScannerFailed
    assert_eq!(effects.len(), 2);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::SetTimer { id, .. }
        if id == "scanner:test")));
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerFailed { id, error })
        if id == "test" && error == "permission denied")
    ));
}

#[test]
fn exceeded_attempts_condition() {
    let clock = FakeClock::new();
    let scanner = make_dead_queue_scanner();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![
        ResourceInfo::new("item-1").with_attempts(5), // Exceeds max (3)
        ResourceInfo::new("item-2").with_attempts(2), // OK
        ResourceInfo::new("item-3").with_attempts(3), // At max, should match
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 2 }
    ));

    // Should dead letter item-1 and item-3
    let deadletter_count = effects
        .iter()
        .filter(|e| matches!(e, Effect::Emit(Event::ScannerDeadLetterResource { .. })))
        .count();
    assert_eq!(deadletter_count, 2);
}

#[test]
fn orphaned_condition() {
    let clock = FakeClock::new();
    let scanner = make_orphan_scanner();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![
        ResourceInfo::new("wt-1").with_parent("pipeline-1"), // Has parent
        ResourceInfo::new("wt-2").orphaned(),                // Orphaned
        ResourceInfo::new("wt-3").orphaned(),                // Orphaned
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 2 }
    ));

    // Should delete wt-2 and wt-3
    let delete_count = effects
        .iter()
        .filter(|e| matches!(e, Effect::Emit(Event::ScannerDeleteResource { .. })))
        .count();
    assert_eq!(delete_count, 2);
}

#[test]
fn terminal_for_condition() {
    let config = ScannerConfig::new(
        "old-pipelines",
        ScannerSource::Pipelines,
        ScannerCondition::TerminalFor {
            threshold: Duration::from_secs(86400), // 24 hours
        },
        CleanupAction::Archive {
            destination: ".oj/archive".to_string(),
        },
        Duration::from_secs(3600),
    );

    let scanner = Scanner::new(ScannerId::new("archive"), config);
    let clock = FakeClock::new();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![
        ResourceInfo::new("p-1")
            .with_state("done")
            .with_age(Duration::from_secs(90000)), // Terminal + old
        ResourceInfo::new("p-2")
            .with_state("running")
            .with_age(Duration::from_secs(90000)), // Not terminal
        ResourceInfo::new("p-3")
            .with_state("failed")
            .with_age(Duration::from_secs(3600)), // Terminal but too recent
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 1 }
    ));

    // Should archive only p-1
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerArchiveResource { scanner_id, resource_id, destination })
        if scanner_id == "archive" && resource_id == "p-1" && destination == ".oj/archive")
    ));
}

#[test]
fn matches_condition() {
    let config = ScannerConfig::new(
        "temp-worktrees",
        ScannerSource::Worktrees,
        ScannerCondition::Matches {
            pattern: "temp-".to_string(),
        },
        CleanupAction::Delete,
        Duration::from_secs(600),
    );

    let scanner = Scanner::new(ScannerId::new("temp"), config);
    let clock = FakeClock::new();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![
        ResourceInfo::new("wt-1"), // No metadata
        ResourceInfo::new("wt-2").with_metadata("branch", "temp-branch-123"),
        ResourceInfo::new("wt-3").with_metadata("branch", "feature-branch"),
    ];

    let (scanner, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    assert!(matches!(
        scanner.state,
        ScannerState::Cleaning { item_count: 1 }
    ));

    // Should delete only wt-2
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerDeleteResource { scanner_id, resource_id })
        if scanner_id == "temp" && resource_id == "wt-2")
    ));
}

#[test]
fn custom_action_triggers_action() {
    let config = ScannerConfig::new(
        "custom-scanner",
        ScannerSource::Sessions,
        ScannerCondition::Stale {
            threshold: Duration::from_secs(600),
        },
        CleanupAction::Custom {
            action_id: ActionId::new("custom-cleanup"),
        },
        Duration::from_secs(300),
    );

    let scanner = Scanner::new(ScannerId::new("custom"), config);
    let clock = FakeClock::new();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![ResourceInfo::new("session-1").with_age(Duration::from_secs(1000))];

    let (_, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    // Should trigger custom action
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ActionTriggered { id, source })
        if id == "custom-cleanup" && source.contains("scanner:custom-scanner:session-1"))
    ));
}

#[test]
fn fail_cleanup_action() {
    let config = ScannerConfig::new(
        "failed-items",
        ScannerSource::Queue {
            name: "test".to_string(),
        },
        ScannerCondition::Stale {
            threshold: Duration::from_secs(60),
        },
        CleanupAction::Fail {
            reason: "timed out".to_string(),
        },
        Duration::from_secs(30),
    );

    let scanner = Scanner::new(ScannerId::new("fail"), config);
    let clock = FakeClock::new();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);

    let resources = vec![ResourceInfo::new("item-1").with_age(Duration::from_secs(120))];

    let (_, effects) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);

    // Should emit fail effect
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Emit(Event::ScannerFailResource { scanner_id, resource_id, reason })
        if scanner_id == "fail" && resource_id == "item-1" && reason == "timed out")
    ));
}

#[test]
fn multiple_cleanup_cycles_accumulate_stats() {
    let clock = FakeClock::new();
    let mut scanner = make_stale_lock_scanner();

    // First cycle
    (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    let resources = vec![ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200))];
    (scanner, _) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);
    (scanner, _) = scanner.transition(ScannerEvent::CleanupComplete { count: 1 }, &clock);
    assert_eq!(scanner.total_cleaned, 1);

    // Second cycle
    (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    let resources = vec![
        ResourceInfo::new("lock-2").with_age(Duration::from_secs(7200)),
        ResourceInfo::new("lock-3").with_age(Duration::from_secs(7200)),
    ];
    (scanner, _) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);
    (scanner, _) = scanner.transition(ScannerEvent::CleanupComplete { count: 2 }, &clock);
    assert_eq!(scanner.total_cleaned, 3);
}

#[test]
fn tick_while_scanning_is_noop() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    assert!(matches!(scanner.state, ScannerState::Scanning));

    // Another tick should be ignored
    let (new_scanner, effects) = scanner.transition(ScannerEvent::Tick, &clock);

    assert!(matches!(new_scanner.state, ScannerState::Scanning));
    assert!(effects.is_empty());
}

#[test]
fn scanner_state_display() {
    assert_eq!(ScannerState::Idle.to_string(), "idle");
    assert_eq!(ScannerState::Scanning.to_string(), "scanning");
    assert_eq!(
        ScannerState::Cleaning { item_count: 5 }.to_string(),
        "cleaning:5"
    );
}

#[test]
fn scanner_state_from_string() {
    assert!(matches!(
        ScannerState::from_string("idle"),
        ScannerState::Idle
    ));
    assert!(matches!(
        ScannerState::from_string("scanning"),
        ScannerState::Scanning
    ));
    assert!(matches!(
        ScannerState::from_string("cleaning:5"),
        ScannerState::Cleaning { item_count: 5 }
    ));
}

#[test]
fn scanner_id_conversions() {
    let id = ScannerId::new("test");
    assert_eq!(id.to_string(), "test");

    let id: ScannerId = "test".into();
    assert_eq!(id.0, "test");

    let id: ScannerId = "test".to_string().into();
    assert_eq!(id.0, "test");
}

#[test]
fn timer_id_format() {
    let scanner = make_stale_lock_scanner();
    assert_eq!(scanner.timer_id(), "scanner:test");
}

#[test]
fn is_active() {
    let clock = FakeClock::new();
    let scanner = make_stale_lock_scanner();

    assert!(!scanner.is_active());

    let (scanner, _) = scanner.transition(ScannerEvent::Tick, &clock);
    assert!(scanner.is_active());

    let resources = vec![ResourceInfo::new("lock-1").with_age(Duration::from_secs(7200))];
    let (scanner, _) = scanner.transition(ScannerEvent::ScanComplete { resources }, &clock);
    assert!(scanner.is_active()); // Cleaning is also active

    let (scanner, _) = scanner.transition(ScannerEvent::CleanupComplete { count: 1 }, &clock);
    assert!(!scanner.is_active());
}

#[test]
fn resource_info_builder() {
    let info = ResourceInfo::new("test-id")
        .with_age(Duration::from_secs(60))
        .with_state("running")
        .with_attempts(3)
        .with_parent("parent-1");

    assert_eq!(info.id, "test-id");
    assert_eq!(info.age, Some(Duration::from_secs(60)));
    assert_eq!(info.state.as_deref(), Some("running"));
    assert_eq!(info.attempts, Some(3));
    assert_eq!(info.parent_id.as_deref(), Some("parent-1"));

    let orphaned = info.orphaned();
    assert!(orphaned.parent_id.is_none());
}

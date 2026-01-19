// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::scheduling::WatcherId;

#[test]
fn register_watcher_with_patterns() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(watcher_id.clone(), vec!["task:failed".to_string()]);

    assert_eq!(bridge.watcher_count(), 1);
    assert!(!bridge.is_empty());
}

#[test]
fn exact_match_pattern() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(watcher_id.clone(), vec!["task:failed".to_string()]);

    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 1);
    assert_eq!(watchers[0], watcher_id);

    // Should not match different event
    let watchers = bridge.watchers_for_event("task:started");
    assert!(watchers.is_empty());
}

#[test]
fn prefix_match_pattern() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(watcher_id.clone(), vec!["task:*".to_string()]);

    // Should match any task event
    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 1);

    let watchers = bridge.watchers_for_event("task:started");
    assert_eq!(watchers.len(), 1);

    let watchers = bridge.watchers_for_event("task:completed");
    assert_eq!(watchers.len(), 1);

    // Should not match non-task events
    let watchers = bridge.watchers_for_event("session:idle");
    assert!(watchers.is_empty());
}

#[test]
fn wildcard_matches_everything() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(watcher_id.clone(), vec!["*".to_string()]);

    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 1);

    let watchers = bridge.watchers_for_event("session:idle");
    assert_eq!(watchers.len(), 1);

    let watchers = bridge.watchers_for_event("any:event:name");
    assert_eq!(watchers.len(), 1);
}

#[test]
fn multiple_patterns_for_watcher() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(
        watcher_id.clone(),
        vec!["task:failed".to_string(), "task:stuck".to_string()],
    );

    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 1);

    let watchers = bridge.watchers_for_event("task:stuck");
    assert_eq!(watchers.len(), 1);

    // Should not match other task events
    let watchers = bridge.watchers_for_event("task:started");
    assert!(watchers.is_empty());
}

#[test]
fn multiple_watchers_for_same_event() {
    let mut bridge = WatcherEventBridge::new();
    let watcher1 = WatcherId::new("watcher-1");
    let watcher2 = WatcherId::new("watcher-2");

    bridge.register(watcher1.clone(), vec!["task:failed".to_string()]);
    bridge.register(watcher2.clone(), vec!["task:failed".to_string()]);

    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 2);
    assert!(watchers.contains(&watcher1));
    assert!(watchers.contains(&watcher2));
}

#[test]
fn unregister_removes_watcher() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    bridge.register(
        watcher_id.clone(),
        vec!["task:failed".to_string(), "task:stuck".to_string()],
    );

    assert_eq!(bridge.watcher_count(), 1);

    bridge.unregister(&watcher_id);

    assert_eq!(bridge.watcher_count(), 0);
    assert!(bridge.is_empty());

    // Should no longer match
    let watchers = bridge.watchers_for_event("task:failed");
    assert!(watchers.is_empty());
}

#[test]
fn unregister_leaves_other_watchers() {
    let mut bridge = WatcherEventBridge::new();
    let watcher1 = WatcherId::new("watcher-1");
    let watcher2 = WatcherId::new("watcher-2");

    bridge.register(watcher1.clone(), vec!["task:failed".to_string()]);
    bridge.register(watcher2.clone(), vec!["task:failed".to_string()]);

    bridge.unregister(&watcher1);

    let watchers = bridge.watchers_for_event("task:failed");
    assert_eq!(watchers.len(), 1);
    assert_eq!(watchers[0], watcher2);
}

#[test]
fn no_duplicate_watchers_with_overlapping_patterns() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("test-watcher");

    // Both patterns match "task:failed"
    bridge.register(
        watcher_id.clone(),
        vec!["task:*".to_string(), "task:failed".to_string()],
    );

    let watchers = bridge.watchers_for_event("task:failed");
    // Should only appear once, not twice
    assert_eq!(watchers.len(), 1);
}

#[test]
fn event_pattern_matches() {
    // Exact match
    let pattern = EventPattern::new("task:failed");
    assert!(pattern.matches("task:failed"));
    assert!(!pattern.matches("task:started"));

    // Prefix match
    let pattern = EventPattern::new("task:*");
    assert!(pattern.matches("task:failed"));
    assert!(pattern.matches("task:started"));
    assert!(pattern.matches("task:"));
    assert!(!pattern.matches("session:idle"));

    // Wildcard
    let pattern = EventPattern::new("*");
    assert!(pattern.matches("anything"));
    assert!(pattern.matches(""));
}

#[test]
fn unregister_nonexistent_watcher_is_noop() {
    let mut bridge = WatcherEventBridge::new();
    let watcher_id = WatcherId::new("nonexistent");

    // Should not panic
    bridge.unregister(&watcher_id);

    assert!(bridge.is_empty());
}

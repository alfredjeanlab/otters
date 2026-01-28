// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn scheduler_timer_lifecycle() {
    let mut scheduler = Scheduler::new();
    let now = Instant::now();

    scheduler.set_timer("test".to_string(), Duration::from_secs(10), now);
    assert!(scheduler.has_timers());
    assert!(scheduler.next_deadline().is_some());

    // Timer hasn't fired yet
    let events = scheduler.fired_timers(now + Duration::from_secs(5));
    assert!(events.is_empty());
    assert!(scheduler.has_timers());

    // Timer fires
    let events = scheduler.fired_timers(now + Duration::from_secs(15));
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], Event::Timer { ref id } if id == "test"));
    assert!(!scheduler.has_timers());
}

#[test]
fn scheduler_cancel_timer() {
    let mut scheduler = Scheduler::new();
    let now = Instant::now();

    scheduler.set_timer("test".to_string(), Duration::from_secs(10), now);
    scheduler.cancel_timer("test");

    let events = scheduler.fired_timers(now + Duration::from_secs(15));
    assert!(events.is_empty());
}

use super::*;
use crate::clock::FakeClock;

#[test]
fn scheduler_fires_items_at_correct_time() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    let now = clock.now();
    scheduler.schedule(
        "item-1",
        now + Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );
    scheduler.schedule(
        "item-2",
        now + Duration::from_secs(5),
        ScheduledKind::TaskTick,
    );

    // Nothing ready yet
    let ready = scheduler.poll(now);
    assert!(ready.is_empty());

    // Advance 5 seconds - item-2 should be ready
    clock.advance(Duration::from_secs(5));
    let ready = scheduler.poll(clock.now());
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "item-2");

    // Advance 5 more seconds - item-1 should be ready
    clock.advance(Duration::from_secs(5));
    let ready = scheduler.poll(clock.now());
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "item-1");
}

#[test]
fn scheduler_repeating_timers_reschedule() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    let now = clock.now();
    scheduler.schedule_repeating(
        "repeat",
        now + Duration::from_secs(10),
        Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );

    // Fire first time
    clock.advance(Duration::from_secs(10));
    let ready = scheduler.poll(clock.now());
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "repeat");

    // Should have rescheduled
    assert!(!scheduler.is_empty());

    // Fire second time
    clock.advance(Duration::from_secs(10));
    let ready = scheduler.poll(clock.now());
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "repeat");
}

#[test]
fn scheduler_cancel_prevents_firing() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    let now = clock.now();
    scheduler.schedule(
        "item-1",
        now + Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );

    // Cancel it
    scheduler.cancel("item-1");

    // Advance past fire time
    clock.advance(Duration::from_secs(15));
    let ready = scheduler.poll(clock.now());
    assert!(ready.is_empty());
}

#[test]
fn scheduler_init_defaults_creates_timers() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    scheduler.init_defaults(&clock);

    assert!(!scheduler.is_empty());

    // Should have next fire time
    assert!(scheduler.next_fire_time().is_some());
}

#[test]
fn scheduler_multiple_items_fire_in_order() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    let now = clock.now();
    scheduler.schedule("a", now + Duration::from_secs(30), ScheduledKind::TaskTick);
    scheduler.schedule("b", now + Duration::from_secs(10), ScheduledKind::TaskTick);
    scheduler.schedule("c", now + Duration::from_secs(20), ScheduledKind::TaskTick);

    // Advance past all
    clock.advance(Duration::from_secs(35));
    let ready = scheduler.poll(clock.now());

    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0].id, "b");
    assert_eq!(ready[1].id, "c");
    assert_eq!(ready[2].id, "a");
}

#[test]
fn scheduler_cancel_repeating_stops_future_fires() {
    let clock = FakeClock::new();
    let mut scheduler = Scheduler::new();

    let now = clock.now();
    scheduler.schedule_repeating(
        "repeat",
        now + Duration::from_secs(10),
        Duration::from_secs(10),
        ScheduledKind::TaskTick,
    );

    // Fire first time
    clock.advance(Duration::from_secs(10));
    let ready = scheduler.poll(clock.now());
    assert_eq!(ready.len(), 1);

    // Cancel before next fire
    scheduler.cancel("repeat");

    // Advance - should not fire
    clock.advance(Duration::from_secs(10));
    let ready = scheduler.poll(clock.now());
    assert!(ready.is_empty());
}

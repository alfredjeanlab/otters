use super::*;
use crate::clock::FakeClock;

fn test_config() -> LockConfig {
    LockConfig::new("test-lock")
        .with_stale_threshold(Duration::from_secs(60))
        .with_heartbeat_interval(Duration::from_secs(15))
}

#[test]
fn new_lock_is_free() {
    let lock = Lock::new(test_config());
    assert!(lock.is_free());
    assert!(lock.holder().is_none());
}

#[test]
fn acquire_free_lock_succeeds() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder = HolderId::new("holder-1");

    let (new_lock, effects) = lock.transition(
        LockInput::Acquire {
            holder: holder.clone(),
            metadata: Some("test".to_string()),
        },
        &clock,
    );

    assert!(!new_lock.is_free());
    assert!(new_lock.is_held_by(&holder));
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockAcquired { name, holder: h, .. })
        if name == "test-lock" && h == "holder-1"
    ));
}

#[test]
fn acquire_held_lock_fails() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder1 = HolderId::new("holder-1");
    let holder2 = HolderId::new("holder-2");

    // First holder acquires
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder1.clone(),
            metadata: None,
        },
        &clock,
    );

    // Second holder tries to acquire
    let (new_lock, effects) = lock.transition(
        LockInput::Acquire {
            holder: holder2.clone(),
            metadata: None,
        },
        &clock,
    );

    // Lock still held by holder1
    assert!(new_lock.is_held_by(&holder1));
    assert!(!new_lock.is_held_by(&holder2));
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockDenied { holder, current_holder, .. })
        if holder == "holder-2" && current_holder == "holder-1"
    ));
}

#[test]
fn release_lock_succeeds() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder = HolderId::new("holder-1");

    // Acquire
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder.clone(),
            metadata: None,
        },
        &clock,
    );

    // Release
    let (new_lock, effects) = lock.transition(
        LockInput::Release {
            holder: holder.clone(),
        },
        &clock,
    );

    assert!(new_lock.is_free());
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockReleased { name, holder: h })
        if name == "test-lock" && h == "holder-1"
    ));
}

#[test]
fn release_by_wrong_holder_is_noop() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder1 = HolderId::new("holder-1");
    let holder2 = HolderId::new("holder-2");

    // Holder1 acquires
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder1.clone(),
            metadata: None,
        },
        &clock,
    );

    // Holder2 tries to release
    let (new_lock, effects) = lock.transition(LockInput::Release { holder: holder2 }, &clock);

    // Lock still held by holder1
    assert!(new_lock.is_held_by(&holder1));
    assert!(effects.is_empty());
}

#[test]
fn heartbeat_refreshes_timestamp() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder = HolderId::new("holder-1");

    // Acquire
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder.clone(),
            metadata: None,
        },
        &clock,
    );

    // Advance time
    clock.advance(Duration::from_secs(30));

    // Send heartbeat
    let (new_lock, _) = lock.transition(LockInput::Heartbeat { holder }, &clock);

    // Lock should not be stale
    assert!(!new_lock.is_stale(&clock));
}

#[test]
fn stale_lock_can_be_reclaimed() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder1 = HolderId::new("holder-1");
    let holder2 = HolderId::new("holder-2");

    // Holder1 acquires
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder1.clone(),
            metadata: None,
        },
        &clock,
    );

    // Advance time beyond stale threshold
    clock.advance(Duration::from_secs(120));

    // Lock should be stale
    assert!(lock.is_stale(&clock));

    // Holder2 can reclaim
    let (new_lock, effects) = lock.transition(
        LockInput::Acquire {
            holder: holder2.clone(),
            metadata: None,
        },
        &clock,
    );

    assert!(new_lock.is_held_by(&holder2));
    assert_eq!(effects.len(), 2);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockReclaimed { previous_holder, new_holder, .. })
        if previous_holder == "holder-1" && new_holder == "holder-2"
    ));
}

#[test]
fn tick_emits_stale_warning() {
    let lock = Lock::new(test_config());
    let clock = FakeClock::new();
    let holder = HolderId::new("holder-1");

    // Acquire
    let (lock, _) = lock.transition(
        LockInput::Acquire {
            holder: holder.clone(),
            metadata: None,
        },
        &clock,
    );

    // Advance time beyond stale threshold
    clock.advance(Duration::from_secs(120));

    // Tick should emit stale warning
    let (_, effects) = lock.transition(LockInput::Tick, &clock);

    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockStale { name, holder: h })
        if name == "test-lock" && h == "holder-1"
    ));
}

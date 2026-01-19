use super::*;
use crate::clock::FakeClock;

fn test_config() -> SemaphoreConfig {
    SemaphoreConfig::new("test-semaphore", 3).with_stale_threshold(Duration::from_secs(60))
}

#[test]
fn new_semaphore_has_full_capacity() {
    let sem = Semaphore::new(test_config());
    assert_eq!(sem.available_slots(), 3);
    assert_eq!(sem.used_slots(), 0);
}

#[test]
fn acquire_succeeds_with_available_slots() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    let (new_sem, effects) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 1,
            metadata: Some("test".to_string()),
        },
        &clock,
    );

    assert_eq!(new_sem.used_slots(), 1);
    assert_eq!(new_sem.available_slots(), 2);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreAcquired { name, holder_id, weight, .. })
        if name == "test-semaphore" && holder_id == "holder-1" && *weight == 1
    ));
}

#[test]
fn acquire_fails_with_insufficient_slots() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    let (new_sem, effects) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 4, // More than max_slots
            metadata: None,
        },
        &clock,
    );

    assert_eq!(new_sem.used_slots(), 0);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreDenied { requested, available, .. })
        if *requested == 4 && *available == 3
    ));
}

#[test]
fn weighted_acquisition_respects_capacity() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // First holder takes 2 slots
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 2,
            metadata: None,
        },
        &clock,
    );

    // Second holder wants 2 slots, but only 1 available
    let (new_sem, effects) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-2".to_string(),
            weight: 2,
            metadata: None,
        },
        &clock,
    );

    assert_eq!(new_sem.used_slots(), 2); // Still just holder-1
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreDenied { .. })
    ));
}

#[test]
fn multiple_holders_can_acquire() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // First holder takes 1 slot
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 1,
            metadata: None,
        },
        &clock,
    );

    // Second holder takes 1 slot
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-2".to_string(),
            weight: 1,
            metadata: None,
        },
        &clock,
    );

    // Third holder takes 1 slot
    let (new_sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-3".to_string(),
            weight: 1,
            metadata: None,
        },
        &clock,
    );

    assert_eq!(new_sem.used_slots(), 3);
    assert_eq!(new_sem.holders.len(), 3);
}

#[test]
fn release_frees_slots() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // Acquire
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 2,
            metadata: None,
        },
        &clock,
    );

    // Release
    let (new_sem, effects) = sem.transition(
        SemaphoreInput::Release {
            holder_id: "holder-1".to_string(),
        },
        &clock,
    );

    assert_eq!(new_sem.available_slots(), 3);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreReleased { weight, available, .. })
        if *weight == 2 && *available == 3
    ));
}

#[test]
fn heartbeat_refreshes_timestamp() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // Acquire
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 1,
            metadata: None,
        },
        &clock,
    );

    // Advance time
    clock.advance(Duration::from_secs(30));

    // Send heartbeat
    let (new_sem, _) = sem.transition(
        SemaphoreInput::Heartbeat {
            holder_id: "holder-1".to_string(),
        },
        &clock,
    );

    // Holder should not be stale
    assert!(!new_sem.is_holder_stale("holder-1", &clock));
}

#[test]
fn stale_holders_reclaimed_on_acquire() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // Holder 1 acquires all slots
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 3,
            metadata: None,
        },
        &clock,
    );

    // Advance time beyond stale threshold
    clock.advance(Duration::from_secs(120));

    // Holder 2 tries to acquire - should succeed after reclaim
    let (new_sem, effects) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-2".to_string(),
            weight: 2,
            metadata: None,
        },
        &clock,
    );

    assert_eq!(new_sem.holders.len(), 1);
    assert!(new_sem.holders.contains_key("holder-2"));
    assert_eq!(effects.len(), 2); // Reclaim + Acquire
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreReclaimed { holder_id, .. })
        if holder_id == "holder-1"
    ));
}

#[test]
fn tick_emits_stale_warnings() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // Acquire
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 1,
            metadata: None,
        },
        &clock,
    );

    // Advance time beyond stale threshold
    clock.advance(Duration::from_secs(120));

    // Tick should emit stale warning
    let (_, effects) = sem.transition(SemaphoreInput::Tick, &clock);

    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreHolderStale { holder_id, .. })
        if holder_id == "holder-1"
    ));
}

#[test]
fn used_slots_never_exceeds_max() {
    let sem = Semaphore::new(test_config());
    let clock = FakeClock::new();

    // Try to acquire more than max
    let (sem, _) = sem.transition(
        SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 10,
            metadata: None,
        },
        &clock,
    );

    // Should be denied, used_slots still 0
    assert_eq!(sem.used_slots(), 0);
    assert!(sem.used_slots() <= sem.config.max_slots);
}

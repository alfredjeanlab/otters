use super::*;
use crate::clock::FakeClock;

#[test]
fn storable_lock_roundtrip_free() {
    let clock = FakeClock::new();
    let lock = Lock::new(LockConfig::new("test-lock"));

    let storable = StorableLock::from_lock(&lock, &clock);
    let restored = storable.to_lock(&clock);

    assert!(restored.is_free());
    assert_eq!(restored.config.name, "test-lock");
}

#[test]
fn storable_lock_roundtrip_held() {
    let clock = FakeClock::new();
    let mut lock = Lock::new(LockConfig::new("test-lock"));
    let (new_lock, _) = lock.transition(
        super::super::lock::LockInput::Acquire {
            holder: HolderId::new("holder-1"),
            metadata: Some("test-metadata".to_string()),
        },
        &clock,
    );
    lock = new_lock;

    // Advance time a bit
    clock.advance(Duration::from_secs(10));

    let storable = StorableLock::from_lock(&lock, &clock);
    let restored = storable.to_lock(&clock);

    assert!(!restored.is_free());
    assert_eq!(restored.holder().unwrap().0, "holder-1");
}

#[test]
fn storable_semaphore_roundtrip() {
    let clock = FakeClock::new();
    let mut sem = Semaphore::new(SemaphoreConfig::new("test-sem", 5));
    let (new_sem, _) = sem.transition(
        super::super::semaphore::SemaphoreInput::Acquire {
            holder_id: "holder-1".to_string(),
            weight: 2,
            metadata: Some("test".to_string()),
        },
        &clock,
    );
    sem = new_sem;

    clock.advance(Duration::from_secs(5));

    let storable = StorableSemaphore::from_semaphore(&sem, &clock);
    let restored = storable.to_semaphore(&clock);

    assert_eq!(restored.used_slots(), 2);
    assert!(restored.holders.contains_key("holder-1"));
}

#[test]
fn storable_coordination_state_roundtrip() {
    let clock = FakeClock::new();
    let mut manager = CoordinationManager::new();

    manager.ensure_lock(LockConfig::new("lock-1"));
    manager.acquire_lock(
        "lock-1",
        HolderId::new("holder-1"),
        Some("metadata".to_string()),
        &clock,
    );

    manager.ensure_semaphore(SemaphoreConfig::new("sem-1", 10));
    manager.acquire_semaphore("sem-1", "holder-2".to_string(), 3, None, &clock);

    let storable = StorableCoordinationState::from_manager(&manager, &clock);

    // Verify serialization
    let json = serde_json::to_string_pretty(&storable).unwrap();
    assert!(json.contains("lock-1"));
    assert!(json.contains("sem-1"));

    // Verify deserialization
    let deserialized: StorableCoordinationState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.locks.len(), 1);
    assert_eq!(deserialized.semaphores.len(), 1);

    // Verify restoration
    let restored_manager = deserialized.to_manager();
    assert!(restored_manager.get_lock("lock-1").is_some());
    assert!(restored_manager.get_semaphore("sem-1").is_some());
}

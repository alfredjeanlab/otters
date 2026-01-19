use super::*;
use crate::clock::FakeClock;
use crate::effect::Event;
use std::time::Duration;

#[test]
fn new_manager_is_empty() {
    let manager = CoordinationManager::new();
    assert!(manager.lock_names().is_empty());
    assert!(manager.semaphore_names().is_empty());
}

#[test]
fn acquire_lock_creates_and_acquires() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();
    let holder = HolderId::new("test-holder");

    let (acquired, effects) = manager.acquire_lock("test-lock", holder.clone(), None, &clock);

    assert!(acquired);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockAcquired { name, .. }) if name == "test-lock"
    ));
    assert!(manager.get_lock("test-lock").is_some());
}

#[test]
fn release_lock_frees_lock() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();
    let holder = HolderId::new("test-holder");

    manager.acquire_lock("test-lock", holder.clone(), None, &clock);
    let effects = manager.release_lock("test-lock", holder, &clock);

    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::LockReleased { .. })
    ));
    assert!(manager.get_lock("test-lock").unwrap().is_free());
}

#[test]
fn acquire_semaphore_creates_and_acquires() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    // First, ensure a semaphore exists with proper capacity
    manager.ensure_semaphore(SemaphoreConfig::new("test-sem", 5));

    let (acquired, effects) =
        manager.acquire_semaphore("test-sem", "holder-1".to_string(), 2, None, &clock);

    assert!(acquired);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreAcquired { name, weight, .. })
        if name == "test-sem" && *weight == 2
    ));
}

#[test]
fn release_semaphore_frees_slots() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.ensure_semaphore(SemaphoreConfig::new("test-sem", 5));
    manager.acquire_semaphore("test-sem", "holder-1".to_string(), 2, None, &clock);

    let effects = manager.release_semaphore("test-sem", "holder-1".to_string(), &clock);

    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::SemaphoreReleased { weight, .. }) if *weight == 2
    ));
}

#[test]
fn build_coordination_inputs_includes_locks() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.ensure_lock(LockConfig::new("free-lock"));
    manager.acquire_lock("held-lock", HolderId::new("holder-1"), None, &clock);

    let inputs = manager.build_coordination_inputs();

    assert_eq!(inputs.locks.get("free-lock"), Some(&true));
    assert_eq!(inputs.locks.get("held-lock"), Some(&false));
    assert_eq!(
        inputs.lock_holders.get("held-lock"),
        Some(&"holder-1".to_string())
    );
}

#[test]
fn build_coordination_inputs_includes_semaphores() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.ensure_semaphore(SemaphoreConfig::new("test-sem", 5));
    manager.acquire_semaphore("test-sem", "holder-1".to_string(), 2, None, &clock);

    let inputs = manager.build_coordination_inputs();

    assert_eq!(inputs.semaphores.get("test-sem"), Some(&3));
}

#[test]
fn evaluate_guard_uses_coordination_state() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.ensure_lock(LockConfig::new("test-lock"));

    let guard = GuardCondition::lock_free("test-lock");
    assert_eq!(manager.evaluate_guard(&guard), GuardResult::Passed);

    manager.acquire_lock("test-lock", HolderId::new("holder-1"), None, &clock);
    assert!(matches!(
        manager.evaluate_guard(&guard),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn register_and_get_guard() {
    let mut manager = CoordinationManager::new();

    let guard = RegisteredGuard::new("guard-1", GuardCondition::lock_free("test-lock"))
        .with_wake_on(vec!["lock:released".to_string()]);

    manager.register_guard(guard);

    let retrieved = manager.get_guard("guard-1");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().wake_on, vec!["lock:released"]);
}

#[test]
fn guards_for_event_matches_patterns() {
    let mut manager = CoordinationManager::new();

    manager.register_guard(
        RegisteredGuard::new("guard-1", GuardCondition::lock_free("lock-1"))
            .with_wake_on(vec!["lock:released".to_string()]),
    );
    manager.register_guard(
        RegisteredGuard::new("guard-2", GuardCondition::lock_free("lock-2"))
            .with_wake_on(vec!["lock:".to_string()]), // Prefix match
    );
    manager.register_guard(
        RegisteredGuard::new("guard-3", GuardCondition::lock_free("lock-3"))
            .with_wake_on(vec!["semaphore:released".to_string()]),
    );

    let guards = manager.guards_for_event("lock:released");
    assert_eq!(guards.len(), 2);

    let guard_ids: Vec<_> = guards.iter().map(|g| g.id.as_str()).collect();
    assert!(guard_ids.contains(&"guard-1"));
    assert!(guard_ids.contains(&"guard-2"));
}

#[test]
fn tick_emits_stale_warnings() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.acquire_lock("test-lock", HolderId::new("holder-1"), None, &clock);
    clock.advance(Duration::from_secs(120));

    let effects = manager.tick(&clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::LockStale { .. }))));
}

#[test]
fn reclaim_stale_releases_resources() {
    let mut manager = CoordinationManager::new();
    let clock = FakeClock::new();

    manager.acquire_lock("test-lock", HolderId::new("holder-1"), None, &clock);
    clock.advance(Duration::from_secs(120));

    let effects = manager.reclaim_stale(&clock);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::LockReleased { .. }))));
    assert!(manager.get_lock("test-lock").unwrap().is_free());
}

#[test]
fn event_matches_pattern_exact() {
    assert!(event_matches_pattern("lock:acquired", "lock:acquired"));
    assert!(!event_matches_pattern("lock:released", "lock:acquired"));
}

#[test]
fn event_matches_pattern_prefix() {
    assert!(event_matches_pattern("lock:acquired", "lock:"));
    assert!(event_matches_pattern("lock:released", "lock:"));
    assert!(!event_matches_pattern("semaphore:acquired", "lock:"));
}

#[test]
fn event_matches_pattern_wildcard() {
    assert!(event_matches_pattern("lock:acquired", "*"));
    assert!(event_matches_pattern("semaphore:released", "*"));
}

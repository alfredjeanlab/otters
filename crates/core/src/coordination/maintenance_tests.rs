use super::*;
use crate::clock::FakeClock;
use crate::coordination::lock::{HolderId, LockConfig};
use crate::coordination::semaphore::SemaphoreConfig;
use crate::effect::Event;

#[test]
fn maintenance_config_default() {
    let config = MaintenanceConfig::default();
    assert_eq!(config.interval, Duration::from_secs(30));
    assert!(config.reclaim_stale);
    assert!(config.emit_warnings);
}

#[test]
fn maintenance_config_builder() {
    let config = MaintenanceConfig::new()
        .with_interval(Duration::from_secs(60))
        .with_reclaim_stale(false)
        .with_emit_warnings(true);

    assert_eq!(config.interval, Duration::from_secs(60));
    assert!(!config.reclaim_stale);
    assert!(config.emit_warnings);
}

#[test]
fn maintenance_tick_emits_warnings() {
    let clock = FakeClock::new();
    let mut manager = CoordinationManager::new();

    manager.ensure_lock(LockConfig::new("test-lock"));
    manager.acquire_lock("test-lock", HolderId::new("holder-1"), None, &clock);

    // Advance past stale threshold
    clock.advance(Duration::from_secs(120));

    let config = MaintenanceConfig::new()
        .with_emit_warnings(true)
        .with_reclaim_stale(false);
    let task = MaintenanceTask::new(config, clock.clone());

    let effects = task.tick(&mut manager);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::LockStale { .. }))));
}

#[test]
fn maintenance_tick_reclaims_stale() {
    let clock = FakeClock::new();
    let mut manager = CoordinationManager::new();

    manager.ensure_lock(LockConfig::new("test-lock"));
    manager.acquire_lock("test-lock", HolderId::new("holder-1"), None, &clock);

    // Advance past stale threshold
    clock.advance(Duration::from_secs(120));

    let config = MaintenanceConfig::new()
        .with_emit_warnings(false)
        .with_reclaim_stale(true);
    let task = MaintenanceTask::new(config, clock.clone());

    let effects = task.tick(&mut manager);

    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::LockReleased { .. }))));
    assert!(manager.get_lock("test-lock").unwrap().is_free());
}

#[test]
fn coordination_stats_collect() {
    let clock = FakeClock::new();
    let mut manager = CoordinationManager::new();

    // Add some locks
    manager.ensure_lock(LockConfig::new("lock-1"));
    manager.ensure_lock(LockConfig::new("lock-2"));
    manager.acquire_lock("lock-1", HolderId::new("holder-1"), None, &clock);

    // Add a semaphore with holders
    manager.ensure_semaphore(SemaphoreConfig::new("sem-1", 5));
    manager.acquire_semaphore("sem-1", "holder-1".to_string(), 2, None, &clock);
    manager.acquire_semaphore("sem-1", "holder-2".to_string(), 1, None, &clock);

    let stats = CoordinationStats::collect(&manager, &clock);

    assert_eq!(stats.total_locks, 2);
    assert_eq!(stats.held_locks, 1);
    assert_eq!(stats.stale_locks, 0);
    assert_eq!(stats.total_semaphores, 1);
    assert_eq!(stats.total_semaphore_holders, 2);
    assert_eq!(stats.stale_semaphore_holders, 0);
}

#[test]
fn coordination_stats_detects_stale() {
    let clock = FakeClock::new();
    let mut manager = CoordinationManager::new();

    manager.ensure_lock(LockConfig::new("lock-1"));
    manager.acquire_lock("lock-1", HolderId::new("holder-1"), None, &clock);

    manager.ensure_semaphore(SemaphoreConfig::new("sem-1", 5));
    manager.acquire_semaphore("sem-1", "holder-2".to_string(), 1, None, &clock);

    // Advance past stale threshold
    clock.advance(Duration::from_secs(120));

    let stats = CoordinationStats::collect(&manager, &clock);

    assert_eq!(stats.stale_locks, 1);
    assert_eq!(stats.stale_semaphore_holders, 1);
}

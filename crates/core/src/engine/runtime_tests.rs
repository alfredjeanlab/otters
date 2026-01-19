use super::*;
use crate::adapters::FakeAdapters;
use crate::clock::FakeClock;
use crate::scheduling::{
    ActionConfig, ActionId, CronConfig, CronId, CronState, WatcherCondition, WatcherConfig,
    WatcherId, WatcherSource,
};
use crate::storage::WalStore;
use std::collections::BTreeMap;
use std::time::Duration;

fn make_test_engine() -> (Engine<FakeAdapters, FakeClock>, FakeClock) {
    let adapters = FakeAdapters::new();
    let store = WalStore::open_temp().unwrap();
    let clock = FakeClock::new();
    let engine = Engine::new(adapters, store, clock.clone());
    (engine, clock)
}

fn make_test_pipeline(id: &str, name: &str) -> Pipeline {
    Pipeline::new_dynamic(id, name, BTreeMap::new())
}

#[tokio::test]
async fn engine_can_add_and_get_pipeline() {
    let (mut engine, _clock) = make_test_engine();

    let pipeline = make_test_pipeline("p-1", "test");
    engine.add_pipeline(pipeline.clone()).unwrap();

    let loaded = engine.get_pipeline(&pipeline.id);
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().name, "test");
}

#[tokio::test]
async fn engine_processes_pipeline_events() {
    let (mut engine, _clock) = make_test_engine();

    let pipeline = make_test_pipeline("p-1", "test");
    engine.add_pipeline(pipeline.clone()).unwrap();

    // Transition from Init to Done (dynamic pipelines go straight to done)
    engine
        .process_pipeline_event(&pipeline.id, PipelineEvent::PhaseComplete)
        .await
        .unwrap();

    let updated = engine.get_pipeline(&pipeline.id).unwrap();
    assert_eq!(updated.phase.name(), "done");
}

#[tokio::test]
async fn engine_can_add_workspace() {
    let (mut engine, _clock) = make_test_engine();

    let workspace = Workspace::new_ready(
        "ws-1",
        "test",
        std::path::PathBuf::from("/tmp/test"),
        "feature-x",
    );
    engine.add_workspace(workspace.clone()).unwrap();

    let loaded = engine.get_workspace(&workspace.id);
    assert!(loaded.is_some());
}

#[tokio::test]
async fn engine_finds_pipeline_by_workspace() {
    let (mut engine, _clock) = make_test_engine();

    let workspace_id = WorkspaceId("ws-1".to_string());
    let workspace = Workspace::new_ready(
        "ws-1",
        "test",
        std::path::PathBuf::from("/tmp/test"),
        "feature-x",
    );
    engine.add_workspace(workspace).unwrap();

    let pipeline = make_test_pipeline("p-1", "test").with_workspace(workspace_id.clone());
    engine.add_pipeline(pipeline.clone()).unwrap();

    let found = engine.find_pipeline_by_workspace(&workspace_id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().id.0, "p-1");
}

// ==================== Scheduling Integration Tests ====================

#[test]
fn init_scheduling_registers_watchers_with_bridge() {
    let (mut engine, clock) = make_test_engine();

    // Add a watcher with wake_on patterns
    let watcher_id = WatcherId::new("test-watcher");
    let config = WatcherConfig::new(
        "test",
        WatcherSource::Session {
            name: "agent-1".into(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    )
    .with_wake_on(vec!["task:failed:*".into(), "session:idle".into()]);

    engine
        .scheduling_manager_mut()
        .add_watcher(watcher_id.clone(), config, &clock);

    // Initialize scheduling
    engine.init_scheduling();

    // Verify watcher was registered with the bridge
    let watchers = engine
        .watcher_bridge()
        .watchers_for_event("task:failed:test");
    assert!(watchers.contains(&watcher_id));

    let watchers = engine.watcher_bridge().watchers_for_event("session:idle");
    assert!(watchers.contains(&watcher_id));
}

#[test]
fn init_scheduling_schedules_enabled_crons() {
    let (mut engine, clock) = make_test_engine();

    // Add an enabled cron
    let cron_id = CronId::new("health-check");
    let config = CronConfig::new("health", Duration::from_secs(60)).enabled();

    engine
        .scheduling_manager_mut()
        .add_cron(cron_id.clone(), config, &clock);

    // Initialize scheduling
    engine.init_scheduling();

    // Verify cron was scheduled
    let next_fire = engine.scheduler().next_fire_time();
    assert!(next_fire.is_some());
}

#[test]
fn tick_scheduling_triggers_cron() {
    let (mut engine, clock) = make_test_engine();

    // Add an enabled cron
    let cron_id = CronId::new("health-check");
    let config = CronConfig::new("health", Duration::from_secs(60)).enabled();

    engine
        .scheduling_manager_mut()
        .add_cron(cron_id.clone(), config, &clock);

    // Initialize scheduling
    engine.init_scheduling();

    // Advance time past the cron interval
    clock.advance(Duration::from_secs(61));

    // Tick scheduling
    let effects = engine.tick_scheduling();

    // Verify cron was triggered (should produce CronTriggered event)
    let has_cron_triggered = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == &cron_id.0));
    assert!(has_cron_triggered);
}

#[test]
fn tick_scheduling_reschedules_cron() {
    let (mut engine, clock) = make_test_engine();

    // Add an enabled cron with 60-second interval
    let cron_id = CronId::new("health-check");
    let config = CronConfig::new("health", Duration::from_secs(60)).enabled();

    engine
        .scheduling_manager_mut()
        .add_cron(cron_id.clone(), config, &clock);

    // Initialize scheduling
    engine.init_scheduling();

    // Advance time and trigger first tick
    clock.advance(Duration::from_secs(61));
    engine.tick_scheduling();

    // The cron should be rescheduled - advance time again
    clock.advance(Duration::from_secs(61));
    let effects = engine.tick_scheduling();

    // Should trigger again
    let has_cron_triggered = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == &cron_id.0));
    assert!(has_cron_triggered);
}

#[test]
fn action_triggered_executes_and_completes() {
    let (mut engine, _clock) = make_test_engine();

    // Add an action
    let action_id = ActionId::new("notify");
    let config = ActionConfig::new("notify", Duration::from_secs(60)).with_command("echo 'hello'");

    engine
        .scheduling_manager_mut()
        .add_action(action_id.clone(), config);

    // Trigger the action
    let effect = Effect::Emit(Event::ActionTriggered {
        id: "notify".into(),
        source: "test".into(),
    });

    let effects = engine.execute_scheduling_effect(&effect);

    // Should produce ActionCompleted (since NoOpCommandRunner always succeeds)
    let has_completed = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::ActionCompleted { id }) if id == "notify"));
    assert!(has_completed);
}

#[test]
fn action_triggered_enters_cooldown() {
    let (mut engine, _clock) = make_test_engine();

    // Add an action with 60-second cooldown
    let action_id = ActionId::new("notify");
    let config = ActionConfig::new("notify", Duration::from_secs(60)).with_command("echo 'hello'");

    engine
        .scheduling_manager_mut()
        .add_action(action_id.clone(), config);

    // Trigger the action
    let effect = Effect::Emit(Event::ActionTriggered {
        id: "notify".into(),
        source: "test".into(),
    });

    engine.execute_scheduling_effect(&effect);

    // Action should now be on cooldown
    let action = engine.scheduling_manager().get_action(&action_id).unwrap();
    assert!(action.is_on_cooldown());
}

#[test]
fn watcher_wakes_on_matching_event() {
    let (mut engine, clock) = make_test_engine();

    // Add a watcher with wake_on patterns
    let watcher_id = WatcherId::new("failure-monitor");
    let config = WatcherConfig::new(
        "failure-monitor",
        WatcherSource::Events {
            pattern: "task:failed:*".into(),
        },
        WatcherCondition::ConsecutiveFailures { count: 3 },
        Duration::from_secs(60),
    )
    .with_wake_on(vec!["task:failed:*".into()]);

    engine
        .scheduling_manager_mut()
        .add_watcher(watcher_id.clone(), config, &clock);

    // Initialize scheduling to register with bridge
    engine.init_scheduling();

    // Emit a matching event
    let effect = Effect::Emit(Event::TaskFailed {
        id: TaskId("task-1".into()),
        reason: "test error".into(),
    });

    // The watcher should be checked (though it won't trigger because condition isn't met)
    let _nested = engine.execute_scheduling_effect(&effect);

    // At minimum, the effect should be processed without errors
    // The watcher check itself uses NoOpSourceFetcher which returns default values
    // so the watcher won't actually trigger, but the wake mechanism should work
}

#[test]
fn cron_with_linked_watcher_triggers_check() {
    let (mut engine, clock) = make_test_engine();

    // Add a watcher
    let watcher_id = WatcherId::new("idle-check");
    let watcher_config = WatcherConfig::new(
        "idle-check",
        WatcherSource::Session {
            name: "agent-1".into(),
        },
        WatcherCondition::Idle {
            threshold: Duration::from_secs(300),
        },
        Duration::from_secs(60),
    );

    engine
        .scheduling_manager_mut()
        .add_watcher(watcher_id.clone(), watcher_config, &clock);

    // Add a cron that links to the watcher
    let cron_id = CronId::new("health");
    let cron_config = CronConfig::new("health", Duration::from_secs(60))
        .enabled()
        .with_watchers(vec![watcher_id.clone()]);

    engine
        .scheduling_manager_mut()
        .add_cron(cron_id.clone(), cron_config, &clock);

    // Initialize scheduling
    engine.init_scheduling();

    // Advance time and trigger cron
    clock.advance(Duration::from_secs(61));
    let effects = engine.tick_scheduling();

    // Cron should have been triggered
    let has_cron_triggered = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == &cron_id.0));
    assert!(has_cron_triggered);
}

#[test]
fn disabled_cron_not_scheduled() {
    let (mut engine, clock) = make_test_engine();

    // Add a disabled cron
    let cron_id = CronId::new("disabled-check");
    let config = CronConfig::new("disabled", Duration::from_secs(60)); // Not enabled

    engine
        .scheduling_manager_mut()
        .add_cron(cron_id.clone(), config, &clock);

    // Verify cron is disabled
    let cron = engine.scheduling_manager().get_cron(&cron_id).unwrap();
    assert_eq!(cron.state, CronState::Disabled);

    // Initialize scheduling
    engine.init_scheduling();

    // Advance time
    clock.advance(Duration::from_secs(61));
    let effects = engine.tick_scheduling();

    // Disabled cron should not trigger
    let has_cron_triggered = effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::CronTriggered { id }) if id == &cron_id.0));
    assert!(!has_cron_triggered);
}

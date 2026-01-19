use super::*;
use crate::clock::FakeClock;

#[test]
fn session_starts_in_starting_state() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );
    assert!(matches!(session.state, SessionState::Starting));
}

#[test]
fn session_transitions_to_running() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );
    let (session, effects) = session.mark_running(&clock);
    assert!(matches!(session.state, SessionState::Running));
    assert_eq!(effects.len(), 1);
}

#[test]
fn session_becomes_idle_after_threshold() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );
    let (session, _) = session.mark_running(&clock);

    // Advance past idle threshold
    clock.advance(Duration::from_secs(120));

    let (session, effects) = session.evaluate_heartbeat(None, None, &clock);
    assert!(matches!(session.state, SessionState::Idle { .. }));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::SessionIdle { .. }))));
}

#[test]
fn session_recovers_from_idle_on_output() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );
    let (session, _) = session.mark_running(&clock);

    // Make it idle
    clock.advance(Duration::from_secs(120));
    let (session, _) = session.evaluate_heartbeat(None, None, &clock);
    assert!(matches!(session.state, SessionState::Idle { .. }));

    // New output arrives
    let now = clock.now();
    let (session, effects) = session.evaluate_heartbeat(Some(now), Some(12345), &clock);
    assert!(matches!(session.state, SessionState::Running));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::SessionActive { .. }))));
}

#[test]
fn hash_output_produces_consistent_hashes() {
    let output = "Hello, world!";
    let hash1 = hash_output(output);
    let hash2 = hash_output(output);
    assert_eq!(hash1, hash2);

    let hash3 = hash_output("Different output");
    assert_ne!(hash1, hash3);
}

#[test]
fn heartbeat_updates_last_activity() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );

    assert!(session.last_heartbeat.is_none());

    let session = session.record_heartbeat(clock.now());

    assert!(session.last_heartbeat.is_some());
}

#[test]
fn idle_time_calculated_correctly() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );

    // No heartbeat yet, idle_time is None
    assert!(session.idle_time(clock.now()).is_none());

    // Record heartbeat
    let session = session.record_heartbeat(clock.now());

    // Advance clock
    clock.advance(Duration::from_secs(30));

    // Check idle time
    let idle = session.idle_time(clock.now());
    assert_eq!(idle, Some(Duration::from_secs(30)));
}

#[test]
fn is_idle_by_heartbeat_works() {
    let clock = FakeClock::new();
    let session = Session::new(
        "sess-1",
        WorkspaceId("ws-1".to_string()),
        Duration::from_secs(60),
        &clock,
    );

    // No heartbeat yet - not idle (returns false by default)
    assert!(!session.is_idle_by_heartbeat(clock.now()));

    // Record heartbeat
    let session = session.record_heartbeat(clock.now());

    // Not idle yet
    assert!(!session.is_idle_by_heartbeat(clock.now()));

    // Advance past threshold
    clock.advance(Duration::from_secs(120));

    // Now idle
    assert!(session.is_idle_by_heartbeat(clock.now()));
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::FakeClock;

#[test]
fn pipeline_creation() {
    let clock = FakeClock::new();
    let pipeline = Pipeline::new(
        "pipe-1".to_string(),
        "test-feature".to_string(),
        "build".to_string(),
        HashMap::new(),
        "init".to_string(),
        &clock,
    );

    assert_eq!(pipeline.phase, "init");
    assert_eq!(pipeline.phase_status, PhaseStatus::Pending);
    assert!(pipeline.workspace_path.is_none());
    assert!(pipeline.session_id.is_none());
}

#[test]
fn pipeline_session_started() {
    let clock = FakeClock::new();
    let pipeline = Pipeline::new(
        "pipe-1".to_string(),
        "test".to_string(),
        "build".to_string(),
        HashMap::new(),
        "init".to_string(),
        &clock,
    )
    .with_session("sess-1".to_string());

    let event = Event::SessionStarted {
        session_id: "sess-1".to_string(),
    };
    let (new_pipeline, effects) = pipeline.transition(&event, &clock);

    assert_eq!(new_pipeline.phase_status, PhaseStatus::Running);
    assert!(effects.is_empty());
}

#[test]
fn pipeline_session_exit_success() {
    let clock = FakeClock::new();
    let mut pipeline = Pipeline::new(
        "pipe-1".to_string(),
        "test".to_string(),
        "build".to_string(),
        HashMap::new(),
        "init".to_string(),
        &clock,
    );
    pipeline.session_id = Some("sess-1".to_string());
    pipeline.phase_status = PhaseStatus::Running;

    let event = Event::SessionExited {
        session_id: "sess-1".to_string(),
        exit_code: 0,
    };
    let (new_pipeline, effects) = pipeline.transition(&event, &clock);

    // Phase transition is now handled by the runtime, not Pipeline::transition
    // The pipeline just marks the phase as completed
    assert_eq!(new_pipeline.phase, "init");
    assert_eq!(new_pipeline.phase_status, PhaseStatus::Completed);
    assert!(new_pipeline.session_id.is_none());
    assert!(effects.is_empty());
}

#[test]
fn pipeline_session_exit_failure() {
    let clock = FakeClock::new();
    let mut pipeline = Pipeline::new(
        "pipe-1".to_string(),
        "test".to_string(),
        "build".to_string(),
        HashMap::new(),
        "init".to_string(),
        &clock,
    );
    pipeline.session_id = Some("sess-1".to_string());
    pipeline.phase_status = PhaseStatus::Running;

    let event = Event::SessionExited {
        session_id: "sess-1".to_string(),
        exit_code: 1,
    };
    let (new_pipeline, effects) = pipeline.transition(&event, &clock);

    assert_eq!(new_pipeline.phase, "failed");
    assert_eq!(new_pipeline.phase_status, PhaseStatus::Failed);
    assert!(new_pipeline.error.is_some());
    assert_eq!(effects.len(), 1); // Persist
}

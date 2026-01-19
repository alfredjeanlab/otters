use super::*;
use crate::clock::FakeClock;
use std::collections::BTreeMap;

fn make_test_pipeline(id: &str, name: &str) -> Pipeline {
    Pipeline::new_dynamic(id, name, BTreeMap::new())
}

fn make_test_pipeline_with_inputs(
    id: &str,
    name: &str,
    inputs: BTreeMap<String, String>,
) -> Pipeline {
    Pipeline::new_dynamic(id, name, inputs)
}

#[test]
fn dynamic_pipeline_starts_in_init() {
    let pipeline = make_test_pipeline("p-1", "auth");
    assert_eq!(pipeline.phase, Phase::Init);
    assert_eq!(pipeline.kind, PipelineKind::Dynamic);
}

#[test]
fn dynamic_pipeline_init_to_done() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");

    // Dynamic pipelines go directly from Init to Done
    let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
    assert_eq!(pipeline.phase, Phase::Done);
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::PipelineComplete { .. }))));
}

#[test]
fn pipeline_can_fail_from_any_phase() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");

    let (pipeline, effects) = pipeline.transition(
        PipelineEvent::PhaseFailed {
            reason: "Tests failed".to_string(),
        },
        &clock,
    );
    assert!(matches!(pipeline.phase, Phase::Failed { .. }));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::PipelineFailed { .. }))));
}

#[test]
fn pipeline_emits_phase_events() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");
    let (_, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);

    // Dynamic pipeline goes to Done, so we get PipelineComplete event
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::PipelineComplete { .. }))));
}

#[test]
fn pipeline_transition_with_outputs() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");

    let mut outputs = BTreeMap::new();
    outputs.insert("plan".to_string(), "detailed plan here".to_string());

    let (pipeline, _) =
        pipeline.transition(PipelineEvent::PhaseCompleteWithOutputs { outputs }, &clock);

    // Goes to Done and has the outputs
    assert_eq!(pipeline.phase, Phase::Done);
    assert_eq!(
        pipeline.outputs.get("plan"),
        Some(&"detailed plan here".to_string())
    );
}

#[test]
fn pipeline_running_to_blocked() {
    let clock = FakeClock::new();

    // Create a pipeline that's in a working phase (not Init, not terminal)
    // We'll simulate by setting up outputs to indicate a working phase
    let mut pipeline = make_test_pipeline("p-1", "auth");
    pipeline.phase = Phase::Plan; // Set to a working phase manually

    let (pipeline, effects) = pipeline.transition(
        PipelineEvent::PhaseFailedRecoverable {
            reason: "Need more context".to_string(),
        },
        &clock,
    );

    assert!(matches!(
        &pipeline.phase,
        Phase::Blocked { waiting_on, .. } if waiting_on == "Need more context"
    ));
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::PipelineBlocked { .. })
    ));
}

#[test]
fn pipeline_blocked_to_running() {
    let clock = FakeClock::new();

    // Create a blocked pipeline with a runbook phase set
    let mut pipeline = make_test_pipeline("p-1", "auth");
    pipeline.phase = Phase::Blocked {
        waiting_on: "Need more context".to_string(),
        guard_id: None,
    };
    pipeline
        .outputs
        .insert("_runbook_phase".to_string(), "plan".to_string());

    let (pipeline, effects) = pipeline.transition(PipelineEvent::Unblocked, &clock);

    // Should resume to the first working phase from runbook metadata
    assert_eq!(pipeline.phase, Phase::Plan);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::PipelineResumed { .. })
    ));
}

#[test]
fn pipeline_task_assignment() {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "auth");
    pipeline.phase = Phase::Plan; // In a working phase

    let (pipeline, effects) = pipeline.transition(
        PipelineEvent::TaskAssigned {
            task_id: TaskId("task-1".to_string()),
        },
        &clock,
    );

    assert_eq!(pipeline.current_task_id, Some(TaskId("task-1".to_string())));
    assert!(effects.is_empty());
}

#[test]
fn pipeline_task_completion_advances_phase() {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "auth");
    pipeline.phase = Phase::Plan; // In a working phase

    let (pipeline, _) = pipeline.transition(
        PipelineEvent::TaskAssigned {
            task_id: TaskId("task-1".to_string()),
        },
        &clock,
    );

    let (pipeline, effects) = pipeline.transition(
        PipelineEvent::TaskComplete {
            task_id: TaskId("task-1".to_string()),
            output: Some("plan output".to_string()),
        },
        &clock,
    );

    // For dynamic pipelines, next phase is Done
    assert_eq!(pipeline.phase, Phase::Done);
    assert!(pipeline.outputs.contains_key("task_output"));
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::PipelineComplete { .. })
    ));
}

#[test]
fn pipeline_checkpoint() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");

    let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

    assert_eq!(pipeline.checkpoint_sequence, 1);
    assert!(pipeline.last_checkpoint_at.is_some());
    assert!(matches!(
        &effects[0],
        Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == 1
    ));
}

#[test]
fn pipeline_checkpoint_increments_sequence() {
    let clock = FakeClock::new();
    let pipeline = make_test_pipeline("p-1", "auth");

    let (pipeline, _) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
    assert_eq!(pipeline.checkpoint_sequence, 1);

    let (pipeline, _) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
    assert_eq!(pipeline.checkpoint_sequence, 2);

    let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);
    assert_eq!(pipeline.checkpoint_sequence, 3);
    assert!(matches!(
        &effects[0],
        Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == 3
    ));
}

#[test]
fn pipeline_restore_from_checkpoint() {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "auth");
    pipeline.phase = Phase::Plan; // Set to working phase

    // Take checkpoint
    let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

    let checkpoint = if let Effect::SaveCheckpoint { checkpoint, .. } = &effects[0] {
        checkpoint.clone()
    } else {
        panic!("Expected SaveCheckpoint effect");
    };

    // Block it (recoverable failure)
    let (pipeline, _) = pipeline.transition(
        PipelineEvent::PhaseFailedRecoverable {
            reason: "error".to_string(),
        },
        &clock,
    );
    assert!(matches!(pipeline.phase, Phase::Blocked { .. }));

    // Restore from checkpoint
    let (pipeline, effects) = pipeline.transition(PipelineEvent::Restore { checkpoint }, &clock);

    assert_eq!(pipeline.phase, Phase::Plan);
    assert!(matches!(
        &effects[0],
        Effect::Emit(Event::PipelineRestored { from_sequence, .. }) if *from_sequence == 1
    ));
}

#[test]
fn done_is_terminal() {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "test");
    pipeline.phase = Phase::Done;

    // Try to transition further - should be no-op
    let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
    assert_eq!(pipeline.phase, Phase::Done);
    assert!(effects.is_empty());
}

#[test]
fn failed_is_terminal() {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "test");
    pipeline.phase = Phase::Failed {
        reason: "error".to_string(),
    };

    // Try to transition further - should be no-op
    let (pipeline, effects) = pipeline.transition(PipelineEvent::PhaseComplete, &clock);
    assert!(matches!(pipeline.phase, Phase::Failed { .. }));
    assert!(effects.is_empty());
}

use yare::parameterized;

#[parameterized(
        from_init = { "init" },
        from_plan = { "plan" },
        from_execute = { "execute" },
    )]
fn failure_from_any_phase(current: &str) {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "test");

    // Set the phase manually
    pipeline.phase = match current {
        "init" => Phase::Init,
        "plan" => Phase::Plan,
        "execute" => Phase::Execute,
        _ => Phase::Init,
    };

    // Fail it
    let (pipeline, effects) = pipeline.transition(
        PipelineEvent::PhaseFailed {
            reason: "Test failure".to_string(),
        },
        &clock,
    );

    assert!(matches!(pipeline.phase, Phase::Failed { .. }));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Emit(Event::PipelineFailed { .. }))));
}

#[parameterized(
        checkpoint_at_init = { "init", 1 },
        checkpoint_at_plan = { "plan", 1 },
        checkpoint_at_execute = { "execute", 1 },
    )]
fn checkpoint_at_phase(phase: &str, expected_sequence: u64) {
    let clock = FakeClock::new();
    let mut pipeline = make_test_pipeline("p-1", "test");

    // Set the phase manually
    pipeline.phase = match phase {
        "init" => Phase::Init,
        "plan" => Phase::Plan,
        "execute" => Phase::Execute,
        _ => Phase::Init,
    };

    // Take a checkpoint
    let (pipeline, effects) = pipeline.transition(PipelineEvent::RequestCheckpoint, &clock);

    assert_eq!(pipeline.checkpoint_sequence, expected_sequence);
    assert!(matches!(
        &effects[0],
        Effect::SaveCheckpoint { checkpoint, .. } if checkpoint.sequence == expected_sequence
    ));
}

#[test]
fn pipeline_restore_from_creates_pipeline() {
    let checkpoint = Checkpoint {
        pipeline_id: PipelineId("p-1".to_string()),
        phase: "execute".to_string(),
        inputs: {
            let mut m = BTreeMap::new();
            m.insert("prompt".to_string(), "test".to_string());
            m
        },
        outputs: {
            let mut m = BTreeMap::new();
            m.insert("plan".to_string(), "plan output".to_string());
            m
        },
        created_at: std::time::Instant::now(),
        sequence: 5,
    };

    let pipeline = Pipeline::restore_from(checkpoint, PipelineKind::Dynamic);

    assert_eq!(pipeline.id.0, "p-1");
    assert_eq!(pipeline.phase, Phase::Execute);
    assert_eq!(pipeline.checkpoint_sequence, 5);
    assert_eq!(
        pipeline.outputs.get("plan"),
        Some(&"plan output".to_string())
    );
}

#[test]
fn pipeline_with_inputs() {
    let mut inputs = BTreeMap::new();
    inputs.insert("prompt".to_string(), "Build auth feature".to_string());
    inputs.insert("name".to_string(), "auth".to_string());

    let pipeline = make_test_pipeline_with_inputs("p-1", "auth", inputs);

    assert_eq!(
        pipeline.inputs.get("prompt"),
        Some(&"Build auth feature".to_string())
    );
    assert_eq!(pipeline.inputs.get("name"), Some(&"auth".to_string()));
}

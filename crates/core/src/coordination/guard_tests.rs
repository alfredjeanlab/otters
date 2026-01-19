use super::*;

fn inputs_with_locks() -> GuardInputs {
    let mut inputs = GuardInputs::default();
    inputs.locks.insert("free-lock".to_string(), true);
    inputs.locks.insert("held-lock".to_string(), false);
    inputs
        .lock_holders
        .insert("held-lock".to_string(), "holder-1".to_string());
    inputs
}

fn inputs_with_semaphores() -> GuardInputs {
    let mut inputs = GuardInputs::default();
    inputs.semaphores.insert("agent-slots".to_string(), 3);
    inputs.semaphores.insert("build-slots".to_string(), 0);
    inputs
}

fn inputs_with_branches() -> GuardInputs {
    let mut inputs = GuardInputs::default();
    inputs.branches.insert("main".to_string(), true);
    inputs.branches.insert("feature/x".to_string(), true);
    inputs.branches.insert("feature/deleted".to_string(), false);
    inputs
        .branch_merged
        .insert(("feature/x".to_string(), "main".to_string()), true);
    inputs
        .branch_merged
        .insert(("feature/y".to_string(), "main".to_string()), false);
    inputs
}

fn inputs_with_issues() -> GuardInputs {
    let mut inputs = GuardInputs::default();
    inputs
        .issues
        .insert("issue-1".to_string(), IssueStatus::Done);
    inputs
        .issues
        .insert("issue-2".to_string(), IssueStatus::InProgress);
    inputs
        .issues
        .insert("issue-3".to_string(), IssueStatus::Todo);
    inputs.issue_lists.insert(
        "feature:x".to_string(),
        vec!["issue-1".to_string(), "issue-2".to_string()],
    );
    inputs
        .issue_lists
        .insert("done-issues".to_string(), vec!["issue-1".to_string()]);
    inputs
}

// Lock tests
#[test]
fn lock_free_passes_when_free() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::lock_free("free-lock");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn lock_free_fails_when_held() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::lock_free("held-lock");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn lock_free_needs_input_when_unknown() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::lock_free("unknown-lock");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::NeedsInput { .. }
    ));
}

#[test]
fn lock_held_by_passes_when_correct_holder() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::lock_held_by("held-lock", "holder-1");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn lock_held_by_fails_when_wrong_holder() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::lock_held_by("held-lock", "holder-2");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

// Semaphore tests
#[test]
fn semaphore_available_passes_with_enough_slots() {
    let inputs = inputs_with_semaphores();
    let guard = GuardCondition::semaphore_available("agent-slots", 2);
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn semaphore_available_fails_with_insufficient_slots() {
    let inputs = inputs_with_semaphores();
    let guard = GuardCondition::semaphore_available("build-slots", 1);
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

// Branch tests
#[test]
fn branch_exists_passes() {
    let inputs = inputs_with_branches();
    let guard = GuardCondition::branch_exists("main");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn branch_exists_fails() {
    let inputs = inputs_with_branches();
    let guard = GuardCondition::branch_exists("feature/deleted");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn branch_not_exists_passes() {
    let inputs = inputs_with_branches();
    let guard = GuardCondition::branch_not_exists("feature/deleted");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn branch_merged_passes() {
    let inputs = inputs_with_branches();
    let guard = GuardCondition::branch_merged("feature/x", "main");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn branch_merged_fails() {
    let inputs = inputs_with_branches();
    let guard = GuardCondition::branch_merged("feature/y", "main");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

// Issue tests
#[test]
fn issue_status_passes() {
    let inputs = inputs_with_issues();
    let guard = GuardCondition::issue_in_status("issue-1", IssueStatus::Done);
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn issue_status_fails() {
    let inputs = inputs_with_issues();
    let guard = GuardCondition::issue_in_status("issue-2", IssueStatus::Done);
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn issues_complete_passes_when_all_done() {
    let inputs = inputs_with_issues();
    let guard = GuardCondition::issues_complete("done-issues");
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn issues_complete_fails_when_not_all_done() {
    let inputs = inputs_with_issues();
    let guard = GuardCondition::issues_complete("feature:x");
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

// Composite tests
#[test]
fn all_passes_when_all_pass() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::all(vec![
        GuardCondition::lock_free("free-lock"),
        GuardCondition::lock_held_by("held-lock", "holder-1"),
    ]);
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn all_fails_when_one_fails() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::all(vec![
        GuardCondition::lock_free("free-lock"),
        GuardCondition::lock_free("held-lock"), // This will fail
    ]);
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn any_passes_when_one_passes() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::any(vec![
        GuardCondition::lock_free("held-lock"), // Fails
        GuardCondition::lock_free("free-lock"), // Passes
    ]);
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn any_fails_when_all_fail() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::any(vec![
        GuardCondition::lock_free("held-lock"),
        GuardCondition::lock_held_by("held-lock", "holder-2"),
    ]);
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

#[test]
fn not_passes_when_inner_fails() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::negate(GuardCondition::lock_free("held-lock"));
    assert_eq!(guard.evaluate(&inputs), GuardResult::Passed);
}

#[test]
fn not_fails_when_inner_passes() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::negate(GuardCondition::lock_free("free-lock"));
    assert!(matches!(
        guard.evaluate(&inputs),
        GuardResult::Failed { .. }
    ));
}

// Required inputs tests
#[test]
fn required_inputs_for_simple_condition() {
    let guard = GuardCondition::lock_free("test-lock");
    let inputs = guard.required_inputs();
    assert_eq!(inputs.len(), 1);
    assert!(matches!(
        &inputs[0],
        GuardInputType::LockState { lock_name } if lock_name == "test-lock"
    ));
}

#[test]
fn required_inputs_for_composite_condition() {
    let guard = GuardCondition::all(vec![
        GuardCondition::lock_free("lock-1"),
        GuardCondition::semaphore_available("sem-1", 1),
    ]);
    let inputs = guard.required_inputs();
    assert_eq!(inputs.len(), 2);
}

#[test]
fn evaluation_is_deterministic() {
    let inputs = inputs_with_locks();
    let guard = GuardCondition::all(vec![
        GuardCondition::lock_free("free-lock"),
        GuardCondition::lock_held_by("held-lock", "holder-1"),
    ]);
    let result1 = guard.evaluate(&inputs);
    let result2 = guard.evaluate(&inputs);
    assert_eq!(result1, result2);
}

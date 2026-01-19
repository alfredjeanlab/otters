use super::*;

#[test]
fn phase_guards_default_is_empty() {
    let guards = PhaseGuards::new();
    assert!(!guards.has_guards());
    assert!(guards.pre.is_none());
    assert!(guards.post.is_none());
}

#[test]
fn phase_guards_with_pre() {
    let guards = PhaseGuards::new().with_pre(GuardCondition::lock_free("main-branch"));
    assert!(guards.has_guards());
    assert!(guards.pre.is_some());
    assert!(guards.post.is_none());
}

#[test]
fn phase_guards_with_post() {
    let guards = PhaseGuards::new().with_post(GuardCondition::branch_exists("feature/x"));
    assert!(guards.has_guards());
    assert!(guards.pre.is_none());
    assert!(guards.post.is_some());
}

#[test]
fn phase_guards_with_wake_on() {
    let guards = PhaseGuards::new()
        .with_pre(GuardCondition::lock_free("main-branch"))
        .with_wake_on(vec!["lock:released".to_string()]);

    assert_eq!(guards.wake_patterns(), &["lock:released"]);
}

#[test]
fn blocked_guard_auto_generates_wake_patterns() {
    let blocked = BlockedGuard::new(
        "guard-1",
        "pipeline-1",
        "merge",
        GuardCondition::lock_free("main-branch"),
        GuardType::Pre,
        "Lock is held",
    );

    assert!(!blocked.wake_on.is_empty());
    assert!(blocked.wake_on.iter().any(|p| p.starts_with("lock")));
}

#[test]
fn blocked_guard_for_semaphore() {
    let blocked = BlockedGuard::new(
        "guard-1",
        "pipeline-1",
        "execute",
        GuardCondition::semaphore_available("agent-slots", 1),
        GuardType::Pre,
        "No slots available",
    );

    assert!(blocked.wake_on.iter().any(|p| p.starts_with("semaphore")));
}

#[test]
fn pipeline_guards_stores_per_phase() {
    let guards = PipelineGuards::new()
        .with_phase(
            "merge",
            PhaseGuards::new().with_pre(GuardCondition::lock_free("main-branch")),
        )
        .with_phase(
            "execute",
            PhaseGuards::new().with_pre(GuardCondition::semaphore_available("agents", 1)),
        );

    assert!(guards.get_phase("merge").is_some());
    assert!(guards.get_phase("execute").is_some());
    assert!(guards.get_phase("plan").is_none());
}

#[test]
fn guard_type_display() {
    assert_eq!(format!("{}", GuardType::Pre), "pre");
    assert_eq!(format!("{}", GuardType::Post), "post");
}

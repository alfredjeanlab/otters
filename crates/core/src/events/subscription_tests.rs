use super::*;

#[test]
fn exact_pattern_matches_exact_event() {
    let pattern = EventPattern::new("pipeline:complete");
    assert!(pattern.matches("pipeline:complete"));
    assert!(!pattern.matches("pipeline:failed"));
    assert!(!pattern.matches("task:complete"));
}

#[test]
fn wildcard_matches_single_segment() {
    let pattern = EventPattern::new("pipeline:*");
    assert!(pattern.matches("pipeline:complete"));
    assert!(pattern.matches("pipeline:failed"));
    assert!(!pattern.matches("task:complete"));
    assert!(!pattern.matches("pipeline:item:added")); // * doesn't match multiple segments
}

#[test]
fn double_wildcard_matches_everything_after() {
    let pattern = EventPattern::new("queue:**");
    assert!(pattern.matches("queue:item:added"));
    assert!(pattern.matches("queue:item:complete"));
    assert!(pattern.matches("queue:anything"));
    assert!(!pattern.matches("task:complete"));
}

#[test]
fn global_wildcards() {
    let star = EventPattern::new("*");
    let double_star = EventPattern::new("**");

    assert!(star.matches("anything"));
    assert!(double_star.matches("anything:here:too"));
}

#[test]
fn subscription_matches_any_pattern() {
    let sub = Subscription::new(
        "test-sub",
        vec![
            EventPattern::new("pipeline:complete"),
            EventPattern::new("task:**"),
        ],
        "Test subscription",
    );

    assert!(sub.matches("pipeline:complete"));
    assert!(sub.matches("task:started"));
    assert!(sub.matches("task:failed"));
    assert!(!sub.matches("queue:item:added"));
}

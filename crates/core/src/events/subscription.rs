// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Event pattern matching and subscriptions

/// Pattern for matching events
/// Supports:
///   - Exact: "pipeline:complete"
///   - Single wildcard: "pipeline:*" matches "pipeline:complete", "pipeline:failed"
///   - Category: "task:**" matches all task events
#[derive(Clone, Debug)]
pub struct EventPattern(String);

impl EventPattern {
    pub fn new(pattern: &str) -> Self {
        Self(pattern.to_string())
    }

    /// Check if this pattern matches an event name
    pub fn matches(&self, event_name: &str) -> bool {
        // Empty pattern matches nothing
        if self.0.is_empty() {
            return false;
        }

        if self.0 == "*" || self.0 == "**" {
            return true;
        }

        // Split into segments
        let pattern_parts: Vec<&str> = self.0.split(':').collect();
        let event_parts: Vec<&str> = event_name.split(':').collect();

        Self::match_segments(&pattern_parts, &event_parts)
    }

    fn match_segments(pattern: &[&str], event: &[&str]) -> bool {
        match (pattern.first(), event.first()) {
            (None, None) => true,
            (Some(&"**"), _) => true, // ** matches everything remaining
            (Some(&"*"), Some(_)) => {
                // * matches single segment
                Self::match_segments(&pattern[1..], &event[1..])
            }
            (Some(p), Some(e)) if *p == *e => Self::match_segments(&pattern[1..], &event[1..]),
            _ => false,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Subscriber handle for unsubscribing
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub String);

/// A subscription to specific event patterns
#[derive(Clone, Debug)]
pub struct Subscription {
    pub id: SubscriberId,
    pub patterns: Vec<EventPattern>,
    pub description: String,
}

impl Subscription {
    pub fn new(
        id: impl Into<String>,
        patterns: Vec<EventPattern>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: SubscriberId(id.into()),
            patterns,
            description: description.into(),
        }
    }

    /// Check if any pattern matches the event
    pub fn matches(&self, event_name: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(event_name))
    }
}

#[cfg(test)]
mod tests {
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
}

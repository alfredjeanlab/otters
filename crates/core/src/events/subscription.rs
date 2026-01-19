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
#[path = "subscription_tests.rs"]
mod tests;

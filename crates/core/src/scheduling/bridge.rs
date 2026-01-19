// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! WatcherEventBridge - connects events to watchers for event-driven wake
//!
//! The WatcherEventBridge allows watchers to be triggered by events instead of
//! just timers, enabling reactive monitoring without polling.

use super::WatcherId;
use std::collections::HashMap;

/// A pattern for matching event names
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventPattern(pub String);

impl EventPattern {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Check if an event name matches this pattern
    ///
    /// Supports:
    /// - Exact match: "task:failed" matches "task:failed"
    /// - Prefix match with *: "task:*" matches "task:failed", "task:started"
    /// - Any match: "*" matches everything
    pub fn matches(&self, event_name: &str) -> bool {
        if self.0 == "*" {
            return true;
        }

        if let Some(prefix) = self.0.strip_suffix('*') {
            event_name.starts_with(prefix)
        } else {
            self.0 == event_name
        }
    }
}

impl From<String> for EventPattern {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EventPattern {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Bridges events to watchers for event-driven wake
#[derive(Debug, Clone, Default)]
pub struct WatcherEventBridge {
    /// Map from event pattern to watcher IDs that should wake
    subscriptions: HashMap<EventPattern, Vec<WatcherId>>,
    /// Reverse map from watcher ID to its patterns (for efficient unregister)
    watcher_patterns: HashMap<WatcherId, Vec<EventPattern>>,
}

impl WatcherEventBridge {
    /// Create a new empty bridge
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a watcher's wake_on patterns
    pub fn register(&mut self, watcher_id: WatcherId, patterns: Vec<String>) {
        let event_patterns: Vec<EventPattern> =
            patterns.into_iter().map(EventPattern::new).collect();

        // Store patterns for this watcher (for unregister)
        self.watcher_patterns
            .insert(watcher_id.clone(), event_patterns.clone());

        // Add watcher to each pattern's subscription list
        for pattern in event_patterns {
            self.subscriptions
                .entry(pattern)
                .or_default()
                .push(watcher_id.clone());
        }
    }

    /// Unregister a watcher
    pub fn unregister(&mut self, watcher_id: &WatcherId) {
        // Get patterns this watcher subscribed to
        if let Some(patterns) = self.watcher_patterns.remove(watcher_id) {
            // Remove watcher from each pattern's subscription list
            for pattern in patterns {
                if let Some(watchers) = self.subscriptions.get_mut(&pattern) {
                    watchers.retain(|id| id != watcher_id);
                    // Clean up empty entries
                    if watchers.is_empty() {
                        self.subscriptions.remove(&pattern);
                    }
                }
            }
        }
    }

    /// Get watchers that should wake for an event
    pub fn watchers_for_event(&self, event_name: &str) -> Vec<WatcherId> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (pattern, watchers) in &self.subscriptions {
            if pattern.matches(event_name) {
                for watcher in watchers {
                    if seen.insert(watcher.clone()) {
                        result.push(watcher.clone());
                    }
                }
            }
        }

        result
    }

    /// Check if there are any subscriptions
    pub fn is_empty(&self) -> bool {
        self.subscriptions.is_empty()
    }

    /// Get the number of registered watchers
    pub fn watcher_count(&self) -> usize {
        self.watcher_patterns.len()
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

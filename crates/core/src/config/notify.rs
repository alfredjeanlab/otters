// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Notification configuration
//!
//! Maps events to notifications based on pattern matching rules.

use crate::adapters::notify::{Notification, NotifyUrgency};
use crate::effect::Event;
use crate::events::EventPattern;

/// Configuration for which events trigger notifications
#[derive(Debug, Clone)]
pub struct NotifyConfig {
    /// Map from event pattern to notification template
    rules: Vec<NotifyRule>,
}

/// A rule mapping an event pattern to a notification
#[derive(Debug, Clone)]
pub struct NotifyRule {
    pub pattern: EventPattern,
    pub urgency: NotifyUrgency,
    /// If true, show notification. If false, suppress.
    pub enabled: bool,
}

impl NotifyConfig {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create default configuration with common notification rules
    pub fn default_config() -> Self {
        let mut config = Self::new();

        // Pipeline events - notify on completion and failure
        config.add_rule("pipeline:complete", NotifyUrgency::Normal, true);
        config.add_rule("pipeline:failed", NotifyUrgency::Important, true);

        // Task events - only notify on stuck and escalation
        config.add_rule("task:stuck", NotifyUrgency::Important, true);

        // Queue events - notify on dead letter
        config.add_rule("queue:item:deadlettered", NotifyUrgency::Important, true);

        config
    }

    /// Add a notification rule
    pub fn add_rule(&mut self, pattern: &str, urgency: NotifyUrgency, enabled: bool) {
        self.rules.push(NotifyRule {
            pattern: EventPattern::new(pattern),
            urgency,
            enabled,
        });
    }

    /// Check if an event should trigger a notification
    pub fn should_notify(&self, event: &Event) -> Option<NotifyUrgency> {
        let event_name = event.name();

        for rule in &self.rules {
            if rule.pattern.matches(&event_name) {
                if rule.enabled {
                    return Some(rule.urgency);
                } else {
                    return None;
                }
            }
        }

        None
    }

    /// Convert an event to a notification if configured
    pub fn to_notification(&self, event: &Event) -> Option<Notification> {
        let urgency = self.should_notify(event)?;
        Some(event_to_notification(event, urgency))
    }
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Convert an event to a notification
fn event_to_notification(event: &Event, urgency: NotifyUrgency) -> Notification {
    match event {
        Event::PipelineComplete { id } => {
            Notification::new("Pipeline Complete", format!("{} finished", id)).with_urgency(urgency)
        }
        Event::PipelineFailed { id, reason } => {
            Notification::new("Pipeline Failed", format!("{}: {}", id, reason))
                .with_urgency(urgency)
        }
        Event::TaskStuck { id, .. } => {
            Notification::new("Task Stuck", format!("Task {} needs attention", id.0))
                .with_urgency(urgency)
        }
        Event::QueueItemDeadLettered {
            queue,
            item_id,
            reason,
        } => Notification::new(
            "Dead Letter",
            format!("Item {} in {}: {}", item_id, queue, reason),
        )
        .with_urgency(urgency),
        // Default: use event name as title
        other => Notification::new(
            other.name().replace(':', " ").to_uppercase(),
            format!("{:?}", other),
        )
        .with_urgency(urgency),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskId;
    use std::time::Instant;

    #[test]
    fn default_config_notifies_on_pipeline_complete() {
        let config = NotifyConfig::default();
        let event = Event::PipelineComplete {
            id: "p-1".to_string(),
        };

        assert_eq!(config.should_notify(&event), Some(NotifyUrgency::Normal));
    }

    #[test]
    fn default_config_notifies_on_pipeline_failed() {
        let config = NotifyConfig::default();
        let event = Event::PipelineFailed {
            id: "p-1".to_string(),
            reason: "build error".to_string(),
        };

        assert_eq!(config.should_notify(&event), Some(NotifyUrgency::Important));
    }

    #[test]
    fn default_config_ignores_pipeline_phase() {
        let config = NotifyConfig::default();
        let event = Event::PipelinePhase {
            id: "p-1".to_string(),
            phase: "build".to_string(),
        };

        assert_eq!(config.should_notify(&event), None);
    }

    #[test]
    fn custom_rule_overrides() {
        let mut config = NotifyConfig::new();
        // Disable all pipeline notifications
        config.add_rule("pipeline:*", NotifyUrgency::Normal, false);

        let event = Event::PipelineComplete {
            id: "p-1".to_string(),
        };
        assert_eq!(config.should_notify(&event), None);
    }

    #[test]
    fn to_notification_creates_correct_message() {
        let config = NotifyConfig::default();

        let event = Event::PipelineFailed {
            id: "my-pipeline".to_string(),
            reason: "compilation error".to_string(),
        };

        let notification = config.to_notification(&event).unwrap();
        assert_eq!(notification.title, "Pipeline Failed");
        assert!(notification.message.contains("my-pipeline"));
        assert!(notification.message.contains("compilation error"));
        assert_eq!(notification.urgency, NotifyUrgency::Important);
    }

    #[test]
    fn to_notification_handles_stuck_task() {
        let config = NotifyConfig::default();

        let event = Event::TaskStuck {
            id: TaskId("task-123".to_string()),
            since: Instant::now(),
        };

        let notification = config.to_notification(&event).unwrap();
        assert_eq!(notification.title, "Task Stuck");
        assert!(notification.message.contains("task-123"));
    }
}

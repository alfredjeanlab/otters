// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Event bus for routing events to subscribers

use super::subscription::{SubscriberId, Subscription};
use crate::effect::Event;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

/// Sender for event delivery
pub type EventSender = mpsc::UnboundedSender<Event>;
/// Receiver for event delivery
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

/// The event bus routes events to matching subscribers
pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<SubscriberId, (Subscription, EventSender)>>>,
    /// Optional handler for all events (for logging)
    global_handler: Arc<RwLock<Option<EventSender>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            global_handler: Arc::new(RwLock::new(None)),
        }
    }

    /// Subscribe to events matching the given patterns
    /// Returns a receiver for events and the subscription ID
    pub fn subscribe(&self, subscription: Subscription) -> EventReceiver {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = subscription.id.clone();

        let mut subs = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        subs.insert(id, (subscription, tx));

        rx
    }

    /// Unsubscribe from events
    pub fn unsubscribe(&self, id: &SubscriberId) {
        let mut subs = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        subs.remove(id);
    }

    /// Set a global handler that receives all events (for logging)
    pub fn set_global_handler(&self) -> EventReceiver {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut handler = self
            .global_handler
            .write()
            .unwrap_or_else(|e| e.into_inner());
        *handler = Some(tx);
        rx
    }

    /// Publish an event to all matching subscribers
    pub fn publish(&self, event: Event) {
        let event_name = event.name();

        // Send to global handler first (for logging)
        if let Some(tx) = self
            .global_handler
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = tx.send(event.clone());
        }

        // Send to matching subscribers
        let subs = self.subscribers.read().unwrap_or_else(|e| e.into_inner());
        for (subscription, tx) in subs.values() {
            if subscription.matches(&event_name) {
                let _ = tx.send(event.clone());
            }
        }
    }

    /// Get count of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.subscribers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// List all subscription IDs
    pub fn list_subscriptions(&self) -> Vec<SubscriberId> {
        self.subscribers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            subscribers: Arc::clone(&self.subscribers),
            global_handler: Arc::clone(&self.global_handler),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventPattern;

    #[tokio::test]
    async fn publish_to_matching_subscribers() {
        let bus = EventBus::new();

        // Subscribe to pipeline events
        let sub = Subscription::new(
            "pipeline-sub",
            vec![EventPattern::new("pipeline:*")],
            "Pipeline events",
        );
        let mut rx = bus.subscribe(sub);

        // Publish matching event
        bus.publish(Event::PipelineComplete {
            id: "p-1".to_string(),
        });

        // Should receive the event
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, Event::PipelineComplete { id } if id == "p-1"));
    }

    #[tokio::test]
    async fn non_matching_events_not_delivered() {
        let bus = EventBus::new();

        // Subscribe only to pipeline events
        let sub = Subscription::new(
            "pipeline-sub",
            vec![EventPattern::new("pipeline:*")],
            "Pipeline events",
        );
        let mut rx = bus.subscribe(sub);

        // Publish non-matching event
        bus.publish(Event::TaskComplete {
            id: crate::task::TaskId("t-1".to_string()),
            output: None,
        });

        // Should not receive the event
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn global_handler_receives_all_events() {
        let bus = EventBus::new();

        let mut global_rx = bus.set_global_handler();

        // Publish various events
        bus.publish(Event::PipelineComplete {
            id: "p-1".to_string(),
        });
        bus.publish(Event::TaskComplete {
            id: crate::task::TaskId("t-1".to_string()),
            output: None,
        });

        // Global handler should receive both
        assert!(global_rx.try_recv().is_ok());
        assert!(global_rx.try_recv().is_ok());
    }

    #[test]
    fn unsubscribe_removes_subscriber() {
        let bus = EventBus::new();

        let sub = Subscription::new("test-sub", vec![EventPattern::new("*")], "Test");
        let _rx = bus.subscribe(sub);

        assert_eq!(bus.subscriber_count(), 1);

        bus.unsubscribe(&SubscriberId("test-sub".to_string()));
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn clone_shares_state() {
        let bus1 = EventBus::new();
        let bus2 = bus1.clone();

        let sub = Subscription::new("test-sub", vec![EventPattern::new("*")], "Test");
        let _rx = bus1.subscribe(sub);

        // Both should see the subscriber
        assert_eq!(bus1.subscriber_count(), 1);
        assert_eq!(bus2.subscriber_count(), 1);
    }
}

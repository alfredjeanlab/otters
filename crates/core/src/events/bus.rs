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
#[path = "bus_tests.rs"]
mod tests;

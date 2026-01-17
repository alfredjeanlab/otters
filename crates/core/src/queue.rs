//! Queue data structure for merge queue and other ordered work

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An item in a queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub data: HashMap<String, String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
}

impl QueueItem {
    /// Create a new queue item
    pub fn new(id: impl Into<String>, data: HashMap<String, String>) -> Self {
        Self {
            id: id.into(),
            data,
            priority: 0,
            created_at: Utc::now(),
            attempts: 0,
        }
    }

    /// Create a new queue item with priority
    pub fn with_priority(id: impl Into<String>, data: HashMap<String, String>, priority: i32) -> Self {
        Self {
            id: id.into(),
            data,
            priority,
            created_at: Utc::now(),
            attempts: 0,
        }
    }

    /// Increment the attempt counter
    pub fn with_incremented_attempts(&self) -> Self {
        Self {
            attempts: self.attempts + 1,
            ..self.clone()
        }
    }
}

/// A dead-lettered item that failed too many times
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetter {
    pub item: QueueItem,
    pub reason: String,
    pub dead_at: DateTime<Utc>,
}

/// A queue with priority ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queue {
    pub name: String,
    pub items: Vec<QueueItem>,
    pub processing: Option<QueueItem>,
    pub dead_letters: Vec<DeadLetter>,
}

impl Queue {
    /// Create a new empty queue
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            processing: None,
            dead_letters: Vec::new(),
        }
    }

    /// Push an item to the queue
    pub fn push(&self, item: QueueItem) -> Queue {
        let mut items = self.items.clone();
        items.push(item);
        // Sort by priority (descending) then by created_at (ascending)
        items.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
        Queue {
            items,
            ..self.clone()
        }
    }

    /// Take the next item for processing
    pub fn take(&self) -> (Queue, Option<QueueItem>) {
        if self.processing.is_some() || self.items.is_empty() {
            return (self.clone(), None);
        }

        let mut items = self.items.clone();
        let item = items.remove(0);

        (
            Queue {
                items,
                processing: Some(item.clone()),
                ..self.clone()
            },
            Some(item),
        )
    }

    /// Mark the current item as complete
    pub fn complete(&self, id: &str) -> Queue {
        if self.processing.as_ref().map(|i| i.id.as_str()) == Some(id) {
            Queue {
                processing: None,
                ..self.clone()
            }
        } else {
            self.clone()
        }
    }

    /// Requeue the current item for retry
    pub fn requeue(&self, item: QueueItem) -> Queue {
        let queue = Queue {
            processing: None,
            ..self.clone()
        };
        queue.push(item)
    }

    /// Move an item to the dead letter queue
    pub fn dead_letter(&self, item: QueueItem, reason: String) -> Queue {
        let mut dead_letters = self.dead_letters.clone();
        dead_letters.push(DeadLetter {
            item,
            reason,
            dead_at: Utc::now(),
        });
        Queue {
            processing: None,
            dead_letters,
            ..self.clone()
        }
    }

    /// Get the number of items waiting
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.processing.is_none()
    }

    /// Check if something is currently being processed
    pub fn is_processing(&self) -> bool {
        self.processing.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, priority: i32) -> QueueItem {
        QueueItem::with_priority(id, HashMap::new(), priority)
    }

    #[test]
    fn queue_starts_empty() {
        let queue = Queue::new("test");
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn queue_push_adds_items() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("item-1", 0));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn queue_orders_by_priority_then_time() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("low", 0));
        let queue = queue.push(make_item("high", 10));
        let queue = queue.push(make_item("medium", 5));

        let (queue, item) = queue.take();
        assert_eq!(item.unwrap().id, "high");

        let (queue, item) = queue.complete("high").take();
        assert_eq!(item.unwrap().id, "medium");

        let (_, item) = queue.complete("medium").take();
        assert_eq!(item.unwrap().id, "low");
    }

    #[test]
    fn queue_take_returns_none_when_processing() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("item-1", 0));
        let queue = queue.push(make_item("item-2", 0));

        let (queue, item1) = queue.take();
        assert!(item1.is_some());

        let (_, item2) = queue.take();
        assert!(item2.is_none()); // Can't take while processing
    }

    #[test]
    fn queue_complete_allows_next_take() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("item-1", 0));
        let queue = queue.push(make_item("item-2", 0));

        let (queue, _) = queue.take();
        let queue = queue.complete("item-1");

        let (_, item) = queue.take();
        assert_eq!(item.unwrap().id, "item-2");
    }

    #[test]
    fn queue_requeue_puts_item_back() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("item-1", 0));

        let (queue, item) = queue.take();
        let item = item.unwrap().with_incremented_attempts();
        let queue = queue.requeue(item);

        assert_eq!(queue.len(), 1);
        assert!(!queue.is_processing());

        let (_, item) = queue.take();
        assert_eq!(item.unwrap().attempts, 1);
    }

    #[test]
    fn queue_dead_letter_removes_from_processing() {
        let queue = Queue::new("test");
        let queue = queue.push(make_item("item-1", 0));

        let (queue, item) = queue.take();
        let queue = queue.dead_letter(item.unwrap(), "Too many failures".to_string());

        assert!(!queue.is_processing());
        assert_eq!(queue.dead_letters.len(), 1);
        assert_eq!(queue.dead_letters[0].reason, "Too many failures");
    }
}

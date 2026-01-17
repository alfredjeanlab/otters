//! Queue data structure with visibility timeout support
//!
//! A queue with priority ordering and claimed item management.
//! Claimed items have a visibility timeout - if not completed or released
//! within the timeout, they are automatically returned to the queue.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// An item in a queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub data: HashMap<String, String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
    pub max_attempts: u32,
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
            max_attempts: 3,
        }
    }

    /// Create a new queue item with priority
    pub fn with_priority(
        id: impl Into<String>,
        data: HashMap<String, String>,
        priority: i32,
    ) -> Self {
        Self {
            id: id.into(),
            data,
            priority,
            created_at: Utc::now(),
            attempts: 0,
            max_attempts: 3,
        }
    }

    /// Set max attempts
    pub fn with_max_attempts(self, max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..self
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

/// A claimed item with visibility timeout tracking
#[derive(Debug, Clone)]
pub struct ClaimedItem {
    pub item: QueueItem,
    pub claimed_at: Instant,
    pub visible_after: Instant,
    pub claim_id: String,
}

/// A dead-lettered item that failed too many times
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetter {
    pub item: QueueItem,
    pub reason: String,
    pub dead_at: DateTime<Utc>,
}

/// Events that can change queue state
#[derive(Clone, Debug)]
pub enum QueueEvent {
    /// Push a new item to the queue
    Push { item: QueueItem },
    /// Claim an item for processing
    Claim {
        claim_id: String,
        visibility_timeout: Option<Duration>,
    },
    /// Mark a claimed item as complete
    Complete { claim_id: String },
    /// Mark a claimed item as failed
    Fail { claim_id: String, reason: String },
    /// Release a claimed item back to the queue
    Release { claim_id: String },
    /// Check for expired claims (called periodically)
    Tick,
}

/// A queue with priority ordering and visibility timeout support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queue {
    pub name: String,
    pub items: Vec<QueueItem>,
    #[serde(skip, default)]
    pub claimed: Vec<ClaimedItem>,
    pub dead_letters: Vec<DeadLetter>,
    #[serde(with = "duration_secs", default = "default_visibility_timeout")]
    pub default_visibility_timeout: Duration,
    // Legacy field for backward compatibility
    #[serde(skip, default)]
    pub processing: Option<QueueItem>,
}

fn default_visibility_timeout() -> Duration {
    Duration::from_secs(300)
}

mod duration_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

impl Queue {
    /// Create a new empty queue
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            claimed: Vec::new(),
            dead_letters: Vec::new(),
            default_visibility_timeout: Duration::from_secs(300), // 5 minutes
            processing: None,
        }
    }

    /// Create a new queue with custom visibility timeout
    pub fn with_visibility_timeout(name: impl Into<String>, timeout: Duration) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            claimed: Vec::new(),
            dead_letters: Vec::new(),
            default_visibility_timeout: timeout,
            processing: None,
        }
    }

    /// Pure transition function - returns new state and effects
    pub fn transition(&self, event: QueueEvent, clock: &impl Clock) -> (Queue, Vec<Effect>) {
        let now = clock.now();

        match event {
            QueueEvent::Push { item } => {
                let mut items = self.items.clone();
                items.push(item.clone());
                self.sort_items(&mut items);

                let queue = Queue {
                    items,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::QueueItemAdded {
                    queue: self.name.clone(),
                    item_id: item.id.clone(),
                })];
                (queue, effects)
            }

            QueueEvent::Claim {
                claim_id,
                visibility_timeout,
            } => {
                if self.items.is_empty() {
                    return (self.clone(), vec![]);
                }

                let mut items = self.items.clone();
                let item = items.remove(0);
                let timeout = visibility_timeout.unwrap_or(self.default_visibility_timeout);

                let claimed_item = ClaimedItem {
                    item: item.clone(),
                    claimed_at: now,
                    visible_after: now + timeout,
                    claim_id: claim_id.clone(),
                };

                let mut claimed = self.claimed.clone();
                claimed.push(claimed_item);

                let queue = Queue {
                    items,
                    claimed,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::QueueItemClaimed {
                    queue: self.name.clone(),
                    item_id: item.id.clone(),
                    claim_id,
                })];
                (queue, effects)
            }

            QueueEvent::Complete { claim_id } => {
                let (completed, remaining): (Vec<_>, Vec<_>) = self
                    .claimed
                    .iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if completed.is_empty() {
                    return (self.clone(), vec![]);
                }

                let item_id = completed[0].item.id.clone();
                let queue = Queue {
                    claimed: remaining,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::QueueItemComplete {
                    queue: self.name.clone(),
                    item_id,
                })];
                (queue, effects)
            }

            QueueEvent::Fail { claim_id, reason } => {
                let (failed, remaining): (Vec<_>, Vec<_>) = self
                    .claimed
                    .iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if failed.is_empty() {
                    return (self.clone(), vec![]);
                }

                let mut item = failed[0].item.clone();
                item.attempts += 1;

                // Requeue or dead-letter based on attempts
                let (items, dead_letters, effects) = if item.attempts >= item.max_attempts {
                    let mut dead = self.dead_letters.clone();
                    dead.push(DeadLetter {
                        item: item.clone(),
                        reason: reason.clone(),
                        dead_at: Utc::now(),
                    });
                    (
                        self.items.clone(),
                        dead,
                        vec![Effect::Emit(Event::QueueItemDeadLettered {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason,
                        })],
                    )
                } else {
                    let mut items = self.items.clone();
                    items.push(item.clone());
                    self.sort_items(&mut items);
                    (
                        items,
                        self.dead_letters.clone(),
                        vec![Effect::Emit(Event::QueueItemFailed {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason,
                        })],
                    )
                };

                let queue = Queue {
                    items,
                    claimed: remaining,
                    dead_letters,
                    ..self.clone()
                };
                (queue, effects)
            }

            QueueEvent::Release { claim_id } => {
                let (released, remaining): (Vec<_>, Vec<_>) = self
                    .claimed
                    .iter()
                    .cloned()
                    .partition(|c| c.claim_id == claim_id);

                if released.is_empty() {
                    return (self.clone(), vec![]);
                }

                let item = released[0].item.clone();
                let mut items = self.items.clone();
                items.push(item.clone());
                self.sort_items(&mut items);

                let queue = Queue {
                    items,
                    claimed: remaining,
                    ..self.clone()
                };
                let effects = vec![Effect::Emit(Event::QueueItemReleased {
                    queue: self.name.clone(),
                    item_id: item.id.clone(),
                    reason: "explicit release".to_string(),
                })];
                (queue, effects)
            }

            QueueEvent::Tick => {
                // Find expired claims
                let (expired, active): (Vec<_>, Vec<_>) = self
                    .claimed
                    .iter()
                    .cloned()
                    .partition(|c| now >= c.visible_after);

                if expired.is_empty() {
                    return (self.clone(), vec![]);
                }

                // Return expired items to queue with incremented attempts
                let mut items = self.items.clone();
                let mut dead_letters = self.dead_letters.clone();
                let mut effects = vec![];

                for claim in &expired {
                    let mut item = claim.item.clone();
                    item.attempts += 1;

                    if item.attempts >= item.max_attempts {
                        dead_letters.push(DeadLetter {
                            item: item.clone(),
                            reason: "visibility timeout exceeded max attempts".to_string(),
                            dead_at: Utc::now(),
                        });
                        effects.push(Effect::Emit(Event::QueueItemDeadLettered {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason: "visibility timeout exceeded max attempts".to_string(),
                        }));
                    } else {
                        items.push(item.clone());
                        effects.push(Effect::Emit(Event::QueueItemReleased {
                            queue: self.name.clone(),
                            item_id: item.id.clone(),
                            reason: "visibility timeout".to_string(),
                        }));
                    }
                }

                self.sort_items(&mut items);

                let queue = Queue {
                    items,
                    claimed: active,
                    dead_letters,
                    ..self.clone()
                };
                (queue, effects)
            }
        }
    }

    /// Sort items by priority (descending) then by created_at (ascending)
    fn sort_items(&self, items: &mut [QueueItem]) {
        items.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
    }

    // Legacy methods for backward compatibility

    /// Push an item to the queue (legacy, prefer transition)
    pub fn push(&self, item: QueueItem) -> Queue {
        let mut items = self.items.clone();
        items.push(item);
        self.sort_items(&mut items);
        Queue {
            items,
            ..self.clone()
        }
    }

    /// Take the next item for processing (legacy, prefer transition with Claim)
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

    /// Mark the current item as complete (legacy)
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

    /// Requeue the current item for retry (legacy)
    pub fn requeue(&self, item: QueueItem) -> Queue {
        let queue = Queue {
            processing: None,
            ..self.clone()
        };
        queue.push(item)
    }

    /// Move an item to the dead letter queue (legacy)
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

    /// Get the number of available items (not claimed)
    pub fn available_count(&self) -> usize {
        self.items.len()
    }

    /// Get the number of claimed items
    pub fn claimed_count(&self) -> usize {
        self.claimed.len()
    }

    /// Get the number of items waiting (alias for available_count)
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.claimed.is_empty() && self.processing.is_none()
    }

    /// Check if something is currently being processed (legacy)
    pub fn is_processing(&self) -> bool {
        self.processing.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    fn make_item(id: &str, priority: i32) -> QueueItem {
        QueueItem::with_priority(id, HashMap::new(), priority)
    }

    // Legacy tests

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

    // New transition-based tests

    #[test]
    fn queue_transition_push_adds_and_sorts() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, effects) = queue.transition(
            QueueEvent::Push {
                item: make_item("low", 0),
            },
            &clock,
        );
        assert_eq!(queue.available_count(), 1);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemAdded { .. })
        ));

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("high", 10),
            },
            &clock,
        );
        assert_eq!(queue.items[0].id, "high");
    }

    #[test]
    fn queue_transition_claim_moves_to_claimed() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-1", 0),
            },
            &clock,
        );

        let (queue, effects) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        assert_eq!(queue.available_count(), 0);
        assert_eq!(queue.claimed_count(), 1);
        assert_eq!(queue.claimed[0].claim_id, "claim-1");
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemClaimed { .. })
        ));
    }

    #[test]
    fn queue_transition_claim_empty_is_no_op() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, effects) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        assert_eq!(queue.claimed_count(), 0);
        assert!(effects.is_empty());
    }

    #[test]
    fn queue_transition_complete_removes_from_claimed() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-1", 0),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        let (queue, effects) = queue.transition(
            QueueEvent::Complete {
                claim_id: "claim-1".to_string(),
            },
            &clock,
        );

        assert_eq!(queue.claimed_count(), 0);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemComplete { .. })
        ));
    }

    #[test]
    fn queue_transition_fail_requeues_item() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let mut item = make_item("item-1", 0);
        item.max_attempts = 3;

        let (queue, _) = queue.transition(QueueEvent::Push { item }, &clock);
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        let (queue, effects) = queue.transition(
            QueueEvent::Fail {
                claim_id: "claim-1".to_string(),
                reason: "error".to_string(),
            },
            &clock,
        );

        assert_eq!(queue.available_count(), 1);
        assert_eq!(queue.claimed_count(), 0);
        assert_eq!(queue.items[0].attempts, 1);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemFailed { .. })
        ));
    }

    #[test]
    fn queue_transition_fail_dead_letters_after_max_attempts() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let mut item = make_item("item-1", 0);
        item.max_attempts = 1;
        item.attempts = 0;

        let (queue, _) = queue.transition(QueueEvent::Push { item }, &clock);
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        let (queue, effects) = queue.transition(
            QueueEvent::Fail {
                claim_id: "claim-1".to_string(),
                reason: "error".to_string(),
            },
            &clock,
        );

        assert_eq!(queue.available_count(), 0);
        assert_eq!(queue.dead_letters.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemDeadLettered { .. })
        ));
    }

    #[test]
    fn queue_transition_release_returns_item() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-1", 0),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        let (queue, effects) = queue.transition(
            QueueEvent::Release {
                claim_id: "claim-1".to_string(),
            },
            &clock,
        );

        assert_eq!(queue.available_count(), 1);
        assert_eq!(queue.claimed_count(), 0);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemReleased { .. })
        ));
    }

    #[test]
    fn queue_transition_tick_expires_claims() {
        let clock = FakeClock::new();
        let queue = Queue::with_visibility_timeout("test", Duration::from_secs(60));

        let mut item = make_item("item-1", 0);
        item.max_attempts = 3;

        let (queue, _) = queue.transition(QueueEvent::Push { item }, &clock);
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: Some(Duration::from_secs(60)),
            },
            &clock,
        );

        assert_eq!(queue.claimed_count(), 1);

        // Advance past visibility timeout
        clock.advance(Duration::from_secs(120));

        let (queue, effects) = queue.transition(QueueEvent::Tick, &clock);

        assert_eq!(queue.available_count(), 1);
        assert_eq!(queue.claimed_count(), 0);
        assert_eq!(queue.items[0].attempts, 1);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemReleased { .. })
        ));
    }

    #[test]
    fn queue_transition_tick_dead_letters_expired_at_max() {
        let clock = FakeClock::new();
        let queue = Queue::with_visibility_timeout("test", Duration::from_secs(60));

        let mut item = make_item("item-1", 0);
        item.max_attempts = 1;
        item.attempts = 0;

        let (queue, _) = queue.transition(QueueEvent::Push { item }, &clock);
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: Some(Duration::from_secs(60)),
            },
            &clock,
        );

        // Advance past visibility timeout
        clock.advance(Duration::from_secs(120));

        let (queue, effects) = queue.transition(QueueEvent::Tick, &clock);

        assert_eq!(queue.available_count(), 0);
        assert_eq!(queue.claimed_count(), 0);
        assert_eq!(queue.dead_letters.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::Emit(Event::QueueItemDeadLettered { .. })
        ));
    }

    #[test]
    fn queue_transition_tick_no_op_when_no_expired() {
        let clock = FakeClock::new();
        let queue = Queue::with_visibility_timeout("test", Duration::from_secs(300));

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-1", 0),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        // Advance but not past timeout
        clock.advance(Duration::from_secs(60));

        let (queue, effects) = queue.transition(QueueEvent::Tick, &clock);

        assert_eq!(queue.claimed_count(), 1);
        assert!(effects.is_empty());
    }

    #[test]
    fn queue_multiple_claims_work_independently() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-1", 0),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("item-2", 0),
            },
            &clock,
        );

        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-2".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        assert_eq!(queue.available_count(), 0);
        assert_eq!(queue.claimed_count(), 2);

        let (queue, _) = queue.transition(
            QueueEvent::Complete {
                claim_id: "claim-1".to_string(),
            },
            &clock,
        );

        assert_eq!(queue.claimed_count(), 1);
        assert_eq!(queue.claimed[0].claim_id, "claim-2");
    }

    #[test]
    fn queue_claims_highest_priority_first() {
        let clock = FakeClock::new();
        let queue = Queue::new("test");

        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("low", 0),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("high", 10),
            },
            &clock,
        );
        let (queue, _) = queue.transition(
            QueueEvent::Push {
                item: make_item("medium", 5),
            },
            &clock,
        );

        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-1".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        assert_eq!(queue.claimed[0].item.id, "high");

        let (queue, _) = queue.transition(
            QueueEvent::Claim {
                claim_id: "claim-2".to_string(),
                visibility_timeout: None,
            },
            &clock,
        );

        assert_eq!(queue.claimed[1].item.id, "medium");
    }

    // Parametrized tests with yare
    mod yare_tests {
        use super::*;
        use yare::parameterized;

        #[parameterized(
            empty_claim_returns_none = { 0, 0 },
            single_item_claims = { 1, 1 },
            multiple_items_claims_one = { 3, 1 },
        )]
        fn queue_claim_count(num_items: usize, expected_claimed: usize) {
            let clock = FakeClock::new();
            let mut queue = Queue::new("test");

            for i in 0..num_items {
                let (q, _) = queue.transition(
                    QueueEvent::Push {
                        item: make_item(&format!("item-{}", i), 0),
                    },
                    &clock,
                );
                queue = q;
            }

            let (queue, _) = queue.transition(
                QueueEvent::Claim {
                    claim_id: "claim-1".to_string(),
                    visibility_timeout: None,
                },
                &clock,
            );

            assert_eq!(queue.claimed_count(), expected_claimed);
        }

        #[parameterized(
            priority_10_before_5 = { 10, 5, "high" },
            priority_5_before_0 = { 5, 0, "high" },
            priority_0_before_neg5 = { 0, -5, "high" },
            same_priority_fifo = { 0, 0, "low" },
        )]
        fn queue_claims_by_priority(high_priority: i32, low_priority: i32, expected_first: &str) {
            let clock = FakeClock::new();
            let mut queue = Queue::new("test");

            // Push low priority first
            let (q, _) = queue.transition(
                QueueEvent::Push {
                    item: make_item("low", low_priority),
                },
                &clock,
            );
            queue = q;

            // Push high priority second
            let (q, _) = queue.transition(
                QueueEvent::Push {
                    item: make_item("high", high_priority),
                },
                &clock,
            );
            queue = q;

            // Claim should get highest priority
            let (queue, _) = queue.transition(
                QueueEvent::Claim {
                    claim_id: "claim-1".to_string(),
                    visibility_timeout: None,
                },
                &clock,
            );

            assert_eq!(queue.claimed[0].item.id, expected_first);
        }

        #[parameterized(
            fail_once_requeues = { 1, 3, 1, 0 },
            fail_twice_requeues = { 2, 3, 1, 0 },
            fail_at_max_dead_letters = { 3, 3, 0, 1 },
            fail_at_max_single = { 1, 1, 0, 1 },
        )]
        fn queue_fail_behavior(
            fail_count: u32,
            max_attempts: u32,
            expected_available: usize,
            expected_dead: usize,
        ) {
            let clock = FakeClock::new();
            let mut item = make_item("test-item", 0);
            item.max_attempts = max_attempts;

            let mut queue = Queue::new("test");
            let (q, _) = queue.transition(QueueEvent::Push { item }, &clock);
            queue = q;

            // Fail the specified number of times
            for i in 0..fail_count {
                let (q, _) = queue.transition(
                    QueueEvent::Claim {
                        claim_id: format!("claim-{}", i),
                        visibility_timeout: None,
                    },
                    &clock,
                );
                queue = q;

                let (q, _) = queue.transition(
                    QueueEvent::Fail {
                        claim_id: format!("claim-{}", i),
                        reason: "test failure".to_string(),
                    },
                    &clock,
                );
                queue = q;
            }

            assert_eq!(queue.available_count(), expected_available);
            assert_eq!(queue.dead_letters.len(), expected_dead);
        }

        #[parameterized(
            release_returns_item = { "release", 1, 0 },
            complete_removes_item = { "complete", 0, 0 },
        )]
        fn queue_claim_resolution(resolution: &str, expected_available: usize, expected_claimed: usize) {
            let clock = FakeClock::new();
            let mut queue = Queue::new("test");

            let (q, _) = queue.transition(
                QueueEvent::Push {
                    item: make_item("item-1", 0),
                },
                &clock,
            );
            queue = q;

            let (q, _) = queue.transition(
                QueueEvent::Claim {
                    claim_id: "claim-1".to_string(),
                    visibility_timeout: None,
                },
                &clock,
            );
            queue = q;

            let event = match resolution {
                "release" => QueueEvent::Release {
                    claim_id: "claim-1".to_string(),
                },
                "complete" => QueueEvent::Complete {
                    claim_id: "claim-1".to_string(),
                },
                _ => panic!("Unknown resolution: {}", resolution),
            };

            let (queue, _) = queue.transition(event, &clock);

            assert_eq!(queue.available_count(), expected_available);
            assert_eq!(queue.claimed_count(), expected_claimed);
        }
    }

    // Property-based tests
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn arb_priority() -> impl Strategy<Value = i32> {
            -100..100i32
        }

        fn arb_item() -> impl Strategy<Value = QueueItem> {
            (any::<u32>(), arb_priority()).prop_map(|(id, priority)| {
                QueueItem::with_priority(format!("item-{}", id), HashMap::new(), priority)
            })
        }

        proptest! {
            #[test]
            fn queue_items_sorted_by_priority(items in proptest::collection::vec(arb_item(), 0..20)) {
                let clock = FakeClock::new();
                let mut queue = Queue::new("test");

                for item in items.iter() {
                    let (q, _) = queue.transition(QueueEvent::Push { item: item.clone() }, &clock);
                    queue = q;
                }

                // Verify items are sorted by priority descending
                for i in 1..queue.items.len() {
                    prop_assert!(
                        queue.items[i - 1].priority >= queue.items[i].priority,
                        "Items not sorted by priority"
                    );
                }
            }

            #[test]
            fn queue_push_claim_complete_preserves_count(
                items in proptest::collection::vec(arb_item(), 1..10)
            ) {
                let clock = FakeClock::new();
                let mut queue = Queue::new("test");

                // Push all items
                for item in items.iter() {
                    let (q, _) = queue.transition(QueueEvent::Push { item: item.clone() }, &clock);
                    queue = q;
                }

                let total = items.len();

                // Claim all items
                let mut claim_ids = vec![];
                for i in 0..total {
                    let (q, _) = queue.transition(
                        QueueEvent::Claim {
                            claim_id: format!("claim-{}", i),
                            visibility_timeout: None,
                        },
                        &clock,
                    );
                    queue = q;
                    claim_ids.push(format!("claim-{}", i));
                }

                prop_assert_eq!(queue.available_count(), 0);
                prop_assert_eq!(queue.claimed_count(), total);

                // Complete all items
                for claim_id in claim_ids {
                    let (q, _) = queue.transition(QueueEvent::Complete { claim_id }, &clock);
                    queue = q;
                }

                prop_assert_eq!(queue.available_count(), 0);
                prop_assert_eq!(queue.claimed_count(), 0);
            }

            #[test]
            fn queue_failed_items_requeue_or_dead_letter(
                max_attempts in 1..5u32
            ) {
                let clock = FakeClock::new();
                let mut item = QueueItem::new("test-item", HashMap::new());
                item.max_attempts = max_attempts;

                let mut queue = Queue::new("test");
                let (q, _) = queue.transition(QueueEvent::Push { item }, &clock);
                queue = q;

                // Fail the item max_attempts times
                for i in 0..max_attempts {
                    let (q, _) = queue.transition(
                        QueueEvent::Claim {
                            claim_id: format!("claim-{}", i),
                            visibility_timeout: None,
                        },
                        &clock,
                    );
                    queue = q;

                    let (q, _) = queue.transition(
                        QueueEvent::Fail {
                            claim_id: format!("claim-{}", i),
                            reason: "test failure".to_string(),
                        },
                        &clock,
                    );
                    queue = q;
                }

                // After max_attempts, item should be dead-lettered
                prop_assert_eq!(queue.available_count(), 0);
                prop_assert_eq!(queue.dead_letters.len(), 1);
            }
        }
    }
}

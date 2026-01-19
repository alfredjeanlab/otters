// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Queue data structure with visibility timeout support
//!
//! A queue with priority ordering and claimed item management.
//! Claimed items have a visibility timeout - if not completed or released
//! within the timeout, they are automatically returned to the queue.

use crate::clock::Clock;
use crate::effect::{Effect, Event};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// An item in a queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub data: BTreeMap<String, String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
    pub max_attempts: u32,
}

impl QueueItem {
    /// Create a new queue item
    pub fn new(id: impl Into<String>, data: BTreeMap<String, String>) -> Self {
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
        data: BTreeMap<String, String>,
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
#[path = "queue_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Timer-based scheduling for periodic tasks

use crate::clock::Clock;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::time::{Duration, Instant};

/// A scheduled item
#[derive(Debug, Clone)]
pub struct ScheduledItem {
    pub id: String,
    pub fire_at: Instant,
    pub kind: ScheduledKind,
    pub repeat: Option<Duration>,
}

/// The kind of scheduled event
#[derive(Debug, Clone)]
pub enum ScheduledKind {
    /// Evaluate all tasks for stuck detection
    TaskTick,
    /// Evaluate queue for visibility timeout
    QueueTick { queue_name: String },
    /// Custom timer
    Timer { id: String },
    /// Heartbeat check for sessions
    HeartbeatPoll,
}

impl PartialEq for ScheduledItem {
    fn eq(&self, other: &Self) -> bool {
        self.fire_at == other.fire_at && self.id == other.id
    }
}

impl Eq for ScheduledItem {}

impl PartialOrd for ScheduledItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: earliest first
        Reverse(self.fire_at).cmp(&Reverse(other.fire_at))
    }
}

/// Manages scheduled events
pub struct Scheduler {
    items: BinaryHeap<ScheduledItem>,
    cancelled: HashSet<String>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            items: BinaryHeap::new(),
            cancelled: HashSet::new(),
        }
    }

    /// Schedule a one-shot timer
    pub fn schedule(&mut self, id: impl Into<String>, fire_at: Instant, kind: ScheduledKind) {
        self.items.push(ScheduledItem {
            id: id.into(),
            fire_at,
            kind,
            repeat: None,
        });
    }

    /// Schedule a repeating timer
    pub fn schedule_repeating(
        &mut self,
        id: impl Into<String>,
        fire_at: Instant,
        interval: Duration,
        kind: ScheduledKind,
    ) {
        self.items.push(ScheduledItem {
            id: id.into(),
            fire_at,
            kind,
            repeat: Some(interval),
        });
    }

    /// Cancel a scheduled item
    pub fn cancel(&mut self, id: &str) {
        self.cancelled.insert(id.to_string());
    }

    /// Get all items that should fire at or before the given time
    pub fn poll(&mut self, now: Instant) -> Vec<ScheduledItem> {
        let mut ready = Vec::new();

        while let Some(item) = self.items.peek() {
            if item.fire_at > now {
                break;
            }

            let Some(item) = self.items.pop() else {
                break;
            };

            // Skip cancelled items
            if self.cancelled.contains(&item.id) {
                self.cancelled.remove(&item.id);
                continue;
            }

            // Re-schedule if repeating
            if let Some(interval) = item.repeat {
                self.items.push(ScheduledItem {
                    fire_at: item.fire_at + interval,
                    ..item.clone()
                });
            }

            ready.push(item);
        }

        ready
    }

    /// Initialize default schedules
    pub fn init_defaults(&mut self, clock: &impl Clock) {
        let now = clock.now();

        // Task tick every 30 seconds
        self.schedule_repeating(
            "task-tick",
            now + Duration::from_secs(30),
            Duration::from_secs(30),
            ScheduledKind::TaskTick,
        );

        // Queue tick every 10 seconds
        self.schedule_repeating(
            "queue-tick-merges",
            now + Duration::from_secs(10),
            Duration::from_secs(10),
            ScheduledKind::QueueTick {
                queue_name: "merges".to_string(),
            },
        );

        // Heartbeat poll every 5 seconds
        self.schedule_repeating(
            "heartbeat-poll",
            now + Duration::from_secs(5),
            Duration::from_secs(5),
            ScheduledKind::HeartbeatPoll,
        );
    }

    /// Check if scheduler has any pending items
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the next fire time, if any
    pub fn next_fire_time(&self) -> Option<Instant> {
        self.items.peek().map(|item| item.fire_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    #[test]
    fn scheduler_fires_items_at_correct_time() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        let now = clock.now();
        scheduler.schedule(
            "item-1",
            now + Duration::from_secs(10),
            ScheduledKind::TaskTick,
        );
        scheduler.schedule(
            "item-2",
            now + Duration::from_secs(5),
            ScheduledKind::TaskTick,
        );

        // Nothing ready yet
        let ready = scheduler.poll(now);
        assert!(ready.is_empty());

        // Advance 5 seconds - item-2 should be ready
        clock.advance(Duration::from_secs(5));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "item-2");

        // Advance 5 more seconds - item-1 should be ready
        clock.advance(Duration::from_secs(5));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "item-1");
    }

    #[test]
    fn scheduler_repeating_timers_reschedule() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        let now = clock.now();
        scheduler.schedule_repeating(
            "repeat",
            now + Duration::from_secs(10),
            Duration::from_secs(10),
            ScheduledKind::TaskTick,
        );

        // Fire first time
        clock.advance(Duration::from_secs(10));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "repeat");

        // Should have rescheduled
        assert!(!scheduler.is_empty());

        // Fire second time
        clock.advance(Duration::from_secs(10));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "repeat");
    }

    #[test]
    fn scheduler_cancel_prevents_firing() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        let now = clock.now();
        scheduler.schedule(
            "item-1",
            now + Duration::from_secs(10),
            ScheduledKind::TaskTick,
        );

        // Cancel it
        scheduler.cancel("item-1");

        // Advance past fire time
        clock.advance(Duration::from_secs(15));
        let ready = scheduler.poll(clock.now());
        assert!(ready.is_empty());
    }

    #[test]
    fn scheduler_init_defaults_creates_timers() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        scheduler.init_defaults(&clock);

        assert!(!scheduler.is_empty());

        // Should have next fire time
        assert!(scheduler.next_fire_time().is_some());
    }

    #[test]
    fn scheduler_multiple_items_fire_in_order() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        let now = clock.now();
        scheduler.schedule("a", now + Duration::from_secs(30), ScheduledKind::TaskTick);
        scheduler.schedule("b", now + Duration::from_secs(10), ScheduledKind::TaskTick);
        scheduler.schedule("c", now + Duration::from_secs(20), ScheduledKind::TaskTick);

        // Advance past all
        clock.advance(Duration::from_secs(35));
        let ready = scheduler.poll(clock.now());

        assert_eq!(ready.len(), 3);
        assert_eq!(ready[0].id, "b");
        assert_eq!(ready[1].id, "c");
        assert_eq!(ready[2].id, "a");
    }

    #[test]
    fn scheduler_cancel_repeating_stops_future_fires() {
        let clock = FakeClock::new();
        let mut scheduler = Scheduler::new();

        let now = clock.now();
        scheduler.schedule_repeating(
            "repeat",
            now + Duration::from_secs(10),
            Duration::from_secs(10),
            ScheduledKind::TaskTick,
        );

        // Fire first time
        clock.advance(Duration::from_secs(10));
        let ready = scheduler.poll(clock.now());
        assert_eq!(ready.len(), 1);

        // Cancel before next fire
        scheduler.cancel("repeat");

        // Advance - should not fire
        clock.advance(Duration::from_secs(10));
        let ready = scheduler.poll(clock.now());
        assert!(ready.is_empty());
    }
}

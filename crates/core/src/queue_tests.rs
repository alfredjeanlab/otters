use super::*;
use crate::clock::FakeClock;
use std::collections::BTreeMap;

fn make_item(id: &str, priority: i32) -> QueueItem {
    QueueItem::with_priority(id, BTreeMap::new(), priority)
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

// Property-based tests
use proptest::prelude::*;

fn arb_priority() -> impl Strategy<Value = i32> {
    -100..100i32
}

fn arb_item() -> impl Strategy<Value = QueueItem> {
    (any::<u32>(), arb_priority()).prop_map(|(id, priority)| {
        QueueItem::with_priority(format!("item-{}", id), BTreeMap::new(), priority)
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
        let mut item = QueueItem::new("test-item", BTreeMap::new());
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

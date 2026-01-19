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

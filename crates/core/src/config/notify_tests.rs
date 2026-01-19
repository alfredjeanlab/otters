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

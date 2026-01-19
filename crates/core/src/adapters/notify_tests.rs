use super::*;

#[test]
fn notification_builder() {
    let n = Notification::new("Title", "Message")
        .with_subtitle("Subtitle")
        .important();

    assert_eq!(n.title, "Title");
    assert_eq!(n.message, "Message");
    assert_eq!(n.subtitle, Some("Subtitle".to_string()));
    assert_eq!(n.urgency, NotifyUrgency::Important);
}

#[test]
fn osascript_builds_simple_script() {
    let notifier = OsascriptNotifier::new("test");
    let notification = Notification::new("Test Title", "Test message");

    let script = notifier.build_script(&notification);
    assert!(script.contains("Test Title"));
    assert!(script.contains("Test message"));
    assert!(!script.contains("sound name")); // Normal urgency = no sound
}

#[test]
fn osascript_builds_script_with_subtitle() {
    let notifier = OsascriptNotifier::new("test");
    let notification = Notification::new("Title", "Message").with_subtitle("Sub");

    let script = notifier.build_script(&notification);
    assert!(script.contains("subtitle \"Sub\""));
}

#[test]
fn osascript_builds_script_with_sound() {
    let notifier = OsascriptNotifier::new("test");

    let important = Notification::new("Title", "Message").important();
    let script = notifier.build_script(&important);
    assert!(script.contains("sound name \"default\""));

    let critical = Notification::new("Title", "Message").critical();
    let script = notifier.build_script(&critical);
    assert!(script.contains("sound name \"Sosumi\""));
}

#[test]
fn escape_applescript_handles_special_chars() {
    assert_eq!(escape_applescript("hello"), "hello");
    assert_eq!(escape_applescript("say \"hello\""), "say \\\"hello\\\"");
    assert_eq!(escape_applescript("path\\to\\file"), "path\\\\to\\\\file");
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Notification adapter for desktop notifications

use async_trait::async_trait;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("notification failed: {0}")]
    Failed(String),
    #[error("osascript error: {0}")]
    Osascript(String),
}

/// Notification urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyUrgency {
    /// Normal notification (no sound)
    Normal,
    /// Important notification (default sound)
    Important,
    /// Critical notification (alert sound, stays visible)
    Critical,
}

/// A notification to display
#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub subtitle: Option<String>,
    pub message: String,
    pub urgency: NotifyUrgency,
}

impl Notification {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            message: message.into(),
            urgency: NotifyUrgency::Normal,
        }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn with_urgency(mut self, urgency: NotifyUrgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn important(mut self) -> Self {
        self.urgency = NotifyUrgency::Important;
        self
    }

    pub fn critical(mut self) -> Self {
        self.urgency = NotifyUrgency::Critical;
        self
    }
}

/// Adapter trait for notification delivery
#[async_trait]
pub trait NotifyAdapter: Clone + Send + Sync + 'static {
    /// Send a notification
    async fn notify(&self, notification: Notification) -> Result<(), NotifyError>;
}

/// macOS notification via osascript
#[derive(Clone, Debug, Default)]
pub struct OsascriptNotifier {
    #[allow(dead_code)] // Reserved: may be used for terminal-notifier integration
    app_name: String,
}

impl OsascriptNotifier {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
        }
    }

    fn build_script(&self, notification: &Notification) -> String {
        let mut script = format!(
            r#"display notification "{}" with title "{}""#,
            escape_applescript(&notification.message),
            escape_applescript(&notification.title),
        );

        if let Some(subtitle) = &notification.subtitle {
            script.push_str(&format!(r#" subtitle "{}""#, escape_applescript(subtitle)));
        }

        // Add sound for important/critical notifications
        match notification.urgency {
            NotifyUrgency::Normal => {}
            NotifyUrgency::Important => {
                script.push_str(r#" sound name "default""#);
            }
            NotifyUrgency::Critical => {
                script.push_str(r#" sound name "Sosumi""#);
            }
        }

        script
    }
}

#[async_trait]
impl NotifyAdapter for OsascriptNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), NotifyError> {
        let script = self.build_script(&notification);

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| NotifyError::Failed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NotifyError::Osascript(stderr.to_string()));
        }

        Ok(())
    }
}

/// Escape special characters for AppleScript strings
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Fake notification adapter for testing
#![cfg_attr(coverage_nightly, coverage(off))]

use super::{NotifyAdapter, NotifyError};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Recorded notification
#[derive(Debug, Clone)]
pub struct NotifyCall {
    pub channel: String,
    pub message: String,
}

/// Fake notification adapter for testing
#[derive(Clone, Default)]
pub struct FakeNotifyAdapter {
    calls: Arc<Mutex<Vec<NotifyCall>>>,
}

impl FakeNotifyAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all recorded notifications
    pub fn calls(&self) -> Vec<NotifyCall> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[async_trait]
impl NotifyAdapter for FakeNotifyAdapter {
    async fn send(&self, channel: &str, message: &str) -> Result<(), NotifyError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(NotifyCall {
                channel: channel.to_string(),
                message: message.to_string(),
            });
        Ok(())
    }
}

#[cfg(test)]
#[path = "fake_tests.rs"]
mod tests;

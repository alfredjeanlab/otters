// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Worker state machine

use crate::clock::Clock;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Worker status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerStatus {
    /// Worker is not running
    Stopped,
    /// Worker is idle, waiting for work
    Idle,
    /// Worker is processing a pipeline
    Processing,
}

/// A worker instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub name: String,
    pub status: WorkerStatus,
    pub current_pipeline: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub last_active: Instant,
}

impl Worker {
    /// Create a new worker
    pub fn new(name: String, clock: &impl Clock) -> Self {
        Self {
            name,
            status: WorkerStatus::Stopped,
            current_pipeline: None,
            last_active: clock.now(),
        }
    }

    /// Start the worker
    pub fn start(&mut self, clock: &impl Clock) {
        self.status = WorkerStatus::Idle;
        self.last_active = clock.now();
    }

    /// Stop the worker
    pub fn stop(&mut self) {
        self.status = WorkerStatus::Stopped;
        self.current_pipeline = None;
    }

    /// Begin processing a pipeline
    pub fn begin_processing(&mut self, pipeline_id: String, clock: &impl Clock) {
        self.status = WorkerStatus::Processing;
        self.current_pipeline = Some(pipeline_id);
        self.last_active = clock.now();
    }

    /// Finish processing the current pipeline
    pub fn finish_processing(&mut self, clock: &impl Clock) {
        self.status = WorkerStatus::Idle;
        self.current_pipeline = None;
        self.last_active = clock.now();
    }

    /// Check if the worker is available to process work
    pub fn is_available(&self) -> bool {
        self.status == WorkerStatus::Idle
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

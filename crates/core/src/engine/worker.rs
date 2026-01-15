// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Merge queue worker daemon

use crate::adapters::RepoAdapter;
use crate::effect::MergeStrategy;
use crate::engine::executor::Adapters;
use crate::events::{EventBus, EventPattern, EventReceiver, Subscription};
use crate::storage::JsonStore;
use std::path::Path;
use thiserror::Error;
use tokio::time::{sleep, Duration};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("repo error: {0}")]
    Repo(#[from] crate::adapters::RepoError),
    #[error("merge conflict on branch: {0}")]
    MergeConflict(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

/// Worker that processes the merge queue
pub struct MergeWorker<A: Adapters> {
    adapters: A,
    store: JsonStore,
    queue_name: String,
    max_attempts: u32,
}

impl<A: Adapters> MergeWorker<A> {
    pub fn new(adapters: A, store: JsonStore) -> Self {
        Self {
            adapters,
            store,
            queue_name: "merges".to_string(),
            max_attempts: 2,
        }
    }

    /// Run the worker once, processing a single item if available
    pub async fn run_once(&self) -> Result<bool, WorkerError> {
        let queue = self.store.load_queue(&self.queue_name)?;
        let (queue, item) = queue.take();

        let Some(item) = item else {
            return Ok(false); // Nothing to process
        };

        self.store.save_queue(&self.queue_name, &queue)?;

        let branch = item
            .data
            .get("branch")
            .ok_or(WorkerError::MissingField("branch"))?
            .clone();

        let result = self.try_merge(&branch).await;

        match result {
            Ok(()) => {
                let queue = queue.complete(&item.id);
                self.store.save_queue(&self.queue_name, &queue)?;
                tracing::info!(branch = %branch, "merge completed successfully");
            }
            Err(e) => {
                let queue = if item.attempts < self.max_attempts {
                    tracing::warn!(branch = %branch, attempts = item.attempts + 1, "merge failed, requeueing");
                    queue.requeue(item.with_incremented_attempts())
                } else {
                    tracing::error!(branch = %branch, error = %e, "merge failed permanently");
                    queue.dead_letter(item, e.to_string())
                };
                self.store.save_queue(&self.queue_name, &queue)?;
            }
        }

        Ok(true)
    }

    /// Try to merge a branch using fast-forward, then rebase
    async fn try_merge(&self, branch: &str) -> Result<(), WorkerError> {
        let path = Path::new(".");

        // Try fast-forward first
        match self
            .adapters
            .repos()
            .merge(path, branch, MergeStrategy::FastForward)
            .await
        {
            Ok(crate::adapters::MergeResult::FastForwarded)
            | Ok(crate::adapters::MergeResult::Success) => return Ok(()),
            Ok(crate::adapters::MergeResult::Conflict { .. }) => {}
            Ok(crate::adapters::MergeResult::Rebased) => return Ok(()),
            Err(_) => {} // Try rebase
        }

        // Try rebase
        match self
            .adapters
            .repos()
            .merge(path, branch, MergeStrategy::Rebase)
            .await
        {
            Ok(crate::adapters::MergeResult::Rebased) => return Ok(()),
            Ok(crate::adapters::MergeResult::Conflict { .. }) => {
                return Err(WorkerError::MergeConflict(branch.to_string()))
            }
            _ => {}
        }

        Err(WorkerError::MergeConflict(branch.to_string()))
    }

    /// Run the worker continuously
    pub async fn run(&self, poll_interval: Duration) -> Result<(), WorkerError> {
        loop {
            match self.run_once().await {
                Ok(true) => {
                    // Processed an item, check for more immediately
                    continue;
                }
                Ok(false) => {
                    // Nothing to process, wait before checking again
                    sleep(poll_interval).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "worker error");
                    sleep(poll_interval).await;
                }
            }
        }
    }
}

// =============================================================================
// Event-Driven Worker Configuration
// =============================================================================

/// Configuration for an event-driven worker
#[derive(Clone, Debug)]
pub struct WorkerConfig {
    /// Worker identifier
    pub id: String,
    /// Queue to process
    pub queue_name: String,
    /// Events that wake this worker
    pub wake_on: Vec<EventPattern>,
    /// Fallback poll interval (for reliability)
    pub poll_interval: Duration,
    /// Visibility timeout for claimed items
    pub visibility_timeout: Duration,
}

impl WorkerConfig {
    pub fn new(id: impl Into<String>, queue_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            queue_name: queue_name.into(),
            wake_on: vec![],
            poll_interval: Duration::from_secs(30),
            visibility_timeout: Duration::from_secs(300),
        }
    }

    /// Add event patterns that wake this worker
    pub fn with_wake_on(mut self, patterns: Vec<&str>) -> Self {
        self.wake_on = patterns.into_iter().map(EventPattern::new).collect();
        self
    }

    /// Set fallback poll interval
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set visibility timeout
    pub fn with_visibility_timeout(mut self, timeout: Duration) -> Self {
        self.visibility_timeout = timeout;
        self
    }
}

/// Why the worker woke up
#[derive(Debug)]
pub enum WakeReason {
    /// Woke due to an event
    Event(String),
    /// Woke due to poll timer
    Poll,
}

/// Worker that processes queue items, waking on events
pub struct EventDrivenWorker<A: Adapters> {
    config: WorkerConfig,
    adapters: A,
    store: JsonStore,
    event_rx: EventReceiver,
}

impl<A: Adapters> EventDrivenWorker<A> {
    pub fn new(config: WorkerConfig, adapters: A, store: JsonStore, event_bus: &EventBus) -> Self {
        // Subscribe to wake-on events
        let subscription = Subscription::new(
            &config.id,
            config.wake_on.clone(),
            format!("Worker {} wake-on", config.id),
        );
        let event_rx = event_bus.subscribe(subscription);

        Self {
            config,
            adapters,
            store,
            event_rx,
        }
    }

    /// Get the worker ID
    pub fn id(&self) -> &str {
        &self.config.id
    }

    /// Get the queue name
    pub fn queue_name(&self) -> &str {
        &self.config.queue_name
    }

    /// Run the worker loop
    pub async fn run<F, Fut>(&mut self, process_fn: F) -> Result<(), WorkerError>
    where
        F: Fn(&crate::queue::QueueItem) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<(), WorkerError>> + Send,
    {
        loop {
            // Wait for wake event or poll timeout
            let wake_reason = tokio::select! {
                event = self.event_rx.recv() => {
                    match event {
                        Some(e) => WakeReason::Event(e.name()),
                        None => break, // Channel closed
                    }
                }
                _ = sleep(self.config.poll_interval) => {
                    WakeReason::Poll
                }
            };

            tracing::debug!(worker = %self.config.id, ?wake_reason, "worker woke");

            // Process available work
            self.process_available(&process_fn).await?;
        }

        Ok(())
    }

    /// Process all available queue items
    async fn process_available<F, Fut>(&mut self, process_fn: &F) -> Result<(), WorkerError>
    where
        F: Fn(&crate::queue::QueueItem) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<(), WorkerError>> + Send,
    {
        use crate::clock::SystemClock;

        loop {
            let queue = self.store.load_queue(&self.config.queue_name)?;

            if queue.items.is_empty() {
                break;
            }

            // Claim an item
            let claim_id = format!("{}-{}", self.config.id, uuid::Uuid::new_v4());
            let clock = SystemClock;
            let (new_queue, _effects) = queue.transition(
                crate::queue::QueueEvent::Claim {
                    claim_id: claim_id.clone(),
                    visibility_timeout: Some(self.config.visibility_timeout),
                },
                &clock,
            );
            self.store.save_queue(&self.config.queue_name, &new_queue)?;

            // Process the claimed item
            if let Some(claimed) = new_queue.claimed.iter().find(|c| c.claim_id == claim_id) {
                match process_fn(&claimed.item).await {
                    Ok(()) => {
                        let (q, _) = new_queue
                            .transition(crate::queue::QueueEvent::Complete { claim_id }, &clock);
                        self.store.save_queue(&self.config.queue_name, &q)?;
                    }
                    Err(e) => {
                        let (q, _) = new_queue.transition(
                            crate::queue::QueueEvent::Fail {
                                claim_id,
                                reason: e.to_string(),
                            },
                            &clock,
                        );
                        self.store.save_queue(&self.config.queue_name, &q)?;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Create an event-driven merge worker
pub fn new_event_driven_merge_worker<A: Adapters>(
    adapters: A,
    store: JsonStore,
    event_bus: &EventBus,
) -> EventDrivenWorker<A> {
    let config = WorkerConfig::new("merge-worker", "merges")
        .with_wake_on(vec!["queue:item:added"])
        .with_poll_interval(Duration::from_secs(60));

    EventDrivenWorker::new(config, adapters, store, event_bus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FakeAdapters;
    use crate::queue::{Queue, QueueItem};
    use std::collections::HashMap;

    #[tokio::test]
    async fn worker_returns_false_when_queue_empty() {
        let adapters = FakeAdapters::new();
        let store = JsonStore::open_temp().unwrap();
        store.save_queue("merges", &Queue::new("merges")).unwrap();

        let worker = MergeWorker::new(adapters, store);
        let result = worker.run_once().await.unwrap();

        assert!(!result);
    }

    #[tokio::test]
    async fn worker_processes_queue_items() {
        let adapters = FakeAdapters::new();
        let store = JsonStore::open_temp().unwrap();

        let mut data = HashMap::new();
        data.insert("branch".to_string(), "feature-x".to_string());
        let item = QueueItem::new("item-1", data);

        let queue = Queue::new("merges").push(item);
        store.save_queue("merges", &queue).unwrap();

        let worker = MergeWorker::new(adapters.clone(), store.clone());
        let result = worker.run_once().await.unwrap();

        assert!(result);

        // Check queue is now empty
        let queue = store.load_queue("merges").unwrap();
        assert!(queue.is_empty());
    }
}

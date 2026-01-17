//! Merge queue worker daemon

use crate::adapters::RepoAdapter;
use crate::effect::MergeStrategy;
use crate::engine::executor::Adapters;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FakeAdapters;
    use crate::queue::{Queue, QueueItem};
    use std::collections::HashMap;

    impl Adapters for FakeAdapters {
        type Sessions = crate::adapters::fake::FakeSessionAdapter;
        type Repos = crate::adapters::fake::FakeRepoAdapter;
        type Issues = crate::adapters::fake::FakeIssueAdapter;

        fn sessions(&self) -> Self::Sessions {
            FakeAdapters::sessions(self)
        }
        fn repos(&self) -> Self::Repos {
            FakeAdapters::repos(self)
        }
        fn issues(&self) -> Self::Issues {
            FakeAdapters::issues(self)
        }
    }

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

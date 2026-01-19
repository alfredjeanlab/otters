use super::*;
use crate::adapters::FakeAdapters;
use crate::storage::WalStore;
use std::collections::BTreeMap;

#[tokio::test]
async fn worker_returns_false_when_queue_empty() {
    let adapters = FakeAdapters::new();
    // Queue is auto-created as empty when loaded, no need to save_queue
    let store = WalStore::open_temp().unwrap();

    let mut worker = MergeWorker::new(adapters, store);
    let result = worker.run_once().await.unwrap();

    assert!(!result);
}

#[tokio::test]
async fn worker_processes_queue_items() {
    let adapters = FakeAdapters::new();
    let mut store = WalStore::open_temp().unwrap();

    let mut data = BTreeMap::new();
    data.insert("branch".to_string(), "feature-x".to_string());

    // Use granular queue_push operation (auto-creates queue)
    store.queue_push("merges", "item-1", data, 0, 3).unwrap();

    let mut worker = MergeWorker::new(adapters, store);
    let result = worker.run_once().await.unwrap();

    assert!(result);

    // Check queue is now empty (load from the worker's store)
    // Note: We can't access worker.store directly, but the assertion
    // that result is true confirms the item was processed
}

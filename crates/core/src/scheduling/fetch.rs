// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Two-phase fetch execution for production adapter wiring
//!
//! This module implements the two-phase execution pattern that separates:
//! 1. **Planning phase**: Determine what needs to be fetched (produces `FetchRequest`s)
//! 2. **Execution phase**: Fetch values, evaluate conditions, produce effects
//!
//! This cleanly separates `&mut SchedulingManager` access from `&MaterializedState` access,
//! avoiding borrow conflicts in the engine.

use super::{
    FetchContext, FetchError, ResourceInfo, ResourceScanner, ScanError, ScannerId, ScannerSource,
    SourceFetcher, SourceValue, WatcherId, WatcherSource,
};

/// Request to fetch a source value
#[derive(Debug, Clone)]
pub enum FetchRequest {
    /// Fetch watcher source for condition evaluation
    WatcherSource {
        watcher_id: WatcherId,
        source: WatcherSource,
        context: FetchContext,
    },
    /// Scan resources for a scanner
    ScannerResources {
        scanner_id: ScannerId,
        source: ScannerSource,
    },
}

/// Result of a fetch request
#[derive(Debug, Clone)]
pub enum FetchResult {
    /// Watcher source value fetched
    WatcherValue {
        watcher_id: WatcherId,
        value: Result<SourceValue, FetchError>,
    },
    /// Scanner resources discovered
    ScannerResources {
        scanner_id: ScannerId,
        resources: Result<Vec<ResourceInfo>, ScanError>,
    },
}

/// Batch of fetch requests to execute
#[derive(Debug, Default)]
pub struct FetchBatch {
    requests: Vec<FetchRequest>,
}

impl FetchBatch {
    /// Create a new empty batch
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a request to the batch
    pub fn add(&mut self, request: FetchRequest) {
        self.requests.push(request);
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Get the number of requests
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Get an iterator over the requests
    pub fn iter(&self) -> impl Iterator<Item = &FetchRequest> {
        self.requests.iter()
    }
}

impl IntoIterator for FetchBatch {
    type Item = FetchRequest;
    type IntoIter = std::vec::IntoIter<FetchRequest>;

    fn into_iter(self) -> Self::IntoIter {
        self.requests.into_iter()
    }
}

/// Batch of fetch results
#[derive(Debug, Default)]
pub struct FetchResults {
    watcher_values: std::collections::HashMap<WatcherId, Result<SourceValue, FetchError>>,
    scanner_resources: std::collections::HashMap<ScannerId, Result<Vec<ResourceInfo>, ScanError>>,
}

impl FetchResults {
    /// Create a new empty results collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a result to the collection
    pub fn add(&mut self, result: FetchResult) {
        match result {
            FetchResult::WatcherValue { watcher_id, value } => {
                self.watcher_values.insert(watcher_id, value);
            }
            FetchResult::ScannerResources {
                scanner_id,
                resources,
            } => {
                self.scanner_resources.insert(scanner_id, resources);
            }
        }
    }

    /// Get a watcher's fetched value
    pub fn watcher_value(&self, id: &WatcherId) -> Option<&Result<SourceValue, FetchError>> {
        self.watcher_values.get(id)
    }

    /// Get a scanner's fetched resources
    pub fn scanner_resources(
        &self,
        id: &ScannerId,
    ) -> Option<&Result<Vec<ResourceInfo>, ScanError>> {
        self.scanner_resources.get(id)
    }

    /// Check if results is empty
    pub fn is_empty(&self) -> bool {
        self.watcher_values.is_empty() && self.scanner_resources.is_empty()
    }
}

/// Executes fetch requests against real system state
pub struct FetchExecutor<'a, S: SourceFetcher, R: ResourceScanner> {
    source_fetcher: &'a S,
    resource_scanner: &'a R,
}

impl<'a, S: SourceFetcher, R: ResourceScanner> FetchExecutor<'a, S, R> {
    /// Create a new fetch executor
    pub fn new(source_fetcher: &'a S, resource_scanner: &'a R) -> Self {
        Self {
            source_fetcher,
            resource_scanner,
        }
    }

    /// Execute a batch of fetch requests
    pub fn execute(&self, batch: FetchBatch) -> FetchResults {
        let mut results = FetchResults::default();

        for request in batch.into_iter() {
            match request {
                FetchRequest::WatcherSource {
                    watcher_id,
                    source,
                    context,
                } => {
                    let value = self.source_fetcher.fetch(&source, &context);
                    results.add(FetchResult::WatcherValue { watcher_id, value });
                }
                FetchRequest::ScannerResources { scanner_id, source } => {
                    let resources = self.resource_scanner.scan(&source);
                    results.add(FetchResult::ScannerResources {
                        scanner_id,
                        resources,
                    });
                }
            }
        }

        results
    }

    /// Execute a single watcher fetch request
    pub fn fetch_watcher(
        &self,
        source: &WatcherSource,
        context: &FetchContext,
    ) -> Result<SourceValue, FetchError> {
        self.source_fetcher.fetch(source, context)
    }

    /// Execute a single scanner fetch request
    pub fn scan_resources(&self, source: &ScannerSource) -> Result<Vec<ResourceInfo>, ScanError> {
        self.resource_scanner.scan(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::{NoOpResourceScanner, NoOpSourceFetcher};

    #[test]
    fn fetch_batch_starts_empty() {
        let batch = FetchBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn fetch_batch_add_requests() {
        let mut batch = FetchBatch::new();

        batch.add(FetchRequest::WatcherSource {
            watcher_id: WatcherId::new("w1"),
            source: WatcherSource::Queue {
                name: "test".to_string(),
            },
            context: FetchContext::default(),
        });

        batch.add(FetchRequest::ScannerResources {
            scanner_id: ScannerId::new("s1"),
            source: ScannerSource::Locks,
        });

        assert!(!batch.is_empty());
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn fetch_results_store_and_retrieve() {
        let mut results = FetchResults::new();

        let watcher_id = WatcherId::new("w1");
        results.add(FetchResult::WatcherValue {
            watcher_id: watcher_id.clone(),
            value: Ok(SourceValue::Numeric { value: 42 }),
        });

        let scanner_id = ScannerId::new("s1");
        results.add(FetchResult::ScannerResources {
            scanner_id: scanner_id.clone(),
            resources: Ok(vec![ResourceInfo::new("r1")]),
        });

        assert!(!results.is_empty());

        let watcher_value = results.watcher_value(&watcher_id);
        assert!(watcher_value.is_some());
        assert!(matches!(
            watcher_value.unwrap(),
            Ok(SourceValue::Numeric { value: 42 })
        ));

        let scanner_resources = results.scanner_resources(&scanner_id);
        assert!(scanner_resources.is_some());
        assert!(scanner_resources.unwrap().is_ok());
    }

    #[test]
    fn fetch_executor_executes_batch() {
        let source_fetcher = NoOpSourceFetcher;
        let resource_scanner = NoOpResourceScanner;
        let executor = FetchExecutor::new(&source_fetcher, &resource_scanner);

        let mut batch = FetchBatch::new();
        batch.add(FetchRequest::WatcherSource {
            watcher_id: WatcherId::new("w1"),
            source: WatcherSource::Queue {
                name: "test".to_string(),
            },
            context: FetchContext::default(),
        });
        batch.add(FetchRequest::ScannerResources {
            scanner_id: ScannerId::new("s1"),
            source: ScannerSource::Locks,
        });

        let results = executor.execute(batch);

        assert!(results.watcher_value(&WatcherId::new("w1")).is_some());
        assert!(results.scanner_resources(&ScannerId::new("s1")).is_some());
    }
}

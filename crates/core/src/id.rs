// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! ID generation abstractions

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Generates unique identifiers
pub trait IdGen: Clone + Send + Sync {
    fn next(&self) -> String;
}

/// UUID-based ID generator for production use
#[derive(Clone, Default)]
pub struct UuidIdGen;

impl IdGen for UuidIdGen {
    fn next(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Sequential ID generator for testing
#[derive(Clone)]
pub struct SequentialIdGen {
    prefix: String,
    counter: Arc<AtomicU64>,
}

impl SequentialIdGen {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl Default for SequentialIdGen {
    fn default() -> Self {
        Self::new("id")
    }
}

impl IdGen for SequentialIdGen {
    fn next(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", self.prefix, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_gen_creates_unique_ids() {
        let id_gen = UuidIdGen;
        let id1 = id_gen.next();
        let id2 = id_gen.next();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 36); // UUID format
    }

    #[test]
    fn sequential_gen_creates_predictable_ids() {
        let id_gen = SequentialIdGen::new("test");
        assert_eq!(id_gen.next(), "test-1");
        assert_eq!(id_gen.next(), "test-2");
        assert_eq!(id_gen.next(), "test-3");
    }

    #[test]
    fn sequential_gen_is_cloneable_and_shared() {
        let id_gen1 = SequentialIdGen::new("shared");
        let id_gen2 = id_gen1.clone();
        assert_eq!(id_gen1.next(), "shared-1");
        assert_eq!(id_gen2.next(), "shared-2");
        assert_eq!(id_gen1.next(), "shared-3");
    }
}

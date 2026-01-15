// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Engine factory for CLI commands

use anyhow::Result;
use oj_core::clock::SystemClock;
use oj_core::engine::Engine;
use oj_core::storage::JsonStore;
use oj_core::RealAdapters;
use std::path::Path;

/// Create a production engine with real adapters
pub fn make_engine() -> Result<Engine<RealAdapters, SystemClock>> {
    let store = find_or_create_store()?;
    let adapters = RealAdapters::new();
    let clock = SystemClock;

    Ok(Engine::new(adapters, store, clock))
}

/// Create a production engine with a specific repo root
pub fn make_engine_with_root(root: impl AsRef<Path>) -> Result<Engine<RealAdapters, SystemClock>> {
    let root = root.as_ref();
    let store_path = root.join(".build/operations");
    std::fs::create_dir_all(&store_path)?;
    let store = JsonStore::open(&store_path)?;
    let adapters = RealAdapters::with_repo_root(root.to_path_buf());
    let clock = SystemClock;

    Ok(Engine::new(adapters, store, clock))
}

/// Find or create the operations store
fn find_or_create_store() -> Result<JsonStore> {
    // Try current directory first
    let local_store = Path::new(".build/operations");
    if local_store.exists() {
        return Ok(JsonStore::open(local_store)?);
    }

    // Try parent directories
    let mut dir = std::env::current_dir()?;
    for _ in 0..5 {
        let store_path = dir.join(".build/operations");
        if store_path.exists() {
            return Ok(JsonStore::open(store_path)?);
        }
        if !dir.pop() {
            break;
        }
    }

    // Create in current directory if not found
    std::fs::create_dir_all(local_store)?;
    Ok(JsonStore::open(local_store)?)
}

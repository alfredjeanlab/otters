// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Engine factory for CLI commands

use anyhow::Result;
use oj_core::clock::SystemClock;
use oj_core::engine::Engine;
use oj_core::storage::WalStore;
use oj_core::RealAdapters;
use std::path::Path;

/// Create a production engine with real adapters
pub fn make_engine() -> Result<Engine<RealAdapters, SystemClock>> {
    let store = find_or_create_store()?;
    let adapters = RealAdapters::new();
    let clock = SystemClock;

    let mut engine = Engine::new(adapters, store, clock);

    // Load runbooks if the directory exists
    if let Some(runbooks_dir) = find_runbooks_dir() {
        if let Err(e) = engine.load_runbooks(&runbooks_dir) {
            tracing::warn!(?e, "Failed to load runbooks, using hardcoded pipelines");
        }
    }

    Ok(engine)
}

/// Find the runbooks directory by searching up from the current directory
fn find_runbooks_dir() -> Option<std::path::PathBuf> {
    // Try current directory first
    let local = Path::new("runbooks");
    if local.exists() && local.is_dir() {
        return Some(local.to_path_buf());
    }

    // Try parent directories
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        let runbooks_path = dir.join("runbooks");
        if runbooks_path.exists() && runbooks_path.is_dir() {
            return Some(runbooks_path);
        }
        if !dir.pop() {
            break;
        }
    }

    None
}

/// Create a production engine with a specific repo root
#[allow(dead_code)] // API: alternative engine factory for nested repo use
pub fn make_engine_with_root(root: impl AsRef<Path>) -> Result<Engine<RealAdapters, SystemClock>> {
    let root = root.as_ref();
    let store_path = root.join(".build/operations");
    std::fs::create_dir_all(&store_path)?;
    let store = WalStore::open_default(&store_path)?;
    let adapters = RealAdapters::with_repo_root(root.to_path_buf());
    let clock = SystemClock;

    Ok(Engine::new(adapters, store, clock))
}

/// Find or create the operations store
fn find_or_create_store() -> Result<WalStore> {
    // Try current directory first
    let local_store = Path::new(".build/operations");
    if local_store.exists() {
        return Ok(WalStore::open_default(local_store)?);
    }

    // Try parent directories
    let mut dir = std::env::current_dir()?;
    for _ in 0..5 {
        let store_path = dir.join(".build/operations");
        if store_path.exists() {
            return Ok(WalStore::open_default(&store_path)?);
        }
        if !dir.pop() {
            break;
        }
    }

    // Create in current directory if not found
    std::fs::create_dir_all(local_store)?;
    Ok(WalStore::open_default(local_store)?)
}

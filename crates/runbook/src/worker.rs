// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Worker definitions

use serde::{Deserialize, Serialize};

/// A worker definition from the runbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDef {
    /// Worker name
    pub name: String,
    /// Maximum concurrent pipelines
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    /// Pipelines this worker processes
    #[serde(default)]
    pub pipelines: Vec<String>,
}

fn default_concurrency() -> u32 {
    1
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

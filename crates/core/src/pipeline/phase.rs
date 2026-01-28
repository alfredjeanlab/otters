// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Pipeline phase definitions

use serde::{Deserialize, Serialize};

/// Status of the current phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseStatus {
    /// Waiting to start
    Pending,
    /// Agent is running
    Running,
    /// Waiting for external input
    Waiting,
    /// Phase completed
    Completed,
    /// Phase failed
    Failed,
}

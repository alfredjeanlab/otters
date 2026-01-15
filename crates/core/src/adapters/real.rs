// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Production adapters bundle for real I/O operations

use crate::adapters::{GitAdapter, OsascriptNotifier, TmuxAdapter, WkAdapter};
use crate::engine::executor::Adapters;
use std::path::PathBuf;

/// Production adapters bundle for real I/O operations
#[derive(Clone)]
pub struct RealAdapters {
    sessions: TmuxAdapter,
    repos: GitAdapter,
    issues: WkAdapter,
    notify: OsascriptNotifier,
}

impl RealAdapters {
    /// Create adapters with default configuration
    pub fn new() -> Self {
        Self {
            sessions: TmuxAdapter::default(),
            repos: GitAdapter::default(),
            issues: WkAdapter::new(),
            notify: OsascriptNotifier::new("Otter Jobs"),
        }
    }

    /// Create adapters for a specific repository root
    pub fn with_repo_root(root: PathBuf) -> Self {
        Self {
            sessions: TmuxAdapter::default(),
            repos: GitAdapter::new(root),
            issues: WkAdapter::new(),
            notify: OsascriptNotifier::new("Otter Jobs"),
        }
    }

    /// Create adapters with a custom session prefix
    pub fn with_session_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.sessions = TmuxAdapter::new(prefix);
        self
    }
}

impl Default for RealAdapters {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapters for RealAdapters {
    type Sessions = TmuxAdapter;
    type Repos = GitAdapter;
    type Issues = WkAdapter;
    type Notify = OsascriptNotifier;

    fn sessions(&self) -> Self::Sessions {
        self.sessions.clone()
    }

    fn repos(&self) -> Self::Repos {
        self.repos.clone()
    }

    fn issues(&self) -> Self::Issues {
        self.issues.clone()
    }

    fn notify(&self) -> Self::Notify {
        self.notify.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_adapters_implements_adapters_trait() {
        let adapters = RealAdapters::new();
        // Verify we can access each adapter type
        let _sessions: TmuxAdapter = adapters.sessions();
        let _repos: GitAdapter = adapters.repos();
        let _issues: WkAdapter = adapters.issues();
        let _notify: OsascriptNotifier = adapters.notify();
    }

    #[test]
    fn real_adapters_with_repo_root() {
        let adapters = RealAdapters::with_repo_root(PathBuf::from("/tmp/test"));
        let repos = adapters.repos();
        assert_eq!(repos.repo_root, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn real_adapters_with_session_prefix() {
        let adapters = RealAdapters::new().with_session_prefix("test-");
        let sessions = adapters.sessions();
        assert_eq!(sessions.session_prefix, "test-");
    }
}

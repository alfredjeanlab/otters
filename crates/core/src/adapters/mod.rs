// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Adapter modules for external integrations

pub mod fake;
pub mod notify;
pub mod real;
pub mod traits;

// Real adapter implementations
pub mod claude;
pub mod git;
pub mod tmux;
pub mod wk;

// Re-export traits
pub use traits::{
    IssueAdapter, IssueError, IssueInfo, MergeResult, RepoAdapter, RepoError, SessionAdapter,
    SessionError, SessionId, SessionInfo, WorktreeInfo,
};

// Re-export notify
pub use notify::{Notification, NotifyAdapter, NotifyError, NotifyUrgency, OsascriptNotifier};

// Re-export fake adapters
pub use fake::{FakeAdapters, FakeNotifier};

// Re-export real adapters
pub use claude::ClaudeAdapter;
pub use git::GitAdapter;
pub use real::RealAdapters;
pub use tmux::TmuxAdapter;
pub use wk::WkAdapter;

//! Adapter modules for external integrations

pub mod fake;
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

// Re-export fake adapters
pub use fake::FakeAdapters;

// Re-export real adapters
pub use claude::ClaudeAdapter;
pub use git::GitAdapter;
pub use tmux::TmuxAdapter;
pub use wk::WkAdapter;

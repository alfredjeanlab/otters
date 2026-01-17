//! Fake adapter implementations for testing

use super::traits::*;
use crate::effect::MergeStrategy;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Recorded call to an adapter method
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterCall {
    // Session calls
    SpawnSession {
        name: String,
        cwd: PathBuf,
        cmd: String,
    },
    SendToSession {
        id: String,
        input: String,
    },
    KillSession {
        id: String,
    },
    IsAlive {
        id: String,
    },
    CapturePaneSession {
        id: String,
        lines: u32,
    },
    ListSessions,

    // Repo calls
    WorktreeAdd {
        branch: String,
        path: PathBuf,
    },
    WorktreeRemove {
        path: PathBuf,
    },
    WorktreeList,
    IsClean {
        path: PathBuf,
    },
    Merge {
        path: PathBuf,
        branch: String,
        strategy: MergeStrategy,
    },

    // Issue calls
    ListIssues {
        labels: Option<Vec<String>>,
    },
    GetIssue {
        id: String,
    },
    StartIssue {
        id: String,
    },
    DoneIssue {
        id: String,
    },
    NoteIssue {
        id: String,
        message: String,
    },
    CreateIssue {
        kind: String,
        title: String,
        labels: Vec<String>,
        parent: Option<String>,
    },
}

/// Shared state for fake adapters
#[derive(Default)]
struct FakeState {
    calls: Vec<AdapterCall>,
    sessions: HashMap<String, FakeSession>,
    worktrees: HashMap<PathBuf, FakeWorktree>,
    issues: HashMap<String, IssueInfo>,
    next_issue_id: u32,
    pane_content: HashMap<String, String>,
    // Configurable failure modes
    send_fails: bool,
    merge_conflicts: bool,
}

struct FakeSession {
    id: SessionId,
    name: String,
    cwd: PathBuf,
    alive: bool,
}

struct FakeWorktree {
    path: PathBuf,
    branch: String,
}

/// Fake adapters with call recording for testing
#[derive(Clone)]
pub struct FakeAdapters {
    state: Arc<Mutex<FakeState>>,
}

impl Default for FakeAdapters {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeAdapters {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }

    /// Get all recorded calls
    pub fn calls(&self) -> Vec<AdapterCall> {
        self.state.lock().unwrap().calls.clone()
    }

    /// Clear recorded calls
    pub fn clear_calls(&self) {
        self.state.lock().unwrap().calls.clear();
    }

    /// Set the pane content for a session (for testing capture_pane)
    pub fn set_pane_content(&self, session_id: &str, content: impl Into<String>) {
        self.state
            .lock()
            .unwrap()
            .pane_content
            .insert(session_id.to_string(), content.into());
    }

    /// Add a pre-existing issue
    pub fn add_issue(&self, info: IssueInfo) {
        self.state
            .lock()
            .unwrap()
            .issues
            .insert(info.id.clone(), info);
    }

    /// Get the session adapter
    pub fn sessions(&self) -> FakeSessionAdapter {
        FakeSessionAdapter {
            state: self.state.clone(),
        }
    }

    /// Get the repo adapter
    pub fn repos(&self) -> FakeRepoAdapter {
        FakeRepoAdapter {
            state: self.state.clone(),
        }
    }

    /// Get the issue adapter
    pub fn issues(&self) -> FakeIssueAdapter {
        FakeIssueAdapter {
            state: self.state.clone(),
        }
    }

    /// Mark a session as dead (is_alive returns false)
    pub fn mark_session_dead(&self, session_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(session) = state.sessions.get_mut(session_id) {
            session.alive = false;
        }
    }

    /// Configure send to fail for testing error paths
    pub fn set_send_fails(&self, fails: bool) {
        let mut state = self.state.lock().unwrap();
        state.send_fails = fails;
    }

    /// Configure merges to conflict for testing
    pub fn set_merge_conflicts(&self, conflicts: bool) {
        let mut state = self.state.lock().unwrap();
        state.merge_conflicts = conflicts;
    }
}

// Implement the Adapters trait for FakeAdapters
impl crate::engine::executor::Adapters for FakeAdapters {
    type Sessions = FakeSessionAdapter;
    type Repos = FakeRepoAdapter;
    type Issues = FakeIssueAdapter;

    fn sessions(&self) -> Self::Sessions {
        self.sessions()
    }

    fn repos(&self) -> Self::Repos {
        self.repos()
    }

    fn issues(&self) -> Self::Issues {
        self.issues()
    }
}

// =============================================================================
// Fake Session Adapter
// =============================================================================

#[derive(Clone)]
pub struct FakeSessionAdapter {
    state: Arc<Mutex<FakeState>>,
}

#[async_trait]
impl SessionAdapter for FakeSessionAdapter {
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::SpawnSession {
            name: name.to_string(),
            cwd: cwd.to_path_buf(),
            cmd: cmd.to_string(),
        });

        if state.sessions.contains_key(name) {
            return Err(SessionError::AlreadyExists(name.to_string()));
        }

        let id = SessionId(name.to_string());
        state.sessions.insert(
            name.to_string(),
            FakeSession {
                id: id.clone(),
                name: name.to_string(),
                cwd: cwd.to_path_buf(),
                alive: true,
            },
        );
        Ok(id)
    }

    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::SendToSession {
            id: id.0.clone(),
            input: input.to_string(),
        });

        if state.send_fails {
            return Err(SessionError::SpawnFailed("configured to fail".to_string()));
        }

        if !state.sessions.contains_key(&id.0) {
            return Err(SessionError::NotFound(id.clone()));
        }
        Ok(())
    }

    async fn kill(&self, id: &SessionId) -> Result<(), SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::KillSession { id: id.0.clone() });

        if let Some(session) = state.sessions.get_mut(&id.0) {
            session.alive = false;
            Ok(())
        } else {
            Err(SessionError::NotFound(id.clone()))
        }
    }

    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::IsAlive { id: id.0.clone() });

        if let Some(session) = state.sessions.get(&id.0) {
            Ok(session.alive)
        } else {
            Err(SessionError::NotFound(id.clone()))
        }
    }

    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::CapturePaneSession {
            id: id.0.clone(),
            lines,
        });

        if !state.sessions.contains_key(&id.0) {
            return Err(SessionError::NotFound(id.clone()));
        }

        Ok(state
            .pane_content
            .get(&id.0)
            .cloned()
            .unwrap_or_default())
    }

    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::ListSessions);

        Ok(state
            .sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                created_at: chrono::Utc::now(),
                attached: false,
            })
            .collect())
    }
}

// =============================================================================
// Fake Repo Adapter
// =============================================================================

#[derive(Clone)]
pub struct FakeRepoAdapter {
    state: Arc<Mutex<FakeState>>,
}

#[async_trait]
impl RepoAdapter for FakeRepoAdapter {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::WorktreeAdd {
            branch: branch.to_string(),
            path: path.to_path_buf(),
        });

        state.worktrees.insert(
            path.to_path_buf(),
            FakeWorktree {
                path: path.to_path_buf(),
                branch: branch.to_string(),
            },
        );
        Ok(())
    }

    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::WorktreeRemove {
            path: path.to_path_buf(),
        });

        state.worktrees.remove(path);
        Ok(())
    }

    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::WorktreeList);

        Ok(state
            .worktrees
            .values()
            .map(|w| WorktreeInfo {
                path: w.path.clone(),
                branch: w.branch.clone(),
                head: "abc123".to_string(),
                locked: false,
            })
            .collect())
    }

    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::IsClean {
            path: path.to_path_buf(),
        });
        Ok(true)
    }

    async fn merge(
        &self,
        path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult, RepoError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::Merge {
            path: path.to_path_buf(),
            branch: branch.to_string(),
            strategy,
        });

        if state.merge_conflicts {
            return Ok(MergeResult::Conflict {
                files: vec!["conflicting-file.rs".to_string()],
            });
        }

        Ok(match strategy {
            MergeStrategy::FastForward => MergeResult::FastForwarded,
            MergeStrategy::Rebase => MergeResult::Rebased,
            MergeStrategy::Merge => MergeResult::Success,
        })
    }
}

// =============================================================================
// Fake Issue Adapter
// =============================================================================

#[derive(Clone)]
pub struct FakeIssueAdapter {
    state: Arc<Mutex<FakeState>>,
}

#[async_trait]
impl IssueAdapter for FakeIssueAdapter {
    async fn list(&self, labels: Option<&[&str]>) -> Result<Vec<IssueInfo>, IssueError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::ListIssues {
            labels: labels.map(|l| l.iter().map(|s| s.to_string()).collect()),
        });

        let issues: Vec<_> = state
            .issues
            .values()
            .filter(|i| {
                labels
                    .map(|ls| ls.iter().any(|l| i.labels.contains(&l.to_string())))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        Ok(issues)
    }

    async fn get(&self, id: &str) -> Result<IssueInfo, IssueError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::GetIssue { id: id.to_string() });

        state
            .issues
            .get(id)
            .cloned()
            .ok_or_else(|| IssueError::NotFound(id.to_string()))
    }

    async fn start(&self, id: &str) -> Result<(), IssueError> {
        let mut state = self.state.lock().unwrap();
        state
            .calls
            .push(AdapterCall::StartIssue { id: id.to_string() });

        if let Some(issue) = state.issues.get_mut(id) {
            issue.status = "in_progress".to_string();
            Ok(())
        } else {
            Err(IssueError::NotFound(id.to_string()))
        }
    }

    async fn done(&self, id: &str) -> Result<(), IssueError> {
        let mut state = self.state.lock().unwrap();
        state
            .calls
            .push(AdapterCall::DoneIssue { id: id.to_string() });

        if let Some(issue) = state.issues.get_mut(id) {
            issue.status = "done".to_string();
            Ok(())
        } else {
            Err(IssueError::NotFound(id.to_string()))
        }
    }

    async fn note(&self, id: &str, message: &str) -> Result<(), IssueError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::NoteIssue {
            id: id.to_string(),
            message: message.to_string(),
        });

        if state.issues.contains_key(id) {
            Ok(())
        } else {
            Err(IssueError::NotFound(id.to_string()))
        }
    }

    async fn create(
        &self,
        kind: &str,
        title: &str,
        labels: &[&str],
        parent: Option<&str>,
    ) -> Result<String, IssueError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(AdapterCall::CreateIssue {
            kind: kind.to_string(),
            title: title.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            parent: parent.map(|s| s.to_string()),
        });

        let id = format!("issue-{}", state.next_issue_id);
        state.next_issue_id += 1;

        state.issues.insert(
            id.clone(),
            IssueInfo {
                id: id.clone(),
                title: title.to_string(),
                status: "open".to_string(),
                labels: labels.iter().map(|s| s.to_string()).collect(),
            },
        );

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_session_adapter_records_calls() {
        let adapters = FakeAdapters::new();
        let sessions = adapters.sessions();

        let id = sessions
            .spawn("test", Path::new("/tmp"), "echo hello")
            .await
            .unwrap();

        assert_eq!(id.0, "test");
        assert_eq!(
            adapters.calls(),
            vec![AdapterCall::SpawnSession {
                name: "test".to_string(),
                cwd: PathBuf::from("/tmp"),
                cmd: "echo hello".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn fake_session_adapter_prevents_duplicates() {
        let adapters = FakeAdapters::new();
        let sessions = adapters.sessions();

        sessions
            .spawn("test", Path::new("/tmp"), "echo hello")
            .await
            .unwrap();
        let result = sessions
            .spawn("test", Path::new("/tmp"), "echo world")
            .await;

        assert!(matches!(result, Err(SessionError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn fake_repo_adapter_tracks_worktrees() {
        let adapters = FakeAdapters::new();
        let repos = adapters.repos();

        repos
            .worktree_add("feature-x", Path::new("/tmp/worktree"))
            .await
            .unwrap();

        let worktrees = repos.worktree_list().await.unwrap();
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].branch, "feature-x");
    }

    #[tokio::test]
    async fn fake_issue_adapter_creates_issues() {
        let adapters = FakeAdapters::new();
        let issues = adapters.issues();

        let id = issues
            .create("feature", "Add auth", &["plan:auth"], None)
            .await
            .unwrap();

        let issue = issues.get(&id).await.unwrap();
        assert_eq!(issue.title, "Add auth");
        assert!(issue.labels.contains(&"plan:auth".to_string()));
    }
}

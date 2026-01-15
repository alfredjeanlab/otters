# Integration Adapters

This document details adapter designs for external integrations: tmux, git, Claude Code, and wk.

## Adapter Design Principles

All adapters follow these rules:

1. **Trait-first** - Define interface before implementation
2. **Async** - All I/O is async for parallelism
3. **Error-rich** - Errors include context for debugging
4. **Testable** - Every trait has a fake implementation
5. **Configurable** - Behavior controlled via config, not hardcoded

## Adapter Trait Pattern

```rust
/// Production adapter
pub struct TmuxAdapter { /* ... */ }

/// Test fake - records calls, returns configured responses
pub struct FakeTmuxAdapter {
    calls: RefCell<Vec<Call>>,
    responses: RefCell<HashMap<CallPattern, Response>>,
}

/// Test simulator - executes simplified logic
pub struct SimulatedTmuxAdapter {
    sessions: RefCell<HashMap<SessionId, SimSession>>,
}
```

## Session Adapter (tmux)

### Trait Definition

```rust
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync + 'static {
    /// Create new session with command
    async fn spawn(
        &self,
        name: &str,
        working_dir: &Path,
        command: &str,
    ) -> Result<SessionId, SessionError>;

    /// Send input to session
    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError>;

    /// Kill session
    async fn kill(&self, id: &SessionId) -> Result<(), SessionError>;

    /// Check if session process is alive
    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError>;

    /// Capture recent pane output
    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError>;

    /// List all armor sessions
    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError>;
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub created: Instant,
    pub is_alive: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(SessionId),

    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    #[error("Failed to spawn session: {0}")]
    SpawnFailed(String),

    #[error("tmux command failed: {cmd}, stderr: {stderr}")]
    CommandFailed { cmd: String, stderr: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Production Implementation

The `TmuxAdapter` wraps tmux commands:
- **spawn** - `tmux new-session -d -s {name} -c {dir} {cmd}`
- **send** - `tmux send-keys -t {id} {input}`
- **kill** - `tmux kill-session -t {id}` (idempotent - ignores "not found")
- **is_alive** - `tmux has-session -t {id}`
- **capture_pane** - `tmux capture-pane -t {id} -p -S -{lines}`
- **list** - `tmux list-sessions -F "#{session_name}:#{session_created}"`

### Fake Implementation

The fake tracks calls and maintains in-memory session state:

```rust
pub struct FakeSessionAdapter {
    calls: Arc<Mutex<Vec<SessionCall>>>,
    sessions: Arc<Mutex<HashMap<SessionId, FakeSession>>>,
    config: FakeSessionConfig,
}

pub struct FakeSessionConfig {
    pub spawn_fails: bool,
    pub default_output: String,
}
```

Test helpers:
- `set_output(id, output)` - Configure pane capture response
- `calls()` - Get recorded calls for assertions
- `assert_called(expected)` - Verify specific call was made

## Repository Adapter (git)

### Trait Definition

```rust
#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync + 'static {
    /// Add worktree for branch
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError>;

    /// Remove worktree
    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError>;

    /// List worktrees
    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError>;

    /// Check if working tree is clean
    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError>;

    /// Check if branch exists
    async fn branch_exists(&self, branch: &str) -> Result<bool, RepoError>;

    /// Check if branch is merged into target
    async fn is_merged(&self, branch: &str, into: &str) -> Result<bool, RepoError>;

    /// Merge branch with strategy
    async fn merge(
        &self,
        path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult, RepoError>;

    /// Get branch info
    async fn branch_info(&self, branch: &str) -> Result<BranchInfo, RepoError>;
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub is_clean: bool,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub exists: bool,
    pub is_clean: bool,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    FastForward,
    Rebase,
    Merge,
}

#[derive(Debug)]
pub enum MergeResult {
    Success,
    FastForwarded,
    Rebased,
    Conflict { files: Vec<PathBuf> },
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Worktree error: {0}")]
    WorktreeError(String),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("Git command failed: {cmd}, stderr: {stderr}")]
    CommandFailed { cmd: String, stderr: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Production Implementation

The `GitAdapter` wraps git commands:
- **worktree_add** - `git worktree add {path} {branch}`
- **worktree_remove** - `git worktree remove {path}`
- **is_clean** - `git status --porcelain` (empty = clean)
- **merge** - Strategy-dependent:
  - FastForward: `git merge --ff-only {branch}`
  - Rebase: `git rebase {branch}`
  - Merge: `git merge --no-ff {branch}`
- Conflict detection via stderr parsing and `git diff --name-only --diff-filter=U`

## Issue Adapter (wk)

### Trait Definition

```rust
#[async_trait]
pub trait IssueAdapter: Clone + Send + Sync + 'static {
    /// List issues with filter
    async fn list(&self, filter: IssueFilter) -> Result<Vec<Issue>, IssueError>;

    /// Get single issue
    async fn get(&self, id: &IssueId) -> Result<Issue, IssueError>;

    /// Create new issue
    async fn create(&self, req: CreateIssueRequest) -> Result<Issue, IssueError>;

    /// Start working on issue
    async fn start(&self, id: &IssueId) -> Result<(), IssueError>;

    /// Complete issue
    async fn done(&self, id: &IssueId) -> Result<(), IssueError>;

    /// Close issue without completing
    async fn close(&self, id: &IssueId, reason: &str) -> Result<(), IssueError>;

    /// Add note to issue
    async fn note(&self, id: &IssueId, content: &str) -> Result<(), IssueError>;

    /// Add label to issue
    async fn label(&self, id: &IssueId, label: &str) -> Result<(), IssueError>;

    /// Remove label from issue
    async fn unlabel(&self, id: &IssueId, label: &str) -> Result<(), IssueError>;

    /// Add dependency
    async fn add_dep(&self, blocker: &IssueId, blocked: &IssueId) -> Result<(), IssueError>;
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub id: IssueId,
    pub title: String,
    pub description: Option<String>,
    pub status: IssueStatus,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IssueStatus {
    Todo,
    InProgress,
    Done,
    Closed,
}

#[derive(Debug, Clone, Default)]
pub struct IssueFilter {
    pub status: Option<IssueStatus>,
    pub labels: Vec<String>,
    pub exclude_labels: Vec<String>,
    pub blocked: Option<bool>,
}
```

### Production Implementation

The `WkAdapter` wraps wk CLI commands:
- **list** - `wk list --json [--status {status}] [--label {label}]... [--blocked]`
- **get** - `wk show {id} --json`
- **create** - `wk new {type} "{title}" [--note "{desc}"] [--label {label}]...`
- **start** - `wk start {id}`
- **done** - `wk done {id}`
- **close** - `wk close {id} --reason="{reason}"`
- **note** - `wk note {id} "{content}"`
- **label/unlabel** - `wk label {id} {label}` / `wk unlabel {id} {label}`
- **add_dep** - `wk dep {blocker} blocks {blocked}`

## Agent Adapter (Claude Code)

### Trait Definition

```rust
#[async_trait]
pub trait AgentAdapter: Clone + Send + Sync + 'static {
    /// Invoke agent with prompt (via session)
    async fn invoke(
        &self,
        session: &SessionId,
        prompt: &str,
    ) -> Result<(), AgentError>;

    /// Check agent heartbeat status
    async fn heartbeat(&self, session: &SessionId) -> Result<HeartbeatStatus, AgentError>;

    /// Nudge stuck agent
    async fn nudge(&self, session: &SessionId) -> Result<(), AgentError>;

    /// Get Claude log path for session
    fn log_path(&self, workspace: &Workspace) -> PathBuf;
}

#[derive(Debug, Clone)]
pub enum HeartbeatStatus {
    Active {
        last_output: Instant,
        last_log: Option<Instant>,
        last_tool: Option<String>,
    },
    Idle {
        since: Instant,
    },
    SessionDead,
}
```

### Production Implementation

The `ClaudeAdapter` composes with `SessionAdapter` for session interaction:
- **invoke** - Send prompt to session via tmux send-keys
- **heartbeat** - Check multiple signals in parallel:
  - Session alive via tmux has-session
  - Terminal output via capture_pane
  - Log file modification time at `~/.claude/projects/{project_hash}/*.jsonl`
  - Returns Active/Idle/SessionDead based on activity recency
- **nudge** - Send newline to wake up prompt

## Notification Adapter

### Trait Definition

```rust
#[async_trait]
pub trait NotifyAdapter: Clone + Send + Sync + 'static {
    /// Send notification
    async fn notify(&self, notification: Notification) -> Result<(), NotifyError>;
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub sound: Option<String>,
    pub urgency: Urgency,
}

#[derive(Debug, Clone, Copy)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}
```

### macOS Implementation

Uses AppleScript via `osascript`:
```applescript
display notification "{message}" with title "{title}" sound name "{sound}"
```

## Adapter Aggregation

Combine all adapters into single injectable:

```rust
pub trait Adapters: Clone + Send + Sync + 'static {
    type Sessions: SessionAdapter;
    type Repos: RepoAdapter;
    type Issues: IssueAdapter;
    type Agent: AgentAdapter;
    type Notify: NotifyAdapter;

    fn sessions(&self) -> &Self::Sessions;
    fn repos(&self) -> &Self::Repos;
    fn issues(&self) -> &Self::Issues;
    fn agent(&self) -> &Self::Agent;
    fn notify(&self) -> &Self::Notify;
}
```

Two implementations:
- `ProductionAdapters` - Real tmux, git, wk, claude, osascript
- `FakeAdapters` - In-memory fakes with call recording and configurable responses

## Contract Testing

Each adapter trait has contract tests that both production and fake implementations must pass:

```rust
/// Contract tests that any SessionAdapter impl must pass
pub async fn session_adapter_contract_tests<A: SessionAdapter>(adapter: A) {
    // spawn creates session, is_alive returns true
    // kill removes session, is_alive returns false
    // spawn with existing name fails with AlreadyExists
}
```

Run fake tests normally; integration tests with real adapters use `#[ignore]` and run with `--ignored`.

## See Also

- [Module Structure](01-modules.md) - Adapter module boundaries
- [Execution Layer](03-execution.md) - Session usage
- [Testing Strategy](07-testing.md) - Testing adapters

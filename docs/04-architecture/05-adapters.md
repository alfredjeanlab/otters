# Adapters

Adapters abstract external system I/O, enabling comprehensive testing without real tmux/git/etc.

## Pattern

```
State Machine → Effect → Executor → Adapter → subprocess
                                      ↓
                              FakeAdapter (tests)
```

State machines are pure. Adapters handle all I/O. Tests use fakes.

## What Gets an Adapter

Adapters wrap **simple CLI tools** with predictable behavior:

| Tool | Adapter | Why |
|------|---------|-----|
| tmux | `SessionAdapter` | Stateful session management |
| git | `RepoAdapter` | Worktree and merge operations |
| wk | `IssueAdapter` | Issue tracker integration |
| osascript | `NotifyAdapter` | Desktop notifications |

**Claude Code does NOT use an adapter.** It's invoked via `SessionAdapter` (runs in tmux). For testing, use [claudeless](https://github.com/anthropics/claudeless) - a full CLI simulator that emulates Claude's interface, TUI, hooks, and permissions.

## Adapter Traits

| Trait | Wraps | Key Methods |
|-------|-------|-------------|
| `SessionAdapter` | tmux | spawn, send, kill, is_alive, capture_pane |
| `RepoAdapter` | git | worktree_add, worktree_remove, is_clean, merge |
| `IssueAdapter` | wk CLI | list, get, start, done, note, create |
| `NotifyAdapter` | osascript | notify |

## SessionAdapter

Manages tmux sessions for running agents.

```rust
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync + 'static {
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError>;
    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError>;
    async fn kill(&self, id: &SessionId) -> Result<(), SessionError>;
    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError>;
    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError>;
    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError>;
}
```

**Production** (`TmuxAdapter`): Shells out to tmux commands.

**Fake** (`FakeSessionAdapter`): In-memory state, records all calls.

## RepoAdapter

Manages git worktrees for workspace isolation.

```rust
#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync + 'static {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError>;
    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError>;
    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError>;
    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError>;
    async fn merge(&self, path: &Path, branch: &str, strategy: MergeStrategy) -> Result<MergeResult, RepoError>;
}
```

**Production** (`GitAdapter`): Shells out to git commands.

**Fake** (`FakeRepoAdapter`): In-memory worktree tracking, configurable merge conflicts.

## IssueAdapter

Integrates with issue tracker (wk CLI).

```rust
#[async_trait]
pub trait IssueAdapter: Clone + Send + Sync + 'static {
    async fn list(&self, labels: Option<&[&str]>) -> Result<Vec<IssueInfo>, IssueError>;
    async fn get(&self, id: &str) -> Result<IssueInfo, IssueError>;
    async fn start(&self, id: &str) -> Result<(), IssueError>;
    async fn done(&self, id: &str) -> Result<(), IssueError>;
    async fn note(&self, id: &str, message: &str) -> Result<(), IssueError>;
    async fn create(&self, kind: &str, title: &str, labels: &[&str], parent: Option<&str>) -> Result<String, IssueError>;
}
```

**Production** (`WkAdapter`): Shells out to wk CLI.

**Fake** (`FakeIssueAdapter`): In-memory issue state.

## NotifyAdapter

Sends desktop notifications.

```rust
#[async_trait]
pub trait NotifyAdapter: Clone + Send + Sync + 'static {
    async fn notify(&self, notification: Notification) -> Result<(), NotifyError>;
}
```

**Production** (`OsascriptNotifier`): Uses AppleScript on macOS.

**Fake** (`FakeNotifier`): Records notifications for test assertions.

## FakeAdapters

Bundles all fakes with shared state and call recording:

```rust
let adapters = FakeAdapters::new();

// Configure behavior
adapters.set_merge_conflicts(true);
adapters.set_pane_content("session-1", "output text");
adapters.add_issue(IssueInfo { ... });

// Run code under test
// ...

// Verify calls
assert!(adapters.calls().contains(&AdapterCall::SpawnSession { ... }));
```

## Testing

Fakes enable:
- **Deterministic tests**: No real tmux/git needed
- **Call verification**: Assert exactly what operations were attempted
- **Error injection**: `set_send_fails(true)` to test error paths
- **State simulation**: Pre-populate sessions, issues, worktrees

Integration tests with real adapters use `#[ignore]` and run separately.

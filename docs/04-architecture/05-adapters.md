# Adapters

Adapters abstract external system I/O, enabling comprehensive testing without real tmux/git/etc.

## Pattern

```
State Machine → Effect → Executor → TracedAdapter → Adapter → subprocess
                                         ↓
                                   FakeAdapter (tests)
```

State machines are pure. Adapters handle all I/O. Tests use fakes.

## What Gets an Adapter

Adapters wrap **simple CLI tools** with predictable behavior:

| Tool | Adapter | Why |
|------|---------|-----|
| tmux | `SessionAdapter` | Stateful session management |
| git | `RepoAdapter` | Worktree operations |
| osascript | `NotifyAdapter` | Desktop notifications |

**Claude Code does NOT use an adapter.** It's invoked via `SessionAdapter` (runs in tmux). For testing, use [claudeless](https://github.com/anthropics/claudeless) - a full CLI simulator that emulates Claude's interface, TUI, hooks, and permissions.

## Adapter Traits

| Trait | Wraps | Key Methods |
|-------|-------|-------------|
| `SessionAdapter` | tmux | spawn, send, kill, is_alive, capture_output |
| `RepoAdapter` | git | worktree_add, worktree_remove, worktree_list, is_clean, merge |
| `IssueAdapter` | wok | list, get, start, done, note, create |
| `NotifyAdapter` | osascript | send |

## SessionAdapter

Manages tmux sessions for running agents.

```rust
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync + 'static {
    async fn spawn(
        &self,
        name: &str,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<String, SessionError>;
    async fn send(&self, id: &str, input: &str) -> Result<(), SessionError>;
    async fn kill(&self, id: &str) -> Result<(), SessionError>;
    async fn is_alive(&self, id: &str) -> Result<bool, SessionError>;
    async fn capture_output(&self, id: &str, lines: u32) -> Result<String, SessionError>;
    async fn is_process_running(&self, id: &str, pattern: &str) -> Result<bool, SessionError>;
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
    async fn worktree_list(&self) -> Result<Vec<String>, RepoError>;
    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError>;
    async fn merge(&self, path: &Path, branch: &str, strategy: MergeStrategy) -> Result<MergeResult, RepoError>;
}
```

**Production** (`GitAdapter`): Shells out to git commands.

**Fake** (`FakeRepoAdapter`): In-memory worktree tracking, configurable merge conflicts.

## IssueAdapter

Integrates with issue tracker (wok).

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

**Production** (`WokAdapter`): Shells out to the wok cli.

**Fake** (`FakeIssueAdapter`): In-memory issue state.

## NotifyAdapter

Sends notifications to external channels.

```rust
#[async_trait]
pub trait NotifyAdapter: Clone + Send + Sync + 'static {
    async fn send(&self, channel: &str, message: &str) -> Result<(), NotifyError>;
}
```

**Production** (`NoOpNotifyAdapter`): Currently a no-op placeholder.

**Fake** (`FakeNotifyAdapter`): Records notifications for test assertions.

## Traced Wrappers

Adapters are wrapped with instrumentation for observability:

```rust
// At construction (in daemon lifecycle)
let sessions = TracedSessionAdapter::new(TmuxAdapter::new());
let repos = TracedRepoAdapter::new(GitAdapter::new(project_root));
```

Traced wrappers provide **generic** observability:
- Entry/exit logging with operation-specific fields
- Timing metrics (`elapsed_ms`) on every call
- Precondition validation before delegating to inner adapter
- Consistent error logging with context

Production adapters retain **implementation-specific** logging:
- `TmuxAdapter`: Warns when killing existing session before spawn
- Other operational details that the generic wrapper can't know about

This layering keeps observability consistent while preserving useful implementation details.

## Precondition Validation

Traced wrappers validate assumptions before attempting operations:

| Operation | Precondition | Error |
|-----------|-------------|-------|
| session.spawn | cwd exists | SpawnFailed("working directory does not exist") |
| repo.worktree_add | parent dir exists | CommandFailed("parent directory does not exist") |

This catches configuration errors early with clear messages, rather than failing deep in subprocess calls. Production adapters should not duplicate these checks.

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
- **State simulation**: Pre-populate sessions, worktrees

Integration tests with real adapters use `#[ignore]` and run separately.

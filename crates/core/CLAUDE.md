# oj-core

Core library for the Otter Jobs (oj) CLI tool.

## Architecture

This crate follows a functional core / imperative shell architecture:

- **State machines** (`workspace.rs`, `session.rs`, `pipeline.rs`, `queue.rs`): Pure functions that compute state transitions and emit effects
- **Adapters** (`adapters/`): I/O boundary implementations (tmux, git, wk)
- **Storage** (`storage/`): JSON file persistence
- **Engine** (`engine/`): Effect execution and workers

## Key Types

### State Machines

- `Workspace` - Git worktree state (Creating → Ready → InUse → Dirty/Stale)
- `Session` - Tmux session state (Starting → Running → Idle → Dead)
- `Pipeline` - Workflow state machine (phases vary by pipeline type)
- `Queue` - Priority queue with dead letter support

### Adapters

- `SessionAdapter` - Tmux operations (spawn, send, kill, capture)
- `RepoAdapter` - Git operations (worktree, merge)
- `IssueAdapter` - Issue tracker operations (wk CLI)

### Testing

Use `FakeAdapters` for testing, which records all calls for verification.

## Usage

```rust
use oj_core::{
    Pipeline, Workspace, Session, Queue,
    TmuxAdapter, GitAdapter, WkAdapter,
    Clock, SystemClock,
};
```

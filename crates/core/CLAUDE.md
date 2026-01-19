# oj-core

Core library for the Otter Jobs (oj) CLI tool.

## Landing Checklist

Before committing changes to core:
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -p otters-core`
- [ ] No new `#[allow(dead_code)]` without justification
- [ ] State machine changes have corresponding test coverage

## Architecture

Functional core / imperative shell:

- **State machines** (`workspace.rs`, `session.rs`, `pipeline.rs`, `queue.rs`): Pure transitions + effects
- **adapters/**: External I/O (tmux, git, wk) — Fake implementations for all traits
- **coordination/**: Distributed resources (locks, semaphores, guards) — Heartbeat-based staleness
- **engine/**: Effect execution orchestration — Causal effect ordering
- **events/**: Event routing and audit — Pattern-based subscriptions
- **pipelines/**: Workflow state machines — Deterministic transitions
- **storage/**: WAL persistence — Atomic writes

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

# Adapters Module

External system integration layer. All I/O happens through adapters.

## Adapter Contracts

### Required Implementations
Every adapter trait MUST have:
1. A real implementation (production use)
2. A fake implementation (testing use)

### Trait Definitions

| Trait | Purpose | Key Methods |
|-------|---------|-------------|
| `SessionAdapter` | tmux session management | `spawn`, `send`, `kill`, `is_alive`, `capture_pane` |
| `RepoAdapter` | Git worktree operations | `worktree_add`, `worktree_remove`, `is_clean`, `merge` |
| `NotifyAdapter` | User notifications | `notify` |
| `ClaudeAdapter` | Claude Code integration | `spawn_session` |
| `IssueAdapter` | Issue tracker integration | `fetch_issue`, `update_status` |

### Invariants

```
INVARIANT: All adapter methods are async. No blocking I/O.
INVARIANT: Return Result<T, E> with descriptive errors.
INVARIANT: Operations should be idempotent where possible.
INVARIANT: Fake implementations must exercise the same code paths.
```

### Testing Pattern

```rust
// Tests use FakeAdapters for determinism
let adapters = FakeAdapters::new();
adapters.session.set_alive("session-1", true);

// Real adapters only in integration tests
#[cfg(feature = "integration")]
async fn test_real_tmux() { ... }
```

## Landing Checklist

- [ ] New adapter trait has fake implementation
- [ ] Fake implementation in `fake.rs` or `*_tests.rs`
- [ ] Error types implement `std::error::Error`
- [ ] No blocking I/O (use async)

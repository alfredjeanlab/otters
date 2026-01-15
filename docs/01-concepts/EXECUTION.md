# Execution Model

Two abstractions sit beneath runbooks, decoupling "what to run" from "where to run it."

```
Runbook layer:    command → worker → pipeline → task
                                          │
Execution layer:               workspace + session
```

## Workspace

An **isolated context for work** - not necessarily code or git.

A workspace provides:
- **Identity**: Unique name for this work context
- **Isolation**: Separate from other concurrent work
- **Lifecycle**: Setup before work, teardown after
- **Context**: Values tasks can reference (paths, connections, handles)

### Workspace Types

| Type | Context | Use Case |
|------|---------|----------|
| `git-worktree` | `path`, `branch` | Code changes in isolated branch |

### Git Worktree Implementation

Storage locations (in order of preference):
1. **XDG state directory**: `~/.local/state/deps/tree/<name>/<repo>/`
2. **Git directory fallback**: `.git/beads-worktrees/<name>/<repo>/`

XDG is preferred because it keeps `.git/` clean and survives `git clean` operations. Falls back to `.git/` when XDG is unavailable.

**Settings sync**: When creating a workspace, copy `.claude/settings.json` to `<tree>/.claude/settings.local.json`. This allows task-specific hook configuration while inheriting project defaults.

> **Future Ideas:**
> | `directory` | `path` | Local directory (no VCS) |
> | `database` | `connection`, `schema` | Isolated DB schema |
> | `browser` | `profile` | Browser automation |
> | `k8s` | `namespace`, `context` | Kubernetes work |
> | `container` | `container_id` | Docker container |

## Session

An **execution environment for a task** - where the agent actually runs.

A session provides:
- **Isolation**: Separate process/environment
- **Monitoring**: Heartbeat detection for stuck tasks
- **Control**: Nudge, restart, or kill stuck sessions

### Session Properties

| Property | Description |
|----------|-------------|
| `id` | Session identifier |
| `cwd` | Working directory |
| `env` | Environment variables |

### Heartbeat

How we detect if a session is alive depends on session type and what's running:

| Context | Possible heartbeats |
|---------|---------------------|
| tmux + shell | Terminal output |
| tmux + Claude | Output, API activity, tool calls, checkpoint writes |
| Container | Process running, logs |
| Browser | Page activity, network requests |

The right heartbeat depends on the task. A shell script might emit regular output; Claude might be "thinking" silently before a burst of tool calls.

When idle too long, recovery actions trigger: nudge → restart → escalate.

## Summary

| Concept | Abstracts | Current | Future |
|---------|-----------|---------|--------|
| **Workspace** | Isolated work context | Git worktree | DB schema, browser, k8s, etc. |
| **Session** | Where agent runs | Tmux | Remote, container, cloud |

These abstractions enable the same runbook to work across different environments.

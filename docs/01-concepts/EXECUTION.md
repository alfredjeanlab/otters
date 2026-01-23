# Execution Model

Two abstractions sit beneath runbooks, decoupling "what to run" from "where to run it."

```
Runbook layer:    command → worker → pipeline → agent
                                          │
Execution layer:               workspace + session
```

## Workspace

An **isolated context for work** - typically a git worktree for code changes.

A workspace provides:
- **Identity**: Unique name for this work context
- **Isolation**: Separate from other concurrent work
- **Lifecycle**: Setup before work, teardown after
- **Context**: Values tasks can reference (`{workspace}`, `{branch}`)

### Git Worktree

The primary workspace type. Creates an isolated git worktree for each pipeline.

```toml
[pipeline.build.defaults]
workspace = ".worktrees/build-{name}"
branch = "build-{name}"
```

**Storage location**: `~/.local/state/oj/worktrees/<repo>/<name>/`

Using XDG state directory keeps `.git/` clean and survives `git clean` operations.

**Settings sync**: When creating a workspace, `.claude/settings.json` is copied to `<workspace>/.claude/settings.local.json`. This allows agent-specific configuration while inheriting project defaults.

## Session

An **execution environment for an agent** - where Claude actually runs.

A session provides:
- **Isolation**: Separate process/environment
- **Monitoring**: Heartbeat detection for stuck agents
- **Control**: Nudge, restart, or kill stuck sessions

### Session Properties

| Property | Description |
|----------|-------------|
| `id` | Session identifier (tmux session name) |
| `cwd` | Working directory (typically the workspace path) |
| `env` | Environment variables passed to the agent |

### Heartbeat Detection

Sessions track activity to detect stuck agents. The primary heartbeat method is **terminal output** - if a session produces no output for `idle_timeout`, it's considered stuck.

```toml
[agent.fix]
heartbeat = "output"
idle_timeout = "3m"
on_stuck = ["nudge", "restart"]
```

When idle too long, recovery actions trigger in order: nudge → restart → escalate.

**Why output-based heartbeat works**: Claude Code produces regular output during tool calls. Extended silence typically means the agent is stuck, not thinking. For agents that legitimately go quiet, increase `idle_timeout`.

## Relationship to Runbooks

```
┌─────────────────────────────────────────────────────────┐
│  Runbook                                                │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐ │
│  │   Worker    │───►│  Pipeline   │───►│    Agent    │ │
│  └─────────────┘    └─────────────┘    └─────────────┘ │
└─────────────────────────────────────────────────────────┘
                            │                   │
                            ▼                   ▼
┌─────────────────────────────────────────────────────────┐
│  Execution                                              │
│  ┌─────────────┐         ┌─────────────┐               │
│  │  Workspace  │◄────────│   Session   │               │
│  │ (git worktree)        │   (tmux)    │               │
│  └─────────────┘         └─────────────┘               │
└─────────────────────────────────────────────────────────┘
```

- **Pipeline** creates and owns a **Workspace**
- **Agent** runs in a **Session** within that workspace
- Session's `cwd` points to the workspace path
- Multiple agents in a pipeline share the same workspace

## Summary

| Concept | Purpose | Implementation |
|---------|---------|----------------|
| **Workspace** | Isolated work context | Git worktree |
| **Session** | Where agent runs | Tmux session |

These abstractions enable the same runbook to work across different environments. The runbook defines *what* to do; the execution layer handles *where* and *how*.

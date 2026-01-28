# Execution Model

Two abstractions sit beneath runbooks, decoupling "what to run" from "where to run it."

```text
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

**Storage location**: `~/.local/state/oj/projects/<project-hash>/workspaces/<name>/`

Using XDG state directory keeps the project directory clean and survives `git clean` operations.

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

### Session State Detection

Sessions are monitored via Claude's JSONL session log, not arbitrary timeouts:

```toml
[agent.fix]
on_idle = { action = "nudge", message = "Continue working on the task." }
on_exit = { action = "recover", message = "Previous attempt exited. Try again." }
on_error = "escalate"
```

**State detection from session log:**

| State | Log Indicator | Trigger |
|-------|--------------|---------|
| Working | `stop_reason: "tool_use"` or recent `user` line | Keep monitoring |
| Waiting for input | `stop_reason: "end_turn"`, no new `user` line | `on_idle` |
| API error | Error in log (unauthorized, quota, network) | `on_error` |

**Process exit detection:**

| Check | Method | Trigger |
|-------|--------|---------|
| tmux alive | `tmux has-session` | Session gone |
| Claude alive | `pgrep -P <pane_pid>` | `on_exit` |

**Why log-based detection works**: Claude Code writes structured JSONL logs with explicit turn boundaries. When `stop_reason: "end_turn"` appears with no subsequent user message, Claude is waiting for input - the exact moment to nudge.

Agents can run indefinitely. There's no timeout.

## Relationship to Runbooks

```
┌─────────────────────────────────────────────────────────┐
│  Runbook                                                │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐  │
│  │   Worker    │───►│  Pipeline   │───►│    Agent    │  │
│  └─────────────┘    └─────────────┘    └─────────────┘  │
└─────────────────────────────────────────────────────────┘
                            │                   │
                            ▼                   ▼
┌─────────────────────────────────────────────────────────┐
│  Execution                                              │
│  ┌─────────────┐         ┌─────────────┐                │
│  │  Workspace  │◄────────│   Session   │                │
│  │ (git worktree)        │   (tmux)    │                │
│  └─────────────┘         └─────────────┘                │
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

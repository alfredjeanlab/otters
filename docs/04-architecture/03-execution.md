# Execution Layer

This document details the design of the execution layer: Workspaces and Sessions.

## Overview

The execution layer manages **where** work runs:

```
┌─────────────────────────────────────────────────────────┐
│                     Workspace                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │
│  │  Worktree   │  │   Config    │  │   Context   │      │
│  │ (isolation) │  │  (.claude)  │  │  (CLAUDE.md)│      │
│  └─────────────┘  └─────────────┘  └─────────────┘      │
│                                                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │                    Session                         │ │
│  │  ┌─────────┐  ┌─────────────┐  ┌───────────────┐   │ │
│  │  │  tmux   │  │  Heartbeat  │  │   Recovery    │   │ │
│  │  │ process │  │   Monitor   │  │    Handler    │   │ │
│  │  └─────────┘  └─────────────┘  └───────────────┘   │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Workspace

Workspaces provide isolated environments for operations.

### Data Structure

```rust
pub struct Workspace {
    id: WorkspaceId,
    kind: WorkspaceKind,
    path: PathBuf,
    branch: String,
    pipeline_id: Option<PipelineId>,
    created_at: Instant,
    state: WorkspaceState,
}

pub enum WorkspaceKind {
    GitWorktree {
        base_repo: PathBuf,
        worktree_name: String,
    },
    Directory {
        original: PathBuf,
    },
    Container {
        image: String,
        container_id: Option<String>,
    },
}

pub enum WorkspaceState {
    Creating,
    Ready,
    InUse { session: SessionId },
    Dirty,  // Has uncommitted changes
    Stale,  // Branch deleted or merged
    Removing,
}
```

### State Transitions

```
transition(event, clock) → (Workspace, Vec<Effect>):
    match (state, event):
        (Creating, SetupComplete) → Ready, emit workspace:ready
        (Ready, SessionStarted) → InUse(session)
        (InUse, SessionEnded(clean=true)) → Ready
        (InUse, SessionEnded(clean=false)) → Dirty, emit workspace:dirty
        (Ready|Dirty, BranchGone) → Stale, emit workspace:stale
        (Ready|Stale, Remove) → Removing, effect WorktreeRemove
        _ → no change
```

Note: Cannot remove Dirty workspaces (uncommitted changes would be lost).

### Workspace Factory (Imperative Shell)

The factory creates workspaces via adapters:
1. Generate workspace ID and path
2. Create worktree/directory/container based on kind
3. Setup `.claude/settings.local.json` with permissions
4. Return workspace in Ready state

## Session

Sessions are execution environments where agents run.

### Data Structure

```rust
pub struct Session {
    id: SessionId,
    workspace: WorkspaceId,
    state: SessionState,
    heartbeat: HeartbeatState,
    recovery: RecoveryState,
    created_at: Instant,
}

pub enum SessionState {
    Starting,
    Running { pid: Option<u32> },
    Idle { since: Instant },
    Dead { reason: DeathReason },
}

pub enum DeathReason {
    Completed,
    Killed,
    Crashed { exit_code: i32 },
    Lost,  // Process vanished
}

pub struct HeartbeatState {
    last_seen: Option<Instant>,
    signals: HeartbeatSignals,
    idle_threshold: Duration,
}

pub struct HeartbeatSignals {
    terminal_output: Option<Instant>,
    log_write: Option<Instant>,
    file_write: Option<Instant>,
    api_call: Option<Instant>,
}

pub struct RecoveryState {
    attempts: u32,
    max_attempts: u32,
    last_action: Option<RecoveryAction>,
    chain: Vec<RecoveryAction>,
}

#[derive(Clone, PartialEq)]
pub enum RecoveryAction {
    Nudge,
    Restart,
    Escalate,
}
```

### Heartbeat Detection

Heartbeat combines multiple signals (terminal output, log writes, file writes, API calls) to determine activity:

```
evaluate_heartbeat(signals, clock) → (Session, Vec<Effect>):
    latest_activity = max(signals.terminal, signals.log, signals.file, signals.api)

    match (state, latest_activity):
        (Running, recent_activity) → stay Running
        (Running, stale_activity) → Idle, emit session:idle
        (Running, no_activity past threshold) → Idle, emit session:idle
        (Idle, recent_activity) → Running, emit session:active
```

### Recovery Chain

Recovery walks through a configured chain of actions:

```
next_recovery_action():
    if not Idle → None
    walk chain [Nudge, Restart(up to max), Escalate]
    return next action based on attempts, or None if exhausted

apply_recovery(action) → (Session, Vec<Effect>):
    match action:
        Nudge → effect Send("\n")
        Restart → Starting, effects Kill + Spawn
        Escalate → emit escalate
```

### Session Runner (Imperative Shell)

The runner handles actual I/O:
- **spawn** - Create tmux session in workspace directory
- **collect_signals** - Check terminal output, log files, file writes, API calls in parallel

## Workspace-Session Integration

An `ExecutionContext` bundles a workspace, session, and pipeline ID. Creation is pure - it returns the context plus a `WorktreeAdd` effect. The shell then executes the effect and transitions the workspace to Ready.

## See Also

- [Runbook Core](02-runbook-core.md) - Pipeline and Task design
- [Adapters](06-adapters.md) - Session adapter implementation
- [Testing Strategy](07-testing.md) - Testing execution layer

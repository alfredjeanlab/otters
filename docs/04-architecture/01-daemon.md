# Daemon Architecture

The system splits into two processes: `oj` (CLI) and `ojd` (daemon).

## Purpose

The core purpose of oj is **background work dispatch** - running agents in isolated sessions while the user does other things. This requires a persistent process that:

1. Receives commands from CLI
2. Runs the event loop
3. Spawns and monitors agents
4. Drives pipelines through phases
5. Persists state for crash recovery

Future: dispatch work to remote machines. Current: local only.

## Process Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  oj (CLI)                                    crates/cli     │
│                                                             │
│  1. Find project root (walk up for .oj/ or runbook)         │
│  2. Hash project path → socket path                         │
│  3. Connect to project's daemon (auto-start if needed)      │
│  4. Send request, receive response                          │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          │  Unix socket: ~/.local/state/oj/projects/<hash>/daemon.sock
                          │  (Future: TCP for remote dispatch)
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  ojd (Daemon)                                crates/daemon  │
│                                                             │
│  Persistent process that owns the event loop:               │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Event Loop                                           │  │
│  │                                                       │  │
│  │  loop {                                               │  │
│  │      select! {                                        │  │
│  │          conn = socket.accept() => handle(conn)       │  │
│  │          event = internal.recv() => process(event)    │  │
│  │          _ = interval(1s) => check_heartbeats()       │  │
│  │      }                                                │  │
│  │  }                                                    │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  Owns:                                                      │
│  - State (pipelines, sessions, workspaces)                  │
│  - Adapters (tmux, git)                                     │
│  - Storage (WAL)                                            │
└─────────────────────────────────────────────────────────────┘
```

## Why a Daemon?

Without a daemon, each CLI invocation would need to:
- Load state from WAL
- Execute one step
- Exit

This creates problems:
- **No continuous monitoring** - can't detect stuck agents
- **Race conditions** - multiple CLI invocations could conflict
- **No event loop** - shell completions can't trigger next phases
- **Inefficient** - constant WAL replay on every command

The daemon solves these by being a single owner of state and the event loop.

## IPC Protocol

**Transport:** Unix socket (local), TCP (future remote)

**Format:** Length-prefixed JSON

```
enum Request {
    Ping                      // Health check (quick)
    Hello { version }         // Version handshake
    Status                    // Detailed status
    Event(Event)              // Deliver event to event loop
    Query(Query)              // Read state
    Shutdown                  // Graceful shutdown
}

enum Query {
    ListPipelines
    GetPipeline(id)
    ListSessions
}

enum Response {
    Pong                      // Response to Ping
    Hello { version }         // Response with daemon version
    Ok
    Event { accepted }
    Pipelines([...])
    Pipeline(data | null)
    Sessions([...])
    Error { message }
    Status { uptime_secs, pipelines_active, sessions_active }
    ShuttingDown              // Response to Shutdown
}
```

## Event Loop

The daemon runs a continuous event loop:

```
┌─────────────────────────────────────────────────────────┐
│                     Event Sources                       │
│                                                         │
│  ┌─────────┐  ┌─────────────┐  ┌──────────┐             │
│  │   IPC   │  │  Internal   │  │  Timers  │             │
│  │ (socket)│  │   Queue     │  │          │             │
│  └────┬────┘  └──────┬──────┘  └────┬─────┘             │
│       │              │              │                   │
│       └──────────────┼──────────────┘                   │
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │  Event Loop   │                          │
│              └───────┬───────┘                          │
│                      │                                  │
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │    Runtime    │                          │
│              │  (engine)     │                          │
│              └───────┬───────┘                          │
│                      │                                  │
│                      ▼                                  │
│              ┌───────────────┐                          │
│              │   Effects     │───────┐                  │
│              └───────────────┘       │                  │
│                      │               │                  │
│      ┌───────────────┼───────────────┼──────┐           │
│      ▼               ▼               ▼      ▼           │
│  ┌───────┐     ┌─────────┐    ┌──────┐ ┌───────┐        │
│  │ tmux  │     │   git   │    │ WAL  │ │ Queue │        │
│  │Adapter│     │ Adapter │    │      │ │(intern)│       │
│  └───────┘     └─────────┘    └──────┘ └───────┘        │
│                                              │          │
│                                              │          │
│                      Internal events ◄───────┘          │
│                      (ShellCompleted, etc.)             │
└─────────────────────────────────────────────────────────┘
```

Effects that produce events (like `Effect::Shell`) feed results back into the internal queue, creating the progression chain.

## Lifecycle

### Startup

```
1. Write startup marker to log ("--- ojd: starting (pid: <pid>) ---")
2. Acquire lock file (prevent multiple daemons)
3. Load state from WAL
4. Reconcile with reality (check sessions, workspaces)
5. Bind socket
6. Enter event loop
```

**Startup Error Reporting:**

When the CLI starts the daemon and it fails, errors are reported via the log:

1. Daemon writes `--- ojd: starting (pid: <pid>) ---` marker before anything else
2. CLI waits for socket to appear (with timeout)
3. If timeout, CLI reads log from last marker, extracts ERROR lines
4. Error message shown to user instead of generic "timeout"

This ensures runbook parse errors, permission issues, etc. are visible to the user.

### Shutdown

```
1. Stop accepting new connections
2. Drain pending events
3. Persist final state
4. Release lock file
5. Exit
```

### Recovery

On restart after crash:

```
1. Replay WAL to reconstruct state
2. Reconcile:
   - Check which sessions are still alive
   - Check which workspaces exist
   - Identify in-progress pipelines
3. Resume or fail stale pipelines
```

The WAL records intent; reconciliation bridges the gap with reality.

## Daemon Management

```bash
# Start daemon (background)
oj daemon start

# Start in foreground (debugging)
oj daemon start --foreground

# Check status
oj daemon status

# Stop gracefully
oj daemon stop

# View logs
oj daemon logs
```

### Auto-Start

The daemon auto-starts on first command if not already running:

```
connect_or_start():
    if can connect to socket:
        return connection
    else:
        start daemon in background
        retry connect with 5s timeout
        return connection
```

This provides seamless UX - users don't need to think about daemon lifecycle for normal usage. Explicit `oj daemon start` is only needed for debugging or custom configurations.

## Project Discovery

CLI finds the project root by walking up from cwd looking for `.oj/` directory.

```text
<project>/
└── .oj/
    ├── config.toml          # Project config (optional)
    └── runbooks/            # Runbook files
        ├── build.toml
        ├── bugfix.toml
        └── ...
```

The directory containing `.oj/` is the **project root**.

## Directory Layout

Each project has its own daemon instance:

```text
~/.local/state/oj/
└── projects/
    └── <project-hash>/      # sha256(canonical_project_path)[0:16]
        ├── daemon.sock      # Unix socket
        ├── daemon.pid       # Lock file (contains PID)
        ├── daemon.version   # Version file (for mismatch detection)
        ├── daemon.log       # Logs
        ├── wal/
        │   └── events.wal   # Write-ahead log
        └── workspaces/      # Workspaces for this project
            └── <name>/
```

**Project hash:** `sha256(canonical_path)[0:16]` where canonical_path is the resolved absolute path to the project root.

**Why per-project:**
- Isolation between projects
- No need to pass project context on every request
- Daemon knows its runbook location
- Simple socket path calculation

**Future:** Configurable shared daemon for multi-project setups (via `.oj/config.toml` workspace setting).

## Future: Remote Dispatch

The daemon architecture enables future remote dispatch:

```
Local machine                          Remote machine
┌─────────────┐                       ┌─────────────┐
│     oj      │ ───── TCP ──────────► │     ojd     │
│   (CLI)     │                       │  (daemon)   │
└─────────────┘                       └─────────────┘
```

Same protocol, different transport. The CLI doesn't need to know where work executes.

Considerations for remote:
- Authentication (API keys, mTLS)
- Session management (remote tmux vs local)
- Workspace sync (git clone vs local worktree)
- Latency handling (async responses, streaming)

## See Also

- [Overview](00-overview.md) - System architecture
- [Effects](02-effects.md) - Effect types
- [Storage](04-storage.md) - WAL persistence

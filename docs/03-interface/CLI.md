# CLI Reference

The `oj` command is a thin client that communicates with the `ojd` daemon. Most commands send events or queries over a Unix socket; the daemon owns the event loop and state.

See [Daemon Architecture](../04-architecture/01-daemon.md) for details on the process split.

## Project Structure

```text
<project>/
└── .oj/
    ├── config.toml          # Project config (optional)
    └── runbooks/            # Runbook files
        ├── build.toml       # oj run build ...
        ├── bugfix.toml      # oj run fix ...
        └── ...
```

CLI finds the project root by walking up from cwd looking for `.oj/` directory.

## Daemon

### oj daemon

Manage the background daemon.

```bash
oj daemon start              # Start daemon (background)
oj daemon start --foreground # Start in foreground (debugging)
oj daemon stop               # Graceful shutdown
oj daemon status             # Health check
oj daemon logs               # View daemon logs
```

The daemon auto-starts on first command if not already running.
Explicit `oj daemon start` is only needed for debugging or custom configurations.

## Entrypoints

### oj run

Execute commands defined in runbooks.

```bash
oj run <command> [args...]
oj run build auth "Add authentication"
oj run build auth "Add auth" --priority 1
```

### oj worker

Manage queue-driven daemons.

```bash
oj worker list
oj worker start <name>
oj worker stop <name>
oj worker wake <name>
```

### oj cron

Manage time-driven daemons.

```bash
oj cron list
oj cron enable <name>
oj cron disable <name>
oj cron run <name>              # Run once now
```

## Resources

### oj pipeline

Manage running pipelines.

```bash
oj pipeline list
oj pipeline show <id>
oj pipeline transition <id> <phase>
oj pipeline resume <id>
oj pipeline checkpoint <id>
```

### oj queue

Manage work queues.

```bash
oj queue list <name>
oj queue add <name> [data...]
oj queue take <name>
oj queue complete <name> <id>
oj queue requeue <name> <id>
```

### oj session

Manage execution sessions.

```bash
oj session list
oj session show <id>
oj session idle-time <id>
oj session nudge <id>
oj session kill <id>
```

### oj workspace

Manage isolated work contexts.

```bash
oj workspace list
oj workspace show <name>
oj workspace create <name>
oj workspace delete <name>
```

### oj lock

Manage exclusive locks.

```bash
oj lock list
oj lock acquire <name>
oj lock release <name>
oj lock force-release <name>
```

### oj semaphore

Manage limited concurrency.

```bash
oj semaphore list
oj semaphore status <name>
oj semaphore acquire <name>
oj semaphore release <name>
```

## Events

### oj emit

Publish events.

```bash
oj emit <event> [--data key=val...]
oj emit pipeline:complete --id build-auth
oj emit build:queued --id auth
```

## Agent Signaling

Commands for agents to signal orchestrators:

```bash
oj done                       # Signal success
oj done --error "reason"      # Signal failure
oj done --restart             # Request fresh session
oj checkpoint "message"       # Record progress
```

## Environment Variables

Runtime context passed to agents:

| Variable | Purpose |
|----------|---------|
| `OJ_PROJECT_ROOT` | Project root path (for daemon connection) |
| `OJ_PIPELINE` | Pipeline identifier |
| `OJ_PHASE` | Current pipeline phase |
| `OJ_WORKSPACE` | Workspace path |

## JSON Output

Most commands support `--json` for programmatic use:

```bash
oj pipeline list --json
oj queue list bugs --json
```

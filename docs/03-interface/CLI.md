# CLI Reference

The `oj` command manages runbook execution.

## Commands

### oj run

Execute user-facing commands defined in runbooks.

```bash
oj run <command> [args...] [flags...]
oj run build auth "Add authentication"
oj run build auth "Add authentication" --priority 1
oj run build auth "Add authentication" -p 1
oj run bugfix 42
```

Arguments follow the command's `args` specification:
- Positional args in order
- Flags by name (`--priority`) or alias (`-p`)
- Variadic args consume remaining positionals

### oj worker

Manage queue-driven daemons.

```bash
oj worker list                    # List workers
oj worker start <name>            # Start worker
oj worker stop <name>             # Stop worker
oj worker wake <name>             # Wake idle worker
oj worker status <name>           # Check status
```

### oj cron

Manage time-driven daemons.

```bash
oj cron list                      # List crons
oj cron enable <name>             # Enable cron
oj cron disable <name>            # Disable cron
oj cron run <name>                # Run once now
```

### oj queue

Manage work queues.

```bash
oj queue list <name>              # List queue items
oj queue add <name> [data...]     # Enqueue item
oj queue take <name>              # Take next item
oj queue complete <name> <id>     # Mark complete
oj queue requeue <name> <id>      # Put back
oj queue dead <name> <id>         # Move to dead letter
```

### oj pipeline

Manage running pipeline instances.

```bash
oj pipeline list                  # List running pipelines
oj pipeline show <id>             # Pipeline state and details
oj pipeline transition <id> <phase>   # Change phase
oj pipeline resume <id>           # Resume paused pipeline
oj pipeline checkpoint <id>       # Save progress
oj pipeline error <id>            # Get last error
```

### oj lock

Manage exclusive locks.

```bash
oj lock list                      # List locks
oj lock acquire <name>            # Acquire lock
oj lock release <name>            # Release lock
oj lock force-release <name>      # Force release stale lock
oj lock is-stale <name>           # Check if stale
```

### oj semaphore

Manage limited concurrency.

```bash
oj semaphore list                 # List semaphores
oj semaphore status <name>        # Show slots used/available
oj semaphore acquire <name>       # Acquire slot
oj semaphore release <name>       # Release slot
```

### oj session

Manage execution sessions.

```bash
oj session list                   # List active sessions
oj session show <id>              # Session details
oj session idle-time <id>         # Time since last activity
oj session nudge <id>             # Send interrupt
oj session kill <id>              # Terminate session
```

### oj workspace

Manage isolated work contexts.

```bash
oj workspace list                 # List workspaces
oj workspace show <name>          # Workspace details
oj workspace create <type> <name> # Create workspace
oj workspace delete <name>        # Remove workspace
```

### oj emit

Publish events for observability and triggers.

```bash
oj emit <event> [--data key=val...]
oj emit pipeline:complete --id build-auth
oj emit bug:created --id 42
```

## Agent Signaling

Commands for agents to signal orchestrators:

```bash
oj done                           # Signal success
oj done --error "reason"          # Signal failure
oj done --restart                 # Request fresh session
oj checkpoint "message"           # Record progress
oj confirm "action"               # Request approval (when OTTER_SAFE=true)
```

## Environment Variables

Runtime context passed to agents:

| Variable | Purpose |
|----------|---------|
| `OTTER_TASK` | Current task identifier |
| `OTTER_WORKSPACE` | Workspace name |
| `OTTER_PHASE` | Current pipeline phase |
| `OTTER_SAFE` | Enable confirmation prompts |

## JSON Output

Most list/show commands support `--json` for programmatic use:

```bash
oj pipeline list --json
oj queue list bugs --json
oj session show abc123 --json
```

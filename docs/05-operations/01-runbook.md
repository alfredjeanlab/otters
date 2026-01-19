# Operational Runbook

This runbook covers day-to-day operations for the oj orchestration system.

## Starting the System

### Start the Daemon

```bash
oj daemon start
```

The daemon handles:
- Polling for session output
- Detecting stuck tasks
- Processing tick events
- Managing scheduled jobs

### Verify Health

```bash
# Quick health check
oj status

# Detailed diagnostics
oj status --verbose
```

## Common Operations

### View Active Work

```bash
# List active pipelines
oj pipeline list --active

# List all sessions
oj session list

# List workspaces
oj workspace list
```

### Handle Stuck Pipeline

1. Check pipeline status:
   ```bash
   oj pipeline status <pipeline-id>
   ```

2. View session output:
   ```bash
   oj session capture <session-id> --tail 50
   ```

3. Nudge the agent:
   ```bash
   oj pipeline nudge <pipeline-id>
   ```

4. If still stuck, restart:
   ```bash
   oj pipeline restart <pipeline-id>
   ```

### Release Stale Lock

```bash
# Check lock status
oj lock status merge-lock

# Force release (use with caution)
oj lock release merge-lock --force
```

## Monitoring

### Resource Usage

```bash
oj status --resources
```

Key metrics:
- Active sessions (limit: 10)
- Memory usage
- WAL size

### WAL Size

```bash
ls -lh .oj/
```

If WAL grows large (>100MB), trigger compaction:
```bash
oj maintenance compact
```

## Maintenance Tasks

### Compact WAL

```bash
oj maintenance compact
```

This removes completed operations and reduces WAL size.

### Clean Up Stale Workspaces

```bash
oj workspace cleanup --stale
```

### Archive Old Pipeline State

```bash
oj maintenance archive --older-than 30d
```

## Shutdown Procedures

### Graceful Shutdown

```bash
oj daemon stop
```

This will:
1. Stop accepting new work
2. Wait for in-progress tasks to complete
3. Checkpoint all sessions
4. Release all locks
5. Sync WAL

### Emergency Shutdown

```bash
oj daemon stop --force
```

Use only when graceful shutdown hangs.

## Backup and Recovery

### Create Snapshot

```bash
oj maintenance snapshot
```

Snapshots are stored in `.oj/snapshots/`.

### Restore from Snapshot

```bash
oj maintenance restore-snapshot --latest
```

Or restore a specific snapshot:
```bash
oj maintenance restore-snapshot --id <snapshot-id>
```

### WAL Recovery

If WAL corruption is detected:
```bash
oj daemon start --recover
```

This truncates the corrupted portion and replays valid entries.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OTTER_TASK` | Current pipeline name | (none) |
| `OTTER_WORKSPACE` | Workspace directory | (none) |
| `OTTER_PHASE` | Current phase | (none) |
| `OJ_LOG_LEVEL` | Logging level | `info` |

## Directory Structure

```
.oj/
├── wal.jsonl              # Write-ahead log (append-only)
└── snapshots/             # Point-in-time snapshots
    └── {sequence}-{timestamp}.json
```

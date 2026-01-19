# Troubleshooting Guide

Common issues and their resolutions.

## Pipeline Issues

### Pipeline Won't Start

**Symptoms**: `oj run build` hangs or returns immediately without starting

**Possible Causes**:
1. Lock held by another pipeline
2. Semaphore capacity exhausted
3. Guard condition not met

**Resolution**:
```bash
# Check locks
oj lock list

# Check semaphores
oj semaphore status agent-slots

# Check guards
oj pipeline guards <pipeline-id>
```

### Pipeline Stuck in Phase

**Symptoms**: Pipeline shows active but no progress

**Possible Causes**:
1. Claude CLI is waiting for input
2. Task timed out
3. Session died

**Resolution**:
```bash
# Check pipeline status
oj pipeline status <pipeline-id>

# View task state
oj pipeline show <pipeline-id> --verbose

# Nudge or restart
oj pipeline nudge <pipeline-id>
oj pipeline restart <pipeline-id>
```

## Session Issues

### Session Not Responding

**Symptoms**: Session shows as active but no output

**Possible Causes**:
1. Claude CLI hung
2. Network timeout
3. Rate limit exceeded

**Resolution**:
```bash
# Check session status
oj session status <session-id>

# View last output
oj session capture <session-id> --tail 50

# Send interrupt
oj session send <session-id> --interrupt

# If unresponsive, kill and restart
oj session kill <session-id>
```

### Session Died Unexpectedly

**Symptoms**: Pipeline failed with "session terminated"

**Possible Causes**:
1. OOM killed
2. tmux server crashed
3. Manual intervention

**Resolution**:
```bash
# Check tmux status
tmux list-sessions

# View system logs
journalctl -u oj --since "1 hour ago"

# Restart the pipeline
oj pipeline restart <pipeline-id>
```

## Lock Issues

### Lock Acquisition Timeout

**Symptoms**: Pipeline waiting for lock indefinitely

**Possible Causes**:
1. Holder pipeline is stuck
2. Holder process crashed without releasing
3. Network partition (distributed locks)

**Resolution**:
```bash
# Check who holds the lock
oj lock status <lock-id>

# If holder is stuck, check its status
oj pipeline status <holder-pipeline-id>

# Force release if holder is gone
oj lock release <lock-id> --force
```

### Stale Lock Detection

**Symptoms**: Lock shows held but holder is not active

**Resolution**:
```bash
# List stale locks
oj lock list --stale

# Release stale locks
oj lock release <lock-id> --force
```

## Storage Issues

### WAL Corruption

**Symptoms**: Engine fails to start with "WAL checksum mismatch"

**Possible Causes**:
1. Crash during write
2. Disk corruption
3. File system full

**Resolution**:
```bash
# Try recovery mode
oj daemon start --recover

# If recovery fails, restore from snapshot
oj maintenance restore-snapshot --latest

# Check disk health
df -h .oj/
```

### WAL Growing Too Large

**Symptoms**: `.oj/wal.jsonl` consuming excessive disk space

**Resolution**:
```bash
# Check WAL size
du -sh .oj/wal.jsonl

# Trigger compaction
oj maintenance compact

# If compaction fails, check for long-running operations
oj pipeline list --all
```

### State Corruption

**Symptoms**: JSON parse error when loading state

**Resolution**:
```bash
# Restore from latest snapshot
oj maintenance restore-snapshot --latest

# Or rebuild state by replaying WAL
oj daemon start --rebuild-state
```

## Resource Issues

### Session Limit Reached

**Symptoms**: "Session limit exceeded" error

**Resolution**:
```bash
# Check current sessions
oj session list

# Kill inactive sessions
oj session cleanup

# Increase limit (if appropriate)
export OJ_MAX_SESSIONS=15
oj daemon restart
```

### Memory Usage High

**Symptoms**: System slowdown, OOM warnings

**Resolution**:
```bash
# Check resource status
oj status --resources

# Compact WAL to reduce memory
oj maintenance compact

# Reduce concurrent sessions
oj session cleanup
```

## Network Issues

### Claude API Timeout

**Symptoms**: Tasks timing out with network errors

**Possible Causes**:
1. Network connectivity issues
2. API rate limiting
3. API service degradation

**Resolution**:
```bash
# Check API status
curl -s https://status.anthropic.com/api/v2/status.json | jq .

# Test connectivity
curl -I https://api.anthropic.com

# If rate limited, wait and retry
oj pipeline resume <pipeline-id>
```

## Daemon Issues

### Daemon Won't Start

**Symptoms**: `oj daemon start` hangs or fails

**Possible Causes**:
1. Port already in use
2. Permission issues
3. Corrupted state

**Resolution**:
```bash
# Check for existing daemon
pgrep -f "oj daemon"

# Kill stale process
pkill -f "oj daemon"

# Check permissions
ls -la .oj/

# Start with verbose logging
OJ_LOG_LEVEL=debug oj daemon start
```

### Daemon Crashes Repeatedly

**Resolution**:
```bash
# Start in foreground for debugging
OJ_LOG_LEVEL=debug oj daemon start --foreground

# If WAL issue, try recovery
oj daemon start --recover
```

## Getting Help

If issues persist:

1. Collect diagnostics:
   ```bash
   oj status --verbose > diagnostics.txt
   oj pipeline list --all >> diagnostics.txt
   ```

2. Check the GitHub issues: https://github.com/anthropics/otters/issues

3. File a new issue with the diagnostics attached

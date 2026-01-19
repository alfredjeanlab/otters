# Migration Guide: From Bash Scripts to oj

This guide covers migrating from the legacy bash scripts (`feature`, `bugfix`, `mergeq`) to the oj orchestration system.

## Overview

The oj system replaces the bash-based workflow scripts with a unified Rust-based orchestrator that provides:

- Structured state management with WAL persistence
- Better error handling and recovery
- Resource limits and monitoring
- Shell completions
- Unified CLI interface

## Prerequisites

1. Rust toolchain installed (`cargo`, `rustc`)
2. No running bash-based processes

## Quick Migration

### 1. Stop Running Processes

```bash
# Stop any running bash processes
pkill -f mergeq
pkill -f feature-daemon
pkill -f bugfix-daemon

# Verify nothing is running
pgrep -f "mergeq|feature-daemon"
```

### 2. Run Migration Script

```bash
# Preview what will change
./scripts/migrate-from-bash.sh --dry-run

# Run the migration
./scripts/migrate-from-bash.sh
```

### 3. Build and Install

```bash
# Build release binary
cargo build --release

# Add to PATH (or copy to a bin directory)
export PATH="$PATH:$(pwd)/target/release"
```

### 4. Verify Installation

```bash
# Check oj is working
oj --version
oj status
```

## Command Mapping

| Old Command | New Command | Notes |
|-------------|-------------|-------|
| `feature start` | `oj run feature` | Starts feature pipeline |
| `bugfix start` | `oj run bugfix` | Starts bugfix pipeline |
| `mergeq add` | `oj queue add` | Add to merge queue |
| `mergeq list` | `oj queue list` | List queue items |
| `mergeq status` | `oj queue status` | Queue status |

## Shell Aliases

For backward compatibility, add these aliases to your shell profile:

```bash
# ~/.bashrc or ~/.zshrc

# Pipeline aliases
alias feature='oj run feature'
alias bugfix='oj run bugfix'

# Queue aliases
alias mergeq='oj queue'

# Common operations
alias feature-status='oj pipeline list'
alias feature-kill='oj pipeline stop'
```

## State Migration

### Merge Queue

Queue items are automatically migrated by the migration script. If manual migration is needed:

```bash
# Export from old format
cat ~/.feature-state/merge-queue.json | jq -r '.[] | .branch' > branches.txt

# Import to oj
while read branch; do
    oj queue add "$branch"
done < branches.txt
```

### Worktrees

Git worktrees are compatible between systems. oj will automatically detect existing worktrees in `.worktrees/` or any configured worktree directory.

### State Files

Old state files in `~/.feature-state/` are backed up by the migration script. You can safely delete the backup after confirming oj works correctly.

## Configuration Changes

### Environment Variables

| Old Variable | New Variable | Notes |
|--------------|--------------|-------|
| `FEATURE_DIR` | (automatic) | oj detects git root |
| `MERGEQ_PARALLEL` | `OJ_MAX_SESSIONS` | Session limit |
| `DEBUG` | `OJ_LOG_LEVEL=debug` | Verbose logging |

### Config Files

oj uses a different configuration format. Convert old configs:

```bash
# Old: ~/.feature-config
# New: .oj/config.toml (in project root)
```

## Troubleshooting

### oj command not found

```bash
# Ensure it's built
cargo build --release

# Check it exists
ls -la target/release/oj

# Add to PATH
export PATH="$PATH:$(pwd)/target/release"
```

### Old processes still running

```bash
# Find and kill any remaining processes
ps aux | grep -E "feature|bugfix|mergeq"
pkill -9 -f "feature|bugfix|mergeq"
```

### State not migrating

```bash
# Manual state inspection
ls -la ~/.feature-state/
cat ~/.feature-state/merge-queue.json | jq .

# Manual import
oj queue import ~/.feature-state/merge-queue.json --format legacy
```

## Rollback

If you need to roll back to bash scripts:

```bash
# Stop oj
oj daemon stop

# Restore backup
BACKUP=$(ls -td ~/.feature-state.backup.* | head -1)
mv "$BACKUP" ~/.feature-state

# Remove oj state (optional)
rm -rf .build/
```

## Verification Checklist

- [ ] Old bash processes stopped
- [ ] Migration script completed successfully
- [ ] oj binary built and in PATH
- [ ] `oj status` runs without error
- [ ] Queue items migrated (if any)
- [ ] Shell completions installed (optional)
- [ ] Aliases added to shell profile (optional)

# Storage

How state syncs across machines in an offline-first system.

## Write-Ahead Log

All state changes append to a WAL. The WAL is the sync protocol:

- **Offline-first**: Each machine has its own WAL, works independently
- **Sync**: WAL entries replicate between machines via websocket
- **Conflict resolution**: Compare operation sequences, merge or reject
- **Audit**: Complete history across all machines

Local state is a derived view of the WAL - it can be rebuilt by replaying entries.

## Operations

What goes in the WAL:

| Operation | Example |
|-----------|---------|
| Pipeline transitions | `pipeline:phase build-auth plan → execute` |
| Queue operations | `queue:enqueue merges branch=fix/123` |
| Lock changes | `lock:acquire main_branch holder=build-auth` |
| Worker events | `worker:started bugfix machine=laptop` |

Each entry includes timestamp, machine ID, and operation-specific data.

## Multi-Machine

Machines connect to a central server and sync WALs:

- **Catch-up**: Reconnecting machines receive missed entries
- **Partition**: Machines continue working offline, reconcile on reconnect

## Conflict Handling

When WAL entries conflict:

| Conflict | Resolution |
|----------|------------|
| Same lock acquired | Leader's entry wins |
| Same queue item claimed | First timestamp wins |
| Pipeline state diverged | Merge if compatible, escalate if not |

Conflicts are rare with proper leader coordination. Offline work targets independent resources.

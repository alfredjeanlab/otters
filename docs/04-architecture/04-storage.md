# Storage Layer

Write-ahead log (WAL) for durable state persistence with crash recovery.

## Architecture

```
Operation → WAL (append + fsync) → Materialized State
                    ↓
              Snapshots (periodic)
```

State is derived from WAL. On startup, load latest snapshot then replay WAL entries.

## WAL Entry Format

Each entry is a single line of JSON:

```json
{"sequence":1,"timestamp_micros":1705123456789000,"machine_id":"m1","operation":{...},"checksum":12345}
```

- **sequence**: Monotonic, never repeats
- **checksum**: CRC32 over the operation JSON
- **machine_id**: Reserved for future multi-machine sync

## Operations

```rust
pub enum Operation {
    // Pipeline lifecycle
    PipelineCreate { id, kind, name, inputs },
    PipelineTransition { id, phase, outputs },
    PipelineDelete { id },

    // Queue management
    QueueCreate { id },
    QueuePush { id, item },
    QueueClaim { id, item_id, holder },
    QueueComplete { id, item_id },
    QueueFail { id, item_id, reason },
    QueueRelease { id, item_id },

    // Coordination
    LockAcquire { name, holder },
    LockRelease { name, holder },
    LockHeartbeat { name, holder },
    SemaphoreAcquire { name, holder, slots },
    SemaphoreRelease { name, holder },

    // Sessions and workspaces
    SessionCreate { id, workspace_id },
    SessionHeartbeat { id, output_hash },
    SessionDelete { id },
    WorkspaceCreate { id, path, branch },
    WorkspaceDelete { id },

    // Snapshot marker
    SnapshotTaken { sequence },
}
```

## Materialized State

State is rebuilt by replaying operations:

```rust
pub struct MaterializedState {
    pub pipelines: HashMap<PipelineId, Pipeline>,
    pub queues: HashMap<String, Queue>,
    pub locks: HashMap<String, LockState>,
    pub semaphores: HashMap<String, SemaphoreState>,
    pub sessions: HashMap<SessionId, Session>,
    pub workspaces: HashMap<WorkspaceId, Workspace>,
}
```

Each operation type has an `apply()` that updates state deterministically.

## Snapshots

Periodic snapshots compress history:

```rust
pub struct Snapshot {
    pub sequence: u64,      // WAL sequence at snapshot time
    pub state: MaterializedState,
}
```

Recovery: Load snapshot, replay only entries after `snapshot.sequence`.

## Compaction

When WAL grows large:
1. Take snapshot at current sequence
2. Rewrite WAL keeping only entries after snapshot
3. Delete old snapshots (keep N most recent)

## Corruption Handling

| Problem | Detection | Recovery |
|---------|-----------|----------|
| Truncated write | JSON parse fails | Truncate at last valid entry |
| Bit flip | CRC32 mismatch | Truncate at last valid entry |
| Missing entries | Sequence gap | Error (manual intervention) |

## Invariants

- Writes fsync before returning success
- Sequence numbers never decrease or repeat
- Replaying same entries produces identical state
- Every entry verified by checksum on read

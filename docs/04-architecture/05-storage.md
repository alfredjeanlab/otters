# Storage Layer

This document details the storage design: WAL, state persistence, and multi-machine sync.

## Overview

Storage follows an event-sourcing pattern with offline-first design:

```
┌─────────────────────────────────────────────────────────┐
│                    Storage Layer                        │
│                                                         │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │     WAL      │───▶│    State     │───▶│  Queries  │  │
│  │ (Operations) │    │(Materialized)│    │  (Views)  │  │
│  └──────────────┘    └──────────────┘    └───────────┘  │
│         │                                               │
│         ▼                                               │
│  ┌──────────────┐                                       │
│  │     Sync     │ ◀──────▶ Other Machines               │
│  │  (Replicate) │                                       │
│  └──────────────┘                                       │
└─────────────────────────────────────────────────────────┘
```

## Write-Ahead Log (WAL)

The WAL is the source of truth. All state changes are operations in the log.

### Data Structure

```rust
pub struct Wal {
    path: PathBuf,
    machine_id: MachineId,
    entries: Vec<WalEntry>,
    next_sequence: u64,
}

pub struct WalEntry {
    sequence: u64,
    timestamp: Instant,
    machine_id: MachineId,
    operation: Operation,
    checksum: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Operation {
    // Pipeline operations
    PipelineCreate {
        id: PipelineId,
        runbook: String,
        branch: String,
        context: Value,
    },
    PipelineTransition {
        id: PipelineId,
        from_phase: String,
        to_phase: String,
    },
    PipelineCheckpoint {
        id: PipelineId,
        message: String,
    },
    PipelineComplete {
        id: PipelineId,
        result: PipelineResult,
    },

    // Queue operations
    QueuePush {
        queue: QueueId,
        item_id: ItemId,
        data: Value,
        priority: i32,
    },
    QueueClaim {
        queue: QueueId,
        item_id: ItemId,
        claim_id: ClaimId,
        visibility_timeout_secs: u64,
    },
    QueueComplete {
        queue: QueueId,
        claim_id: ClaimId,
    },
    QueueFail {
        queue: QueueId,
        claim_id: ClaimId,
        reason: String,
    },
    QueueRelease {
        queue: QueueId,
        claim_id: ClaimId,
    },
    QueueTick {
        queue: QueueId,
        tick_result_json: String,
    },

    // Coordination operations
    LockAcquire {
        lock: String,
        holder: HolderId,
    },
    LockRelease {
        lock: String,
        holder: HolderId,
    },
    LockHeartbeat {
        lock: String,
        holder: HolderId,
    },
    SemaphoreAcquire {
        semaphore: String,
        holder: HolderId,
        slots: u32,
    },
    SemaphoreRelease {
        semaphore: String,
        holder: HolderId,
    },

    // Workspace operations
    WorkspaceCreate {
        id: WorkspaceId,
        path: PathBuf,
        branch: String,
    },
    WorkspaceRemove {
        id: WorkspaceId,
    },

    // Session operations
    SessionStart {
        id: SessionId,
        workspace: WorkspaceId,
    },
    SessionHeartbeat {
        id: SessionId,
    },
    SessionEnd {
        id: SessionId,
        reason: SessionEndReason,
    },

    // Worker operations
    WorkerStart {
        id: WorkerId,
    },
    WorkerStop {
        id: WorkerId,
    },

    // Compaction marker
    Snapshot {
        state_hash: String,
    },
}

pub struct MachineId(String);
```

### WAL Operations

```
append(operation, clock) → (Wal, WalEntry):
    create entry with next_sequence, timestamp, machine_id, operation
    return (new Wal with entry appended, entry)

replay() → State:
    fold over entries, applying each operation to state

since(seq) → [WalEntry]:
    return entries starting from seq (for sync)

merge(remote_entries) → Result<Wal, MergeConflict>:
    if remote_start > next_sequence → Gap error (missing entries)
    for overlapping entries:
        if same sequence but different machine → Divergence error
    append non-overlapping remote entries
```

```rust
pub enum MergeConflict {
    Gap { local_next: u64, remote_start: u64 },
    Divergence { sequence: u64, local: WalEntry, remote: WalEntry },
}
```

### WAL Writer (Imperative Shell)

The writer wraps the pure WAL with durable I/O:
- **append** - Compute new WAL (pure), write entry to disk with fsync, update in-memory state
- **load** - Read newline-delimited JSON entries from file, reconstruct WAL state

## State Materialization

State is derived from WAL by replaying operations.

### State Structure

```rust
pub struct State {
    pub pipelines: HashMap<PipelineId, Pipeline>,
    pub queues: HashMap<QueueId, Queue>,
    pub coordination: CoordinationState,
    pub workspaces: HashMap<WorkspaceId, Workspace>,
    pub sessions: HashMap<SessionId, Session>,
    pub workers: HashMap<WorkerId, Worker>,
}
```

```
apply(op) → State:
    match op:
        PipelineCreate → insert new pipeline
        PipelineTransition → update pipeline phase
        QueuePush → add item to queue
        LockAcquire → delegate to coordination.handle
        ... (each Operation variant updates corresponding state)
```

### State Store

The Store combines WAL persistence with in-memory materialized state:
- **open** - Load WAL from disk, replay to build state
- **execute** - Write operation to WAL (durability first), then apply to state
- **state** - Query current materialized state
- **wal** - Access WAL for sync operations

## Multi-Machine Sync

Machines sync via WAL entry replication.

### Sync Protocol

```
Machine A                    Server                    Machine B
    │                           │                           │
    │  connect + last_seq=42    │                           │
    │──────────────────────────▶│                           │
    │                           │                           │
    │  entries since 42         │                           │
    │◀──────────────────────────│                           │
    │                           │                           │
    │  apply entries            │                           │
    │                           │                           │
    │  new op (seq=47)          │                           │
    │──────────────────────────▶│                           │
    │                           │                           │
    │                           │  broadcast (seq=47)       │
    │                           │──────────────────────────▶│
    │                           │                           │
    │                           │                           │  apply
    │                           │                           │
```

### Sync State Machine (Pure)

```rust
pub struct SyncState {
    connection: ConnectionState,
    pending_send: Vec<WalEntry>,
    pending_recv: Vec<WalEntry>,
    last_confirmed: u64,
}

pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { server_seq: u64 },
    Syncing { progress: f32 },
}

pub enum SyncEffect {
    RequestEntries { since: u64 },
    SendEntry { entry: WalEntry },
    ApplyEntries,
    ScheduleReconnect { delay: Duration },
}
```

```
handle(event) → (SyncState, Vec<SyncEffect>):
    match event:
        Connected { server_seq } → Connected, effect RequestEntries { since: last_confirmed }
        EntriesReceived { entries } → Syncing, store pending_recv, effect ApplyEntries
        EntryConfirmed { sequence } → remove confirmed from pending_send
        LocalOperation { entry } → add to pending_send, effect SendEntry
        Disconnected → Disconnected, effect ScheduleReconnect
```

### Sync Runner (Imperative Shell)

The runner executes the sync state machine:
1. Event loop: handle WebSocket messages and local operations
2. Execute effects: send messages, apply entries via WAL merge, schedule reconnects
3. Handle merge conflicts by logging or prompting user

## Snapshots and Compaction

For large WALs, periodic snapshots compress history.

```rust
pub struct Snapshot {
    sequence: u64,
    timestamp: Instant,
    state: State,
    hash: String,
}
```

Operations:
- **snapshot** - Serialize current state to disk, record `Operation::Snapshot` in WAL
- **load_with_snapshot** - Load snapshot, then replay only WAL entries after snapshot sequence
- **compact** - Find latest snapshot marker, rewrite WAL keeping only entries after snapshot

## See Also

- [Module Structure](01-modules.md) - Storage module boundaries
- [Adapters](06-adapters.md) - Filesystem adapter
- [Testing Strategy](07-testing.md) - Testing storage layer

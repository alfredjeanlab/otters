# Epic 7: Storage & Durability

**Root Feature:** `otters-c7e7`

Replace JSON file storage with a write-ahead log (WAL) for durability and audit trail.

## 1. Overview

This epic replaces the current `JsonStore` implementation with a proper write-ahead log (WAL). The WAL becomes the source of truth for all state changes. Instead of mutating state directly, all changes are appended as operations to the log. Current state is derived by replaying entries.

Key components:

- **WAL structure**: Each entry contains sequence number, timestamp, machine ID, operation, and CRC32 checksum
- **Operation types**: Typed operations for Pipeline, Queue, Lock, Semaphore, Workspace, Session, and Event
- **WAL writer**: Durable append with fsync, atomic writes, corruption detection
- **State materialization**: Rebuild full state by replaying operations from snapshot
- **Store interface**: Unified API for execute (write + apply) and query operations
- **Snapshots**: Periodic full-state serialization for faster startup
- **Compaction**: Rewrite WAL keeping only entries after the latest snapshot
- **Recovery**: Resume from last snapshot and replay subsequent entries

The architecture follows the existing functional core pattern: operations are immutable records, state is derived, and effects are explicit.

## 2. Project Structure

```
crates/core/src/
├── storage/
│   ├── mod.rs                   # Module exports, unified Store trait
│   ├── wal/
│   │   ├── mod.rs               # WAL submodule exports
│   │   ├── entry.rs             # WalEntry, sequence, checksum
│   │   ├── entry_tests.rs       # Entry serialization tests
│   │   ├── operation.rs         # Operation enum (all operation types)
│   │   ├── operation_tests.rs   # Operation serialization tests
│   │   ├── writer.rs            # WalWriter (append, sync, rotate)
│   │   ├── writer_tests.rs      # Writer durability tests
│   │   ├── reader.rs            # WalReader (iterate, validate)
│   │   ├── reader_tests.rs      # Reader/corruption tests
│   │   └── CLAUDE.md            # WAL module documentation
│   ├── snapshot/
│   │   ├── mod.rs               # Snapshot submodule exports
│   │   ├── types.rs             # Snapshot format, state serialization
│   │   ├── types_tests.rs       # Snapshot serialization tests
│   │   ├── manager.rs           # SnapshotManager (create, load, prune)
│   │   ├── manager_tests.rs     # Snapshot lifecycle tests
│   │   └── CLAUDE.md            # Snapshot module documentation
│   ├── store.rs                 # WalStore implementation
│   ├── store_tests.rs           # Store integration tests
│   ├── recovery.rs              # Recovery logic (snapshot + replay)
│   ├── recovery_tests.rs        # Recovery scenario tests
│   ├── compaction.rs            # WAL compaction logic
│   ├── compaction_tests.rs      # Compaction tests
│   └── CLAUDE.md                # Updated storage documentation
├── engine/
│   └── runtime.rs               # Update to use WalStore
└── lib.rs                       # Update exports
```

Files to delete (after migration complete):
- `crates/core/src/storage/json.rs`
- `crates/core/src/storage/json_tests.rs`

## 3. Dependencies

Add to `crates/core/Cargo.toml`:

```toml
[dependencies]
crc32fast = "1.3"               # Fast CRC32 checksums
```

No new major dependencies required. The WAL uses:
- `serde_json` (existing) for operation serialization within entries
- `crc32fast` for checksum calculation
- Standard library `fs::File` with `sync_all()` for durability

## 4. Implementation Phases

### Phase 1: WAL Entry & Operation Types

**Goal**: Define the fundamental WAL data structures and operation types.

**Files**:
- `crates/core/src/storage/wal/mod.rs`
- `crates/core/src/storage/wal/entry.rs`
- `crates/core/src/storage/wal/entry_tests.rs`
- `crates/core/src/storage/wal/operation.rs`
- `crates/core/src/storage/wal/operation_tests.rs`
- `crates/core/src/storage/wal/CLAUDE.md`

**WAL Entry structure**:
```rust
/// A single entry in the write-ahead log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Monotonically increasing sequence number
    pub sequence: u64,
    /// Microseconds since Unix epoch
    pub timestamp_micros: u64,
    /// Unique machine identifier (for future multi-machine sync)
    pub machine_id: String,
    /// The operation being recorded
    pub operation: Operation,
    /// CRC32 checksum of serialized operation
    pub checksum: u32,
}

impl WalEntry {
    pub fn new(
        sequence: u64,
        machine_id: &str,
        operation: Operation,
        clock: &impl Clock,
    ) -> Self;

    /// Verify the checksum matches the operation
    pub fn verify(&self) -> bool;

    /// Serialize to newline-delimited JSON
    pub fn to_line(&self) -> Result<String, StorageError>;

    /// Parse from a single line
    pub fn from_line(line: &str) -> Result<Self, StorageError>;
}
```

**Operation types**:
```rust
/// All state-changing operations in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    // Pipeline operations
    PipelineCreate(PipelineCreateOp),
    PipelineTransition(PipelineTransitionOp),
    PipelineDelete(PipelineDeleteOp),

    // Task operations
    TaskCreate(TaskCreateOp),
    TaskTransition(TaskTransitionOp),
    TaskDelete(TaskDeleteOp),

    // Workspace operations
    WorkspaceCreate(WorkspaceCreateOp),
    WorkspaceTransition(WorkspaceTransitionOp),
    WorkspaceDelete(WorkspaceDeleteOp),

    // Queue operations
    QueuePush(QueuePushOp),
    QueueClaim(QueueClaimOp),
    QueueComplete(QueueCompleteOp),
    QueueFail(QueueFailOp),
    QueueDelete(QueueDeleteOp),

    // Lock operations
    LockAcquire(LockAcquireOp),
    LockRelease(LockReleaseOp),
    LockHeartbeat(LockHeartbeatOp),

    // Semaphore operations
    SemaphoreAcquire(SemaphoreAcquireOp),
    SemaphoreRelease(SemaphoreReleaseOp),
    SemaphoreHeartbeat(SemaphoreHeartbeatOp),

    // Session operations
    SessionCreate(SessionCreateOp),
    SessionTransition(SessionTransitionOp),
    SessionDelete(SessionDeleteOp),

    // Event operations (events now durable)
    EventEmit(EventEmitOp),

    // Snapshot marker (for compaction)
    SnapshotTaken { snapshot_id: String },
}

// Example operation structs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCreateOp {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub workspace_id: String,
    pub inputs: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTransitionOp {
    pub id: String,
    pub from_phase: String,
    pub to_phase: String,
    pub outputs: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuePushOp {
    pub queue_name: String,
    pub item_id: String,
    pub data: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockAcquireOp {
    pub lock_name: String,
    pub holder_id: String,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEmitOp {
    pub event_type: String,
    pub payload: serde_json::Value,
}
```

**Milestone**: All operation types serialize/deserialize correctly with checksum verification.

---

### Phase 2: WAL Writer & Reader

**Goal**: Implement durable append-only log writing and reading with corruption detection.

**Files**:
- `crates/core/src/storage/wal/writer.rs`
- `crates/core/src/storage/wal/writer_tests.rs`
- `crates/core/src/storage/wal/reader.rs`
- `crates/core/src/storage/wal/reader_tests.rs`

**WAL Writer**:
```rust
pub struct WalWriter {
    path: PathBuf,
    file: File,
    next_sequence: u64,
    machine_id: String,
    bytes_written: u64,
}

impl WalWriter {
    /// Open or create a WAL file
    pub fn open(path: &Path, machine_id: &str) -> Result<Self, StorageError>;

    /// Create a new WAL file for testing
    pub fn open_temp(machine_id: &str) -> Result<Self, StorageError>;

    /// Append an operation to the WAL
    /// Returns the assigned sequence number
    pub fn append(
        &mut self,
        operation: Operation,
        clock: &impl Clock,
    ) -> Result<u64, StorageError>;

    /// Force sync to disk (called after critical operations)
    pub fn sync(&mut self) -> Result<(), StorageError>;

    /// Get current sequence number
    pub fn sequence(&self) -> u64;

    /// Get bytes written since open
    pub fn bytes_written(&self) -> u64;
}
```

**Append implementation** (durability focused):
```rust
impl WalWriter {
    pub fn append(
        &mut self,
        operation: Operation,
        clock: &impl Clock,
    ) -> Result<u64, StorageError> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let entry = WalEntry::new(
            sequence,
            &self.machine_id,
            operation,
            clock,
        );

        let line = entry.to_line()?;
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;

        // Critical: sync to ensure durability before returning
        self.file.sync_all()?;

        self.bytes_written += line.len() as u64 + 1;
        Ok(sequence)
    }
}
```

**WAL Reader**:
```rust
pub struct WalReader {
    path: PathBuf,
}

impl WalReader {
    pub fn open(path: &Path) -> Result<Self, StorageError>;

    /// Iterate over all valid entries
    /// Stops at first corrupted entry (truncated write)
    pub fn entries(&self) -> Result<WalEntryIter, StorageError>;

    /// Read entries starting from a sequence number
    pub fn entries_from(&self, sequence: u64) -> Result<WalEntryIter, StorageError>;

    /// Get the last valid sequence number
    pub fn last_sequence(&self) -> Result<Option<u64>, StorageError>;
}

pub struct WalEntryIter {
    reader: BufReader<File>,
    line_number: u64,
}

impl Iterator for WalEntryIter {
    type Item = Result<WalEntry, WalReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Read line, parse entry, verify checksum
        // Return None at EOF, Err on corruption
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WalReadError {
    #[error("corrupted entry at line {line}: {reason}")]
    Corrupted { line: u64, reason: String },
    #[error("checksum mismatch at line {line}")]
    ChecksumMismatch { line: u64 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

**Corruption handling**:
- Truncated writes (crash during write) are detected by JSON parse failure
- Bit flips are detected by CRC32 checksum mismatch
- Recovery truncates WAL at last valid entry

**Milestone**: Writer survives simulated crashes, reader detects and handles corruption.

---

### Phase 3: State Materialization

**Goal**: Rebuild in-memory state by replaying WAL operations.

**Files**:
- `crates/core/src/storage/store.rs` (partial)
- `crates/core/src/storage/store_tests.rs` (partial)

**Materialized state**:
```rust
/// Full system state materialized from WAL
#[derive(Debug, Default, Clone)]
pub struct MaterializedState {
    pub pipelines: HashMap<PipelineId, Pipeline>,
    pub tasks: HashMap<TaskId, Task>,
    pub workspaces: HashMap<WorkspaceId, Workspace>,
    pub queues: HashMap<String, Queue>,
    pub coordination: CoordinationState,
    pub events: Vec<Event>,  // Recent events (ring buffer)
}

impl MaterializedState {
    /// Apply a single operation to the state
    pub fn apply(&mut self, op: &Operation, clock: &impl Clock) -> Result<(), ApplyError>;

    /// Get pipeline by ID
    pub fn pipeline(&self, id: &PipelineId) -> Option<&Pipeline>;

    /// Get all pipelines
    pub fn pipelines(&self) -> impl Iterator<Item = &Pipeline>;

    // Similar accessors for other entity types...
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("entity not found: {kind} {id}")]
    NotFound { kind: &'static str, id: String },
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
}
```

**Apply implementation**:
```rust
impl MaterializedState {
    pub fn apply(&mut self, op: &Operation, clock: &impl Clock) -> Result<(), ApplyError> {
        match op {
            Operation::PipelineCreate(op) => {
                let pipeline = Pipeline::new(
                    PipelineId(op.id.clone()),
                    PipelineKind::from_str(&op.kind),
                    &op.name,
                    WorkspaceId(op.workspace_id.clone()),
                    op.inputs.clone(),
                    clock,
                );
                self.pipelines.insert(pipeline.id.clone(), pipeline);
            }
            Operation::PipelineTransition(op) => {
                let pipeline = self.pipelines.get_mut(&PipelineId(op.id.clone()))
                    .ok_or_else(|| ApplyError::NotFound {
                        kind: "pipeline",
                        id: op.id.clone(),
                    })?;
                // Apply phase transition...
            }
            Operation::LockAcquire(op) => {
                self.coordination.acquire_lock(
                    &op.lock_name,
                    HolderId(op.holder_id.clone()),
                    op.metadata.clone(),
                    clock,
                );
            }
            // ... other operations
        }
        Ok(())
    }
}
```

**Milestone**: State can be rebuilt from a sequence of operations with test verification.

---

### Phase 4: Snapshots

**Goal**: Periodic full-state serialization for faster startup.

**Files**:
- `crates/core/src/storage/snapshot/mod.rs`
- `crates/core/src/storage/snapshot/types.rs`
- `crates/core/src/storage/snapshot/types_tests.rs`
- `crates/core/src/storage/snapshot/manager.rs`
- `crates/core/src/storage/snapshot/manager_tests.rs`
- `crates/core/src/storage/snapshot/CLAUDE.md`

**Snapshot format**:
```rust
/// Serializable snapshot of full system state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version (for migrations)
    pub version: u32,
    /// Sequence number of last applied WAL entry
    pub sequence: u64,
    /// Timestamp when snapshot was created
    pub timestamp_micros: u64,
    /// Machine that created the snapshot
    pub machine_id: String,
    /// Serialized state
    pub state: SnapshotState,
    /// CRC32 of serialized state
    pub checksum: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotState {
    pub pipelines: Vec<StorablePipeline>,
    pub tasks: Vec<StorableTask>,
    pub workspaces: Vec<Workspace>,
    pub queues: HashMap<String, Queue>,
    pub coordination: StorableCoordinationState,
}

impl Snapshot {
    pub fn from_state(
        state: &MaterializedState,
        sequence: u64,
        machine_id: &str,
        clock: &impl Clock,
    ) -> Self;

    pub fn to_state(&self, clock: &impl Clock) -> MaterializedState;

    pub fn verify(&self) -> bool;
}
```

**Snapshot manager**:
```rust
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new(snapshots_dir: &Path) -> Result<Self, StorageError>;

    /// Create a new snapshot
    pub fn create(
        &self,
        state: &MaterializedState,
        sequence: u64,
        machine_id: &str,
        clock: &impl Clock,
    ) -> Result<String, StorageError>;  // Returns snapshot_id

    /// Load the latest valid snapshot
    pub fn load_latest(&self, clock: &impl Clock) -> Result<Option<(Snapshot, MaterializedState)>, StorageError>;

    /// Load a specific snapshot
    pub fn load(&self, snapshot_id: &str, clock: &impl Clock) -> Result<Snapshot, StorageError>;

    /// List all snapshots, newest first
    pub fn list(&self) -> Result<Vec<SnapshotInfo>, StorageError>;

    /// Delete snapshots older than the given sequence
    pub fn prune(&self, before_sequence: u64) -> Result<u32, StorageError>;
}

#[derive(Debug)]
pub struct SnapshotInfo {
    pub id: String,
    pub sequence: u64,
    pub timestamp: SystemTime,
    pub size_bytes: u64,
}
```

**Snapshot naming**:
```
.oj/snapshots/
├── 00000042-1705123456.snapshot.json  # sequence-timestamp
├── 00000084-1705234567.snapshot.json
└── 00000126-1705345678.snapshot.json
```

**Milestone**: Snapshots create/load correctly, state matches original after round-trip.

---

### Phase 5: WalStore & Recovery

**Goal**: Unified store interface with recovery from snapshot + WAL replay.

**Files**:
- `crates/core/src/storage/store.rs` (complete)
- `crates/core/src/storage/store_tests.rs` (complete)
- `crates/core/src/storage/recovery.rs`
- `crates/core/src/storage/recovery_tests.rs`
- `crates/core/src/storage/mod.rs` (update exports)

**WalStore interface**:
```rust
pub struct WalStore {
    base_path: PathBuf,
    machine_id: String,
    writer: WalWriter,
    snapshots: SnapshotManager,
    state: MaterializedState,
    last_snapshot_sequence: u64,
    snapshot_interval: u64,  // Entries between snapshots
}

impl WalStore {
    /// Open an existing store or create new
    pub fn open(
        base_path: &Path,
        machine_id: &str,
        clock: &impl Clock,
    ) -> Result<Self, StorageError>;

    /// Create a temporary store for testing
    pub fn open_temp(clock: &impl Clock) -> Result<Self, StorageError>;

    /// Execute an operation (write to WAL + apply to state)
    pub fn execute(
        &mut self,
        operation: Operation,
        clock: &impl Clock,
    ) -> Result<u64, StorageError>;

    /// Execute multiple operations atomically
    pub fn execute_batch(
        &mut self,
        operations: Vec<Operation>,
        clock: &impl Clock,
    ) -> Result<u64, StorageError>;

    /// Query current state
    pub fn state(&self) -> &MaterializedState;

    /// Force a snapshot now
    pub fn snapshot(&mut self, clock: &impl Clock) -> Result<String, StorageError>;

    /// Compact WAL (rewrite keeping entries after snapshot)
    pub fn compact(&mut self) -> Result<CompactionResult, StorageError>;

    /// Current sequence number
    pub fn sequence(&self) -> u64;
}
```

**Recovery logic**:
```rust
pub fn recover(
    base_path: &Path,
    machine_id: &str,
    clock: &impl Clock,
) -> Result<RecoveryResult, StorageError> {
    let snapshots = SnapshotManager::new(&base_path.join("snapshots"))?;
    let wal_path = base_path.join("wal.jsonl");

    // 1. Load latest snapshot if available
    let (mut state, start_sequence) = match snapshots.load_latest(clock)? {
        Some((snapshot, state)) => {
            log::info!("Loaded snapshot at sequence {}", snapshot.sequence);
            (state, snapshot.sequence + 1)
        }
        None => {
            log::info!("No snapshot found, starting from empty state");
            (MaterializedState::default(), 0)
        }
    };

    // 2. Replay WAL entries after snapshot
    let reader = WalReader::open(&wal_path)?;
    let mut replayed = 0u64;
    let mut last_sequence = start_sequence.saturating_sub(1);

    for entry_result in reader.entries_from(start_sequence)? {
        match entry_result {
            Ok(entry) => {
                if !entry.verify() {
                    log::warn!("Checksum mismatch at sequence {}, truncating", entry.sequence);
                    break;
                }
                state.apply(&entry.operation, clock)?;
                last_sequence = entry.sequence;
                replayed += 1;
            }
            Err(WalReadError::Corrupted { line, .. }) => {
                log::warn!("Corrupted entry at line {}, truncating WAL", line);
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }

    log::info!("Replayed {} entries, at sequence {}", replayed, last_sequence);

    Ok(RecoveryResult {
        state,
        last_sequence,
        entries_replayed: replayed,
    })
}

#[derive(Debug)]
pub struct RecoveryResult {
    pub state: MaterializedState,
    pub last_sequence: u64,
    pub entries_replayed: u64,
}
```

**Auto-snapshot policy**:
```rust
impl WalStore {
    pub fn execute(
        &mut self,
        operation: Operation,
        clock: &impl Clock,
    ) -> Result<u64, StorageError> {
        let sequence = self.writer.append(operation.clone(), clock)?;
        self.state.apply(&operation, clock)?;

        // Auto-snapshot if interval exceeded
        let entries_since_snapshot = sequence - self.last_snapshot_sequence;
        if entries_since_snapshot >= self.snapshot_interval {
            self.snapshot(clock)?;
        }

        Ok(sequence)
    }
}
```

**Milestone**: Store recovers correctly from crash at any point, state matches pre-crash.

---

### Phase 6: Compaction & Migration

**Goal**: WAL compaction and migration from JsonStore to WalStore.

**Files**:
- `crates/core/src/storage/compaction.rs`
- `crates/core/src/storage/compaction_tests.rs`
- `crates/core/src/engine/runtime.rs` (update)
- Delete: `crates/core/src/storage/json.rs`, `json_tests.rs`

**Compaction**:
```rust
pub fn compact(
    wal_path: &Path,
    snapshot_sequence: u64,
) -> Result<CompactionResult, StorageError> {
    let temp_path = wal_path.with_extension("jsonl.tmp");
    let reader = WalReader::open(wal_path)?;

    // Write entries after snapshot to temp file
    let mut writer = File::create(&temp_path)?;
    let mut kept = 0u64;
    let mut discarded = 0u64;

    for entry_result in reader.entries()? {
        let entry = entry_result?;
        if entry.sequence > snapshot_sequence {
            let line = entry.to_line()?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
            kept += 1;
        } else {
            discarded += 1;
        }
    }

    writer.sync_all()?;

    // Atomic replace
    std::fs::rename(&temp_path, wal_path)?;

    Ok(CompactionResult { kept, discarded })
}

#[derive(Debug)]
pub struct CompactionResult {
    pub kept: u64,
    pub discarded: u64,
}
```

**Engine migration** (update `runtime.rs`):
```rust
// Before (remove):
pub struct Engine<A: Adapters> {
    store: JsonStore,
    // ...
}

// After:
pub struct Engine<A: Adapters> {
    store: WalStore,
    // ...
}

impl<A: Adapters> Engine<A> {
    pub fn load(&mut self, clock: &impl Clock) -> Result<(), EngineError> {
        // State is already loaded during WalStore::open()
        // Just sync our caches from store.state()
        let state = self.store.state();

        self.pipelines = state.pipelines.clone();
        self.tasks = state.tasks.clone();
        self.workspaces = state.workspaces.clone();
        // ...

        Ok(())
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline, clock: &impl Clock) -> Result<(), EngineError> {
        let op = Operation::PipelineCreate(PipelineCreateOp {
            id: pipeline.id.0.clone(),
            kind: pipeline.kind.to_string(),
            name: pipeline.name.clone(),
            workspace_id: pipeline.workspace_id.0.clone(),
            inputs: pipeline.inputs.clone(),
        });

        self.store.execute(op, clock)?;
        self.pipelines.insert(pipeline.id.clone(), pipeline);

        Ok(())
    }

    // Similar updates for other add/update methods...
}
```

**File layout migration**:
```
# Old (JsonStore)
.oj/
├── pipelines/
│   └── {id}.json
├── tasks/
│   └── {id}.json
├── workspaces/
│   └── {id}.json
└── queues/
    └── {name}.json

# New (WalStore)
.oj/
├── wal.jsonl           # Write-ahead log
└── snapshots/
    └── {seq}-{ts}.snapshot.json
```

**Milestone**: All existing tests pass with WalStore, JsonStore deleted.

## 5. Key Implementation Details

### Checksum Calculation

CRC32 is calculated over the JSON-serialized operation:
```rust
impl WalEntry {
    fn calculate_checksum(operation: &Operation) -> u32 {
        let json = serde_json::to_string(operation).unwrap();
        crc32fast::hash(json.as_bytes())
    }

    pub fn new(sequence: u64, machine_id: &str, operation: Operation, clock: &impl Clock) -> Self {
        let checksum = Self::calculate_checksum(&operation);
        Self {
            sequence,
            timestamp_micros: clock.now_system().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64,
            machine_id: machine_id.to_string(),
            operation,
            checksum,
        }
    }

    pub fn verify(&self) -> bool {
        self.checksum == Self::calculate_checksum(&self.operation)
    }
}
```

### Handling Instant Fields

Types with `Instant` fields (Task, Lock, Semaphore) use age-based serialization:
```rust
// On write: convert Instant to age in microseconds
pub fn to_storable(task: &Task, clock: &impl Clock) -> StorableTask {
    let age_micros = clock.now().duration_since(task.started_at).as_micros() as u64;
    StorableTask {
        // ... other fields
        age_micros,
    }
}

// On read: reconstruct Instant from age
pub fn from_storable(storable: &StorableTask, clock: &impl Clock) -> Task {
    let started_at = clock.now() - Duration::from_micros(storable.age_micros);
    Task {
        // ... other fields
        started_at,
    }
}
```

This pattern is already established in `StorableTask` and `StorableLock` in the existing codebase.

### Atomic Snapshot + WAL Coordination

To ensure consistency between snapshots and WAL:
```rust
impl WalStore {
    pub fn snapshot(&mut self, clock: &impl Clock) -> Result<String, StorageError> {
        let sequence = self.writer.sequence();

        // 1. Create snapshot of current state
        let snapshot_id = self.snapshots.create(
            &self.state,
            sequence,
            &self.machine_id,
            clock,
        )?;

        // 2. Write marker to WAL
        self.writer.append(
            Operation::SnapshotTaken { snapshot_id: snapshot_id.clone() },
            clock,
        )?;

        self.last_snapshot_sequence = sequence;

        Ok(snapshot_id)
    }
}
```

### Event Durability

Events are now durable via WAL:
```rust
// In EventBus::emit()
pub fn emit(&mut self, event: Event, store: &mut WalStore, clock: &impl Clock) -> Result<(), EventError> {
    // Write to WAL first
    store.execute(
        Operation::EventEmit(EventEmitOp {
            event_type: event.event_type.clone(),
            payload: event.payload.clone(),
        }),
        clock,
    )?;

    // Then dispatch to subscribers
    self.dispatch(&event);

    Ok(())
}
```

### Crash Safety Invariants

The WAL maintains these invariants:
1. **Write ordering**: Operations are written to WAL before state is updated
2. **Sync before ack**: `sync_all()` completes before returning success
3. **Monotonic sequences**: Sequence numbers never decrease or repeat
4. **Checksum integrity**: Every entry is verified on read
5. **Recovery idempotence**: Replaying the same entries produces identical state

## 6. Verification Plan

### Unit Tests

**Entry tests** (`entry_tests.rs`):
- Serialize/deserialize round-trip
- Checksum calculation and verification
- Detect tampered entries

**Operation tests** (`operation_tests.rs`):
- All operation types serialize correctly
- Backward compatibility with `#[serde(default)]`
- JSON format is stable

**Writer tests** (`writer_tests.rs`):
- Append increments sequence
- Sync flushes to disk (verified via re-read)
- Handles concurrent access gracefully

**Reader tests** (`reader_tests.rs`):
- Read all valid entries
- Stop at corrupted entry
- Handle empty WAL
- Handle truncated final entry

**Snapshot tests** (`types_tests.rs`, `manager_tests.rs`):
- State round-trip through snapshot
- Checksum verification
- List/prune operations
- Handle corrupted snapshots

### Integration Tests

**Recovery scenarios** (`recovery_tests.rs`):
```rust
#[test]
fn recovery_from_empty() {
    // Fresh start with no WAL or snapshot
}

#[test]
fn recovery_from_wal_only() {
    // WAL exists, no snapshot
}

#[test]
fn recovery_from_snapshot_only() {
    // Snapshot exists, empty WAL
}

#[test]
fn recovery_from_snapshot_plus_wal() {
    // Normal case: snapshot + subsequent entries
}

#[test]
fn recovery_truncates_corrupted_tail() {
    // WAL has corruption at end, recovery truncates
}

#[test]
fn recovery_skips_corrupted_snapshot() {
    // Latest snapshot corrupted, falls back to older
}
```

**Store integration** (`store_tests.rs`):
```rust
#[test]
fn execute_persists_and_applies() {
    let mut store = WalStore::open_temp(&SystemClock).unwrap();

    store.execute(Operation::PipelineCreate(...), &clock).unwrap();

    // Verify in-memory state
    assert!(store.state().pipeline(&id).is_some());

    // Verify persistence by reopening
    drop(store);
    let store = WalStore::open(path, &clock).unwrap();
    assert!(store.state().pipeline(&id).is_some());
}

#[test]
fn auto_snapshot_triggers() {
    let mut store = WalStore::with_snapshot_interval(10, &clock);

    for _ in 0..15 {
        store.execute(...);
    }

    // Snapshot should have been created
    assert!(store.snapshots.list().unwrap().len() >= 1);
}
```

**Compaction tests** (`compaction_tests.rs`):
```rust
#[test]
fn compaction_removes_pre_snapshot_entries() {
    // Create 100 entries, snapshot at 50, compact
    // WAL should only have entries 51-100
}

#[test]
fn compaction_preserves_state() {
    // State after compaction equals state before
}
```

### Property Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn operation_roundtrip(op in arb_operation()) {
        let json = serde_json::to_string(&op).unwrap();
        let parsed: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, parsed);
    }

    #[test]
    fn apply_sequence_is_deterministic(ops in vec(arb_operation(), 1..100)) {
        let mut state1 = MaterializedState::default();
        let mut state2 = MaterializedState::default();

        for op in &ops {
            let _ = state1.apply(op, &FakeClock::new());
        }
        for op in &ops {
            let _ = state2.apply(op, &FakeClock::new());
        }

        assert_eq!(state1, state2);
    }
}
```

### Regression Tests

Ensure all existing tests pass:
- All `engine_tests.rs` tests
- All `pipeline_tests.rs` tests
- All `task_tests.rs` tests
- All `coordination/*_tests.rs` tests
- All CLI integration tests

### Pre-commit Verification

Before each phase commit:
```bash
./checks/lint.sh
make check   # fmt, clippy, test, build, audit, deny
```

### Performance Verification

After migration complete:
```bash
# Measure startup time with varying WAL sizes
./scripts/benchmark.sh storage_startup

# Compare with JsonStore baseline
./scripts/compare.sh baseline.json current.json
```

Target: Startup with 10K entries + snapshot should be <100ms.

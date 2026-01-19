# Epic 9e: WAL Compaction & Recovery Fixes

**Root Feature:** `otters-9bbb`

**Depends on**: None (standalone)
**Blocks**: None

## Problem Statement

Two WAL durability issues from Epic 7:

1. **WAL compaction is a stub** - The `compact()` method only cleans snapshots but doesn't truncate the WAL file itself. Disk space is never reclaimed.

2. **Recovery doesn't truncate corrupted WAL** - When recovery encounters corruption, it silently stops replaying but leaves the corrupt tail in the file. Future writes append after corruption, potentially causing issues.

## Goal

Complete WAL durability features so disk space is reclaimed and corrupted WAL files are properly truncated.

## Implementation

### 1. Implement Actual Compaction in `crates/core/src/storage/store.rs`

```rust
impl WalStore {
    /// Compact the WAL by removing entries before the last snapshot.
    /// Rewrites WAL to temp file with entries after snapshot, then atomic rename.
    pub fn compact(&mut self) -> Result<CompactionResult, WalStoreError> {
        // 1. Return early if no snapshot
        // 2. Collect entries after snapshot_seq
        // 3. Write to temp file, sync
        // 4. Atomic rename to replace original
        // 5. Reopen reader/writer, update first_sequence
        // 6. Cleanup old snapshots
        // 7. Return CompactionResult with stats
    }

    pub fn should_compact(&self) -> bool {
        // True if entries before snapshot > compaction_threshold
    }

    pub fn maybe_compact(&mut self) -> Result<Option<CompactionResult>, WalStoreError> {
        // Compact if should_compact() returns true
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Number of entries removed
    pub entries_removed: usize,
    /// Number of entries kept
    pub entries_kept: usize,
    /// Bytes reclaimed from disk
    pub bytes_reclaimed: u64,
}
```

### 2. Add Config Options

```rust
/// WAL store configuration
#[derive(Debug, Clone)]
pub struct WalStoreConfig {
    /// Number of entries before snapshot to trigger compaction
    pub compaction_threshold: u64,
    /// Number of old snapshots to keep
    pub keep_old_snapshots: usize,
    /// Sync mode (every write, periodic, or manual)
    pub sync_mode: SyncMode,
}

impl Default for WalStoreConfig {
    fn default() -> Self {
        Self {
            compaction_threshold: 10_000,
            keep_old_snapshots: 2,
            sync_mode: SyncMode::EveryWrite,
        }
    }
}
```

### 3. Implement WAL Truncation on Recovery

```rust
impl WalStore {
    /// Recover state from WAL, handling corruption gracefully
    fn recover_from_wal(&mut self, start_seq: u64) -> Result<RecoveryResult, WalStoreError> {
        // Iterate entries, apply to state, track last_valid_position
        // On Checksum/Truncated/InvalidMagic errors: set corrupted=true, break
        // If corrupted: truncate_wal(last_valid_position)
        // Return RecoveryResult with stats
    }

    /// Truncate WAL file at the given position
    fn truncate_wal(&mut self, position: u64) -> Result<(), WalStoreError> {
        // Close writer, set_len(position), sync, reopen writer
    }
}

/// Result of WAL recovery
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Number of entries successfully recovered
    pub entries_recovered: u64,
    /// Whether corruption was detected
    pub corrupted: bool,
    /// Sequence number where corruption was detected
    pub corruption_sequence: Option<u64>,
    /// Position where WAL was truncated (if truncated)
    pub truncated_at: Option<u64>,
}
```

### 4. Add Position Tracking to WalReader

```rust
// In crates/core/src/storage/wal/reader.rs
impl WalReader {
    pub fn position(&self) -> u64 { /* file.stream_position() */ }
    pub fn seek(&mut self, position: u64) -> Result<(), WalReaderError> { /* file.seek() */ }
}
```

### 5. Add Error Variants

```rust
// In crates/core/src/storage/wal/reader.rs
#[derive(Debug, thiserror::Error)]
pub enum WalReaderError {
    // ... existing ...

    #[error("checksum mismatch at sequence {sequence}: expected {expected}, got {actual}")]
    Checksum {
        sequence: u64,
        expected: u32,
        actual: u32,
    },

    #[error("entry truncated at position {position}")]
    Truncated { position: u64 },

    #[error("invalid magic number at position {position}")]
    InvalidMagic { position: u64 },
}
```

## Files

- `crates/core/src/storage/store.rs` - Compaction and recovery
- `crates/core/src/storage/wal/reader.rs` - Position tracking and error variants
- `crates/core/src/storage/store_tests.rs` - Tests

## Tests

```rust
#[test]
fn compaction_removes_old_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let mut store = WalStore::open(&path).unwrap();

    // Write 100 entries
    for i in 0..100 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }

    // Take snapshot at seq 50
    store.snapshot().unwrap();

    // Compact
    let result = store.compact().unwrap();

    // Should have removed entries 0-49
    assert_eq!(result.entries_removed, 50);
    assert_eq!(result.entries_kept, 50);
    assert!(result.bytes_reclaimed > 0);

    // Verify remaining entries
    let entries: Vec<_> = store.reader.iter_from(0).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 50);
    assert_eq!(entries[0].sequence, 50);
}

#[test]
fn compaction_is_atomic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    let mut store = WalStore::open(&path).unwrap();

    // Write entries
    for i in 0..50 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }
    store.snapshot().unwrap();
    for i in 50..100 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }

    // Compact
    store.compact().unwrap();

    // Reopen and verify integrity
    let store2 = WalStore::open(&path).unwrap();
    let entries: Vec<_> = store2.reader.iter_from(0).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 50);
}

#[test]
fn recovery_truncates_corrupted_wal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write valid entries
    {
        let mut store = WalStore::open(&path).unwrap();
        for i in 0..10 {
            store.append(Operation::Noop { seq: i }).unwrap();
        }
    }

    // Corrupt the file by appending garbage
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"GARBAGE_DATA_HERE").unwrap();
    }

    // Recovery should truncate
    let store = WalStore::open(&path).unwrap();
    assert_eq!(store.next_sequence, 10); // All valid entries recovered

    // File should be truncated (garbage removed)
    let entries: Vec<_> = store.reader.iter_from(0).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 10);
}

#[test]
fn recovery_handles_partial_write() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write valid entries
    {
        let mut store = WalStore::open(&path).unwrap();
        for i in 0..5 {
            store.append(Operation::Noop { seq: i }).unwrap();
        }
    }

    // Simulate partial write by truncating mid-entry
    {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap();
        let size = file.metadata().unwrap().len();
        file.set_len(size - 5).unwrap(); // Remove last 5 bytes
    }

    // Recovery should handle gracefully
    let store = WalStore::open(&path).unwrap();
    // Should recover 4 entries (last one was truncated)
    assert!(store.next_sequence >= 4);
}

#[test]
fn should_compact_respects_threshold() {
    let mut store = WalStore::new_in_memory();
    store.config.compaction_threshold = 100;

    // No snapshot, no compaction
    assert!(!store.should_compact());

    // Write entries and snapshot
    for i in 0..50 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }
    store.snapshot().unwrap();

    // Below threshold
    assert!(!store.should_compact());

    // Write more entries
    for i in 50..200 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }
    store.snapshot().unwrap();

    // Now above threshold
    assert!(store.should_compact());
}

#[test]
fn writes_work_after_truncation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");

    // Write, corrupt, recover
    {
        let mut store = WalStore::open(&path).unwrap();
        for i in 0..5 {
            store.append(Operation::Noop { seq: i }).unwrap();
        }
    }
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"CORRUPT").unwrap();
    }

    // Recover and continue writing
    let mut store = WalStore::open(&path).unwrap();
    for i in 5..10 {
        store.append(Operation::Noop { seq: i }).unwrap();
    }

    // Verify all entries
    let entries: Vec<_> = store.reader.iter_from(0).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries.len(), 10);
}
```

## Landing Checklist

- [ ] `compact()` removes entries before last snapshot
- [ ] Compaction is atomic (uses temp file + rename)
- [ ] Recovery truncates WAL at corruption point
- [ ] Recovery handles partial writes gracefully
- [ ] Writes work correctly after truncation
- [ ] `should_compact()` respects threshold
- [ ] Old snapshots are cleaned up
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

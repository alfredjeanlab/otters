# Plan: Complete WAL Migration - Remove QueueSet Legacy Pattern

## Overview

Remove the legacy `save_queue()`/`QueueSet` pattern and migrate all queue operations to use granular WAL operations. Also clean up JsonStore documentation references.

## Files to Modify

| File | Changes |
|------|---------|
| `crates/core/src/storage/wal/store.rs` | Add `queue_claim`, `queue_complete`, `queue_fail`, `queue_release` methods; remove `save_queue` |
| `crates/core/src/storage/wal/operation.rs` | Remove `QueueSet` variant and `QueueSetOp` struct |
| `crates/core/src/storage/wal/state.rs` | Remove `QueueSet` handler |
| `crates/cli/src/commands/queue.rs` | Refactor to claim-based model |
| `crates/core/src/engine/worker.rs` | Refactor `MergeWorker` to claim-based; update `EventDrivenWorker` |
| `crates/core/src/engine/signals.rs` | Add `QueueTick` operation for visibility timeout handling |
| `crates/core/src/engine/worker_tests.rs` | Use `queue_push` instead of `save_queue` |
| `crates/core/src/storage/CLAUDE.md` | Remove JsonStore documentation |

## Implementation Steps

### Phase 1: Add Granular Queue Methods to WalStore

**File: `crates/core/src/storage/wal/store.rs`**

Add these methods (wrappers around existing operations):

```rust
pub fn queue_claim(&mut self, queue_name: &str, item_id: &str, claim_id: &str, visibility_timeout_secs: u64) -> Result<(), WalStoreError>

pub fn queue_complete(&mut self, queue_name: &str, claim_id: &str) -> Result<(), WalStoreError>

pub fn queue_fail(&mut self, queue_name: &str, claim_id: &str, reason: &str) -> Result<(), WalStoreError>

pub fn queue_release(&mut self, queue_name: &str, claim_id: &str) -> Result<(), WalStoreError>
```

### Phase 2: Add QueueTick Operation

**Files: `operation.rs`, `state.rs`, `store.rs`**

Add `QueueTick` operation for visibility timeout tick handling:
- `QueueTickOp { queue_name: String, tick_result_json: String }`
- Handles bulk state changes from tick (expired items, dead-lettered items)
- Add `queue_tick()` method to WalStore

### Phase 3: Refactor CLI Commands

**File: `crates/cli/src/commands/queue.rs`**

| Function | Current | New |
|----------|---------|-----|
| `add_to_queue` | `save_queue` | `queue_push` |
| `take_from_queue` | `queue.take()` + `save_queue` | `queue_claim` with generated claim_id |
| `complete_item` | `queue.complete(id)` + `save_queue` | `queue_complete(claim_id)` |

**CLI change:** `complete` subcommand argument changes from `--id` to `--claim-id`.

### Phase 4: Refactor MergeWorker

**File: `crates/core/src/engine/worker.rs`**

Refactor `MergeWorker::run_once` from legacy pattern to claim-based:

```rust
// Before (legacy):
let (queue, item) = queue.take();
self.store.save_queue(&self.queue_name, &queue)?;
// ... process ...
let queue = queue.complete(&item.id);
self.store.save_queue(&self.queue_name, &queue)?;

// After (claim-based):
let claim_id = generate_claim_id();
self.store.queue_claim(&self.queue_name, &item_id, &claim_id, timeout)?;
// ... process ...
self.store.queue_complete(&self.queue_name, &claim_id)?;
```

### Phase 5: Update EventDrivenWorker

**File: `crates/core/src/engine/worker.rs`**

Already uses claim-based pattern via `transition()`. Replace `save_queue` calls:
- Line 290: `save_queue` → `queue_claim`
- Line 298: `save_queue` → `queue_complete`
- Line 308: `save_queue` → `queue_fail`

### Phase 6: Refactor Signals

**File: `crates/core/src/engine/signals.rs`**

Replace `tick_queue` implementation:
- Line 154: `save_queue` → `queue_tick`

### Phase 7: Update Tests

**File: `crates/core/src/engine/worker_tests.rs`**

- Line 11: `save_queue` for empty queue → use `queue_push` (auto-creates queue)
- Line 29: `save_queue` with item → `queue_push`

### Phase 8: Remove Legacy Code

1. **`operation.rs`**: Remove `QueueSet(QueueSetOp)` variant (lines 38-39) and `QueueSetOp` struct (lines 200-206)
2. **`state.rs`**: Remove `Operation::QueueSet` match arm
3. **`store.rs`**: Remove `save_queue()` method (lines 382-391)

### Phase 9: Documentation Cleanup

**File: `crates/core/src/storage/CLAUDE.md`**

- Remove lines 3-5 mentioning "two backends"
- Remove lines 48-67 (entire JsonStore section)
- Update backward compatibility section to focus on WAL operations only

## Verification

```bash
# Run full test suite
make check

# Verify no remaining save_queue references
grep -r "save_queue" crates/

# Verify no QueueSet references in code
grep -r "QueueSet" crates/core/src/

# Verify documentation cleanup
grep -r "JsonStore" --include="*.md" --exclude-dir=plans .
```

### Phase 10: Update Architecture Documentation

**File: `docs/04-architecture/05-storage.md`**

Update to reflect actual operation names:
- `QueueTake` → `QueueClaim` (align docs with implementation)
- Remove any JsonStore references if present

**File: `crates/core/src/storage/wal/CLAUDE.md`**

Update operation table to add `QueueTick` and remove `QueueSet`.

## Compatibility Notes

- **EPICS.md (Epic 7)**: Plan aligns with documented granular queue operations. `QueueSet` was never in the epic spec - it was added as a shortcut.
- **docs/04-architecture/05-storage.md**: Uses `QueueTake` name, but code uses `QueueClaim`. Will reconcile naming.
- Historical plans in `plans/` directory remain unchanged (they document migration history).
- The `Queue` struct may still have legacy methods (`take`, `complete`, etc.) - these can be removed in a follow-up if unused.
- `QueuePush` auto-creates queues, so no `QueueCreate` operation needed.

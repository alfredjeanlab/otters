# Epic 5c Part 2: Code Quality Completion

**Root Feature:** `otters-1807`
**Prerequisite:** Epic 5c Part 1 (commit `8770e8f`)

## Overview

This plan closes the remaining gaps from Epic 5c. Part 1 established measurement infrastructure and achieved a clean baseline. Part 2 completes the dead code audit, **extracts inline tests to sibling `_tests.rs` files**, and verifies test parametrization.

### Completed in Part 1

- [x] Quality measurement scripts (`evaluate.sh`, `benchmark.sh`, `compare.sh`)
- [x] CI jobs for quality metrics and benchmarks
- [x] Baseline committed (`reports/quality/baseline.json`)
- [x] Makefile targets for quality commands
- [x] Global `dead_code = "allow"` removed from Cargo.toml files
- [x] Targeted `#[allow(dead_code)]` with justifying comments (8 annotations)
- [x] Split `runtime.rs` → `runtime.rs` + `signals.rs`

### Remaining Work

| Phase | Status | Gap |
|-------|--------|-----|
| Phase 1: Dead Code | 90% | cargo machete not run |
| Phase 2: Test Extraction | 0% | **37 files with inline tests → `_tests.rs` siblings** |
| Phase 3: Simplify evaluate.sh | 0% | Remove `#[cfg(test)]` line-counting hack |
| Phase 4: Test Parametrization | 90% | Review guard.rs for opportunities |
| Phase 5: Code Duplication | 90% | Verify no significant duplication remains |

### Why Extract Tests?

The Part 1 `evaluate.sh` uses a fragile hack to count source-only lines:
```bash
# Finds #[cfg(test)] and counts lines before it
test_line=$(grep -n "#\[cfg(test)\]" "$file" | head -1 | cut -d: -f1)
```

**Problems with inline tests:**
1. Fragile parsing - breaks if `#[cfg(test)]` appears in comments or strings
2. Misleading file sizes - `queue.rs` shows 1262 lines but only 495 are source
3. Editor confusion - files appear large, harder to navigate
4. Inconsistent metrics - source LOC depends on test module placement

**Benefits of `_tests.rs` pattern:**
1. Clean separation - source files contain only source code
2. Accurate metrics - no special parsing needed
3. Better tooling - editors, blame, coverage all work naturally
4. Consistent pattern - matches `tests/` directory convention
5. LLM-friendly - smaller files fit better in context windows

## Implementation Phases

### Phase 1: Cargo Machete Audit

**Goal**: Run cargo machete to identify unused dependencies.

**Prerequisite**:
```bash
cargo install cargo-machete
```

**Process**:
1. Run `cargo machete` at workspace root
2. For each unused dependency reported:
   - Verify it's truly unused (machete can have false positives for procedural macros)
   - Remove from Cargo.toml if confirmed unused
   - Or document why it's needed if machete is wrong

**Verification**:
- [ ] `cargo machete` runs without errors
- [ ] Any reported unused dependencies addressed or documented
- [ ] `cargo build --all` still succeeds

---

### Phase 2: Extract Inline Tests to `_tests.rs` Siblings

**Goal**: Move all inline `#[cfg(test)] mod tests` to sibling `_tests.rs` files.

**Files to Extract** (sorted by test line count):

| File | Test Lines | Priority |
|------|------------|----------|
| `queue.rs` | 767 | High |
| `task.rs` | 644 | High |
| `pipeline.rs` | 418 | High |
| `coordination/guard.rs` | 294 | High |
| `coordination/semaphore.rs` | 283 | Medium |
| `coordination/lock.rs` | 228 | Medium |
| `coordination/manager.rs` | 222 | Medium |
| `engine/recovery.rs` | 163 | Medium |
| `engine/scheduler.rs` | 149 | Medium |
| `coordination/maintenance.rs` | 120 | Low |
| `events/log.rs` | 104 | Low |
| `coordination/storage.rs` | 97 | Low |
| `events/bus.rs` | 95 | Low |
| `storage/json.rs` | 93 | Low |
| `coordination/phase_guard.rs` | 89 | Low |
| `session.rs` | 88 | Low |
| `engine/runtime.rs` | 81 | Low |
| `config/notify.rs` | 80 | Low |
| `adapters/fake.rs` | 71 | Low |
| `workspace.rs` | 64 | Low |
| `events/subscription.rs` | 56 | Low |
| `adapters/notify.rs` | 56 | Low |
| `pipelines/bugfix.rs` | 47 | Low |
| `engine/worker.rs` | 41 | Low |
| `pipelines/build.rs` | 37 | Low |
| `clock.rs` | 32 | Low |
| `id.rs` | 30 | Low |
| `adapters/real.rs` | 28 | Low |
| `adapters/tmux.rs` | 11 | Low |
| `adapters/git.rs` | 10 | Low |

**Pattern**:

```rust
// BEFORE: queue.rs
pub struct Queue { ... }
impl Queue { ... }

#[cfg(test)]
mod tests {
    use super::*;
    // 767 lines of tests
}

// AFTER: queue.rs
pub struct Queue { ... }
impl Queue { ... }

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;

// AFTER: queue_tests.rs (new file)
use super::*;
// 767 lines of tests (moved here)
```

**Directory Structure After Extraction**:

```
crates/core/src/
├── queue.rs
├── queue_tests.rs          # NEW
├── task.rs
├── task_tests.rs           # NEW
├── pipeline.rs
├── pipeline_tests.rs       # NEW
├── session.rs
├── session_tests.rs        # NEW
├── workspace.rs
├── workspace_tests.rs      # NEW
├── coordination/
│   ├── guard.rs
│   ├── guard_tests.rs      # NEW
│   ├── semaphore.rs
│   ├── semaphore_tests.rs  # NEW
│   ├── lock.rs
│   ├── lock_tests.rs       # NEW
│   ├── manager.rs
│   ├── manager_tests.rs    # NEW
│   └── ...
├── engine/
│   ├── runtime.rs
│   ├── runtime_tests.rs    # NEW
│   ├── scheduler.rs
│   ├── scheduler_tests.rs  # NEW
│   └── ...
└── ...
```

**Extraction Process** (per file):

1. Create new `foo_tests.rs` sibling file
2. Move everything inside `mod tests { ... }` to new file
3. Add `use super::*;` at top of new file (and any other needed imports)
4. Replace inline test module with:
   ```rust
   #[cfg(test)]
   #[path = "foo_tests.rs"]
   mod tests;
   ```
5. Run `cargo test -p oj-core` to verify tests still pass
6. Commit after each file or logical group

**Verification**:
- [ ] All 30+ files with inline tests extracted
- [ ] `cargo test --all` passes
- [ ] No `#[cfg(test)] mod tests { ... }` blocks remain in source files
- [ ] All `_tests.rs` files compile and run

---

### Phase 3: Simplify evaluate.sh

**Goal**: Remove the fragile `#[cfg(test)]` line-counting hack now that tests are in separate files.

**Before** (fragile):
```bash
count_source_lines() {
    local file=$1
    local test_line=$(grep -n "#\[cfg(test)\]" "$file" 2>/dev/null | head -1 | cut -d: -f1)
    if [ -n "$test_line" ]; then
        echo $((test_line - 1))
    else
        wc -l < "$file" | tr -d ' '
    fi
}
```

**After** (simple):
```bash
# Source files: count all lines in src/*.rs excluding *_tests.rs
# Test files: count all lines in *_tests.rs and tests/*.rs
get_file_stats() {
    local crate=$1
    local type=$2  # "src" or "tests"

    if [ "$type" = "src" ]; then
        # Exclude _tests.rs files from source count
        local files=$(find "crates/$crate/src" -name "*.rs" ! -name "*_tests.rs" 2>/dev/null)
        local limit=700
    else
        # Include _tests.rs and tests/ directory
        local files=$(find "crates/$crate/src" -name "*_tests.rs" 2>/dev/null)
        files="$files $(find "crates/$crate/tests" -name "*.rs" 2>/dev/null)"
        local limit=1100
    fi
    # ... rest of function
}
```

**Changes to evaluate.sh**:
1. Remove `count_source_lines()` function entirely
2. Update `get_file_stats()` to exclude `*_tests.rs` from source counts
3. Update `get_file_stats()` to include `*_tests.rs` in test counts
4. Update `get_loc_by_crate()` similarly

**Verification**:
- [ ] `evaluate.sh` no longer greps for `#[cfg(test)]`
- [ ] Source LOC counts exclude `*_tests.rs` files
- [ ] Test LOC counts include `*_tests.rs` files
- [ ] `make quality` produces valid JSON
- [ ] Metrics are accurate (spot-check a few files manually)

---

### Phase 4: Update Baseline

**Goal**: Regenerate baseline with accurate metrics after test extraction.

**Process**:
```bash
make quality-baseline
git add reports/quality/baseline.json
git commit -m "Update baseline after test extraction"
```

**Expected Changes**:
- Source file counts will increase (each `_tests.rs` is a new file)
- Source max LOC will decrease significantly (no more 1262-line queue.rs)
- Test file counts will increase
- Total LOC unchanged (just reorganized)

**Verification**:
- [ ] New baseline committed
- [ ] `make quality-compare` shows no regressions
- [ ] `over_limit` counts remain 0

---

### Phase 5: Guard Test Parametrization Review

**Goal**: Evaluate whether guard.rs tests would benefit from yare parametrization.

**Current State** (25 tests, 0 parametrized):
- Tests follow clear pass/fail/needs_input patterns
- Each test is self-documenting with descriptive names

**Verdict**: **Skip parametrization** for guard.rs. The tests are well-organized with comments grouping related tests. Parametrization would reduce readability without significant benefit.

**Verification**:
- [ ] Review completed
- [ ] Existing yare usage in queue.rs, task.rs, pipeline.rs confirmed adequate

---

### Phase 6: Code Duplication Verification

**Goal**: Verify no significant code duplication exists.

**Already Extracted**:
- `crates/cli/tests/common/mod.rs` - `setup_test_env()`, `unique_id()`
- `crates/cli/tests/common/tmux.rs` - Tmux test helpers
- `crates/cli/tests/common/claudeless.rs` - Claudeless mode helpers

**Verification**:
- [ ] No identical multi-line code blocks across files
- [ ] Test helpers appropriately scoped

---

## Execution Checklist

### Prerequisites
```bash
cargo install cargo-machete
```

### Steps

1. **Run cargo machete**:
   ```bash
   cargo machete
   ```
   Address any findings.

2. **Extract tests** (main work):
   - Start with high-priority files (queue, task, pipeline, guard)
   - Use the pattern: move tests → add `#[path]` directive → verify
   - Commit in logical batches

3. **Simplify evaluate.sh**:
   - Remove `count_source_lines()` function
   - Update file filtering to exclude `*_tests.rs` from source

4. **Update baseline**:
   ```bash
   make quality-baseline
   ```

5. **Run full verification**:
   ```bash
   make check
   make quality
   make quality-compare
   ```

---

## Verification Summary

### Phase 1 (Cargo Machete)
- [ ] `cargo machete` runs successfully
- [ ] No unused dependencies (or documented exceptions)

### Phase 2 (Test Extraction)
- [ ] All inline tests moved to `_tests.rs` siblings
- [ ] No `mod tests { ... }` blocks remain in source files
- [ ] `cargo test --all` passes

### Phase 3 (Simplify evaluate.sh)
- [ ] No `#[cfg(test)]` parsing in evaluate.sh
- [ ] `*_tests.rs` excluded from source LOC
- [ ] `*_tests.rs` included in test LOC

### Phase 4 (Baseline Update)
- [ ] New baseline committed
- [ ] `over_limit` counts remain 0
- [ ] Source max LOC reflects actual source (not source+tests)

### Phase 5 (Test Parametrization)
- [ ] guard.rs review completed
- [ ] Existing yare usage confirmed adequate

### Phase 6 (Code Duplication)
- [ ] No significant cross-file duplication found

### Final
- [ ] `make check` passes
- [ ] `make quality` produces valid metrics
- [ ] No regressions from baseline

---

## Notes

This part-2 work is **primarily test extraction** - moving ~4,000 lines of inline tests to sibling files. This is mechanical but important for:

1. **Accurate metrics** - Source LOC will reflect actual source code
2. **LLM context** - Smaller files fit better in context windows
3. **Code navigation** - Editors show true file sizes
4. **Maintainability** - Clear separation of concerns

| Metric | Part 1 | After Part 2 |
|--------|--------|--------------|
| Source max LOC | 623 (with hack) | ~500 (actual) |
| Test files | 1 (core), 10 (cli) | ~40 (core), 10 (cli) |
| evaluate.sh complexity | Fragile | Simple |
| Inline test modules | 30+ | 0 |

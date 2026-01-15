# Epic 5c: Code Quality & Metrics

**Root Feature:** `oj-0846`

## Overview

Establish code quality baselines, identify technical debt, and create repeatable measurement infrastructure. This epic focuses on making the codebase maintainable before adding more features.

> **Note:** The `claude-sim` crate will be moved to its own repository in separate work and is excluded from this plan.

The codebase currently has:
- **~17.4K LOC** across 2 crates (oj-core: 13.3K, oj CLI: 4.1K)
- **Strong linting**: `forbid(unsafe_code)`, `deny(unwrap/expect/panic)` in production code
- **6 source files exceeding 500 LOC** in oj-core (target: ≤700 LOC)
- **No quality measurement infrastructure** (no baseline, no benchmarks, no comparison scripts)
- **Underutilized parametrized testing** (yare available but not widely used)
- **Dead code allowed** via `#[allow(dead_code)]` with no audit trail

**Key Deliverables:**
1. Dead code audit with justified `#[allow(dead_code)]` annotations
2. Code duplication analysis and extraction of shared utilities
3. Test parametrization conversion using `yare`
4. File size enforcement with splits where needed
5. Quality measurement scripts producing JSON metrics
6. Benchmark scripts for compile/test/runtime performance
7. CI job uploading quality/benchmark artifacts
8. Baseline report for regression detection

## Project Structure

```
otters/
├── checks/
│   └── quality/
│       ├── evaluate.sh             # NEW: Quality metrics script
│       ├── benchmark.sh            # NEW: Performance benchmarks
│       └── compare.sh              # NEW: Baseline comparison
│
├── reports/
│   └── quality/
│       └── baseline.json           # NEW: Current state baseline
│
├── crates/
│   ├── core/
│   │   ├── Cargo.toml              # UPDATE: Remove dead_code allow where safe
│   │   └── src/
│   │       ├── queue.rs            # SPLIT: 1,269 LOC → queue.rs + queue_operations.rs
│   │       ├── queue_operations.rs # NEW: Queue operations extracted
│   │       ├── task.rs             # REVIEW: 920 LOC (may need split)
│   │       ├── pipeline.rs         # REVIEW: 898 LOC (may need split)
│   │       └── engine/
│   │           └── runtime.rs      # REVIEW: 907 LOC (may need split)
│   │
│   └── cli/
│       └── tests/
│           └── common/
│               └── mod.rs          # UPDATE: Extract shared test utilities
│
├── .github/
│   └── workflows/
│       └── ci.yml                  # UPDATE: Add quality reporting job
│
└── Makefile                        # UPDATE: Add quality targets
```

## Dependencies

### New Dev Dependencies (workspace)

```toml
[workspace.dev-dependencies]
criterion = "0.5"       # Benchmarking
```

### External Tools

```bash
# Install via cargo
cargo install cargo-machete   # Dead code detection
cargo install cargo-bloat     # Binary size analysis
cargo install hyperfine       # Command benchmarking
```

### CI Dependencies

No new CI dependencies - uses existing cargo tooling and shell scripts.

## Implementation Phases

### Phase 1: Quality Measurement Infrastructure

**Goal**: Create scripts that produce consistent, machine-readable quality metrics.

**Deliverables**:
1. `checks/quality/evaluate.sh` producing JSON metrics
2. `checks/quality/benchmark.sh` measuring performance
3. `reports/quality/baseline.json` capturing current state
4. Makefile targets for easy invocation

**Key Code**:

```bash
#!/bin/bash
# checks/quality/evaluate.sh
#
# Produces JSON metrics for code quality tracking

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Collect LOC by crate
get_loc_by_crate() {
    local crate=$1
    local src_loc=$(find "crates/$crate/src" -name "*.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    local test_loc=$(find "crates/$crate/tests" -name "*.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    echo "{\"source\": $src_loc, \"test\": $test_loc}"
}

# File size statistics
get_file_stats() {
    local crate=$1
    local type=$2  # "src" or "tests"

    local files=$(find "crates/$crate/$type" -name "*.rs" 2>/dev/null || true)
    if [ -z "$files" ]; then
        echo "{\"count\": 0, \"avg\": 0, \"max\": 0, \"over_limit\": 0}"
        return
    fi

    local limit=700
    [ "$type" = "tests" ] && limit=1100

    local count=0
    local total=0
    local max=0
    local over_limit=0

    for f in $files; do
        local lines=$(wc -l < "$f")
        count=$((count + 1))
        total=$((total + lines))
        [ "$lines" -gt "$max" ] && max=$lines
        [ "$lines" -gt "$limit" ] && over_limit=$((over_limit + 1))
    done

    local avg=$((total / count))
    echo "{\"count\": $count, \"avg\": $avg, \"max\": $max, \"over_limit\": $over_limit}"
}

# Escape hatch counts
count_escape_hatches() {
    local pattern=$1
    grep -r "$pattern" crates/core/src crates/cli/src --include="*.rs" 2>/dev/null | wc -l | tr -d ' '
}

# Test counts
count_tests() {
    grep -r "#\[test\]" crates/core crates/cli --include="*.rs" 2>/dev/null | wc -l | tr -d ' '
}

# Build the JSON output
cat << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_sha": "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')",
  "loc": {
    "core": $(get_loc_by_crate core),
    "cli": $(get_loc_by_crate cli)
  },
  "file_stats": {
    "core": {
      "src": $(get_file_stats core src),
      "tests": $(get_file_stats core tests)
    },
    "cli": {
      "src": $(get_file_stats cli src),
      "tests": $(get_file_stats cli tests)
    }
  },
  "escape_hatches": {
    "unsafe": $(count_escape_hatches "unsafe "),
    "unwrap": $(count_escape_hatches "\.unwrap()"),
    "expect": $(count_escape_hatches "\.expect("),
    "allow_dead_code": $(count_escape_hatches "#\[allow(dead_code)\]")
  },
  "tests": {
    "count": $(count_tests)
  }
}
EOF
```

```bash
#!/bin/bash
# checks/quality/benchmark.sh
#
# Measures compile time, test time, binary size, and basic performance

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Cold compile time (from clean)
measure_cold_compile() {
    cargo clean 2>/dev/null
    local start=$(date +%s.%N)
    cargo build --release 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Incremental compile time (touch and rebuild)
measure_incremental_compile() {
    touch crates/cli/src/main.rs
    local start=$(date +%s.%N)
    cargo build --release 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Cold test time
measure_cold_test() {
    cargo clean 2>/dev/null
    local start=$(date +%s.%N)
    cargo test --all 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Warm test time
measure_warm_test() {
    local start=$(date +%s.%N)
    cargo test --all 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Binary sizes
measure_binary_sizes() {
    cargo build --release 2>/dev/null
    local oj_size=$(stat -f%z target/release/oj 2>/dev/null || stat -c%s target/release/oj 2>/dev/null || echo 0)

    # Stripped sizes
    cp target/release/oj target/release/oj-stripped 2>/dev/null || true
    strip target/release/oj-stripped 2>/dev/null || true
    local oj_stripped=$(stat -f%z target/release/oj-stripped 2>/dev/null || stat -c%s target/release/oj-stripped 2>/dev/null || echo 0)

    echo "{\"oj\": {\"release\": $oj_size, \"stripped\": $oj_stripped}}"
}

echo "Running benchmarks (this may take several minutes)..."

cold_compile=$(measure_cold_compile)
echo "Cold compile: ${cold_compile}s"

incremental_compile=$(measure_incremental_compile)
echo "Incremental compile: ${incremental_compile}s"

cold_test=$(measure_cold_test)
echo "Cold test: ${cold_test}s"

warm_test=$(measure_warm_test)
echo "Warm test: ${warm_test}s"

binary_sizes=$(measure_binary_sizes)
echo "Binary sizes collected"

cat << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_sha": "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')",
  "compile_time": {
    "cold_seconds": $cold_compile,
    "incremental_seconds": $incremental_compile
  },
  "test_time": {
    "cold_seconds": $cold_test,
    "warm_seconds": $warm_test
  },
  "binary_size": $binary_sizes
}
EOF
```

**Verification**:
- Run `./checks/quality/evaluate.sh` and verify JSON output is valid
- Run `./checks/quality/benchmark.sh` and verify measurements complete
- Run `make quality` to verify Makefile integration

---

### Phase 2: Dead Code Audit

**Goal**: Systematically review all code marked with `#[allow(dead_code)]`, delete truly dead code, and add justifying comments for future-epic code.

**Deliverables**:
1. Run `cargo machete` to find unused dependencies
2. Remove or use unused dependencies
3. Audit all `#[allow(dead_code)]` annotations in oj-core and oj CLI
4. Delete truly dead code
5. Add justifying comments for code needed in future epics
6. Remove global `dead_code = "allow"` from Cargo.toml where possible

**Process**:

1. **Run cargo machete**:
   ```bash
   cargo machete
   ```

2. **Audit dead code annotations** - for each file with `#[allow(dead_code)]`:
   - If code is unused and not needed: **delete it**
   - If code is for future epic: **add comment** like:
     ```rust
     #[allow(dead_code)] // Used in Epic 6: Strategy chains
     pub struct Strategy { ... }
     ```
   - If code is test-only: **move to test module** or use `#[cfg(test)]`

3. **Categorize dead code by crate**:

   **oj-core** - Currently has global `dead_code = "allow"` in Cargo.toml:
   - `coordination/` - Review guard types, lock/semaphore internals
   - `engine/` - Review recovery chain, worker types
   - `adapters/` - Review adapter methods not yet called from CLI

   **oj CLI** - Also has `dead_code = "allow"`:
   - Review command helper functions
   - Review output formatting utilities

**Key Changes**:

```toml
# crates/core/Cargo.toml
# CHANGE: Remove global allow, add targeted allows in code
[lints.rust]
# dead_code = "allow"  # REMOVE THIS LINE
unsafe_code = "forbid"
```

```rust
// Example of justified dead code annotation
// crates/core/src/coordination/guard.rs

/// Guard condition for checking lock state
#[allow(dead_code)] // Used in Epic 6: Runbook guards
pub enum GuardCondition {
    LockFree(String),
    IssuesComplete,
    BranchExists(String),
    Custom(String),
}
```

**Verification**:
- `cargo build --all` succeeds without dead_code warnings (only targeted allows)
- `cargo machete` reports no unused dependencies
- Each `#[allow(dead_code)]` has a comment referencing the epic that needs it

---

### Phase 3: File Size Enforcement

**Goal**: Split large files to meet limits (source ≤700 LOC, test ≤1100 LOC) and establish the `_tests.rs` sibling pattern.

**Files to Split**:

**oj-core** (6 files over 500 LOC):
| File | Current LOC | Action |
|------|-------------|--------|
| `queue.rs` | 1,269 | Split: extract operations to `queue_operations.rs` |
| `task.rs` | 920 | Review: may be acceptable, look for natural splits |
| `pipeline.rs` | 898 | Review: may be acceptable, look for natural splits |
| `engine/runtime.rs` | 907 | Review: may be acceptable, look for natural splits |
| `coordination/guard.rs` | 812 | Review: may be acceptable |
| `coordination/manager.rs` | 572 | OK (under 700) |

**Splitting Strategy**:

```rust
// BEFORE: queue.rs (1,269 LOC)
pub struct Queue { ... }
impl Queue {
    pub fn new() -> Self { ... }
    pub fn push(&mut self, item: Item) { ... }
    pub fn pop(&mut self) -> Option<Item> { ... }
    // Many more operations...
}

// AFTER: queue.rs (~600 LOC)
mod operations;
pub use operations::*;

pub struct Queue { ... }
impl Queue {
    pub fn new() -> Self { ... }
    // Core methods only
}

// AFTER: queue/operations.rs (~600 LOC)
impl super::Queue {
    pub fn push(&mut self, item: Item) { ... }
    pub fn pop(&mut self) -> Option<Item> { ... }
    // Additional operations
}
```

**Test Organization** - Establish `_tests.rs` sibling pattern:

```
crates/core/src/
├── queue.rs
├── queue_tests.rs         # Tests for queue.rs
├── task.rs
├── task_tests.rs          # Tests for task.rs (if tests exist inline)
```

```rust
// queue.rs - at the end
#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
```

**Verification**:
- Run `./checks/quality/evaluate.sh` and verify `over_limit` counts are 0
- All tests pass after splits
- No functionality changes (refactor only)

---

### Phase 4: Test Parametrization

**Goal**: Convert repetitive test patterns to use `yare` parametrized macros.

**Analysis** - Find repetitive patterns:

```bash
# Find test functions with similar names (suggesting variants)
grep -r "fn test_" crates/core crates/cli --include="*.rs" | \
  sed 's/_[0-9]*$//' | sort | uniq -c | sort -rn | head -20
```

**Targets for Conversion**:

1. **Queue priority ordering** - Tests for different priorities:
   ```rust
   #[parameterized(
       high_before_low = { Priority::High, Priority::Low, Ordering::Less },
       same_priority_fifo = { Priority::Normal, Priority::Normal, Ordering::Equal },
       low_after_high = { Priority::Low, Priority::High, Ordering::Greater },
   )]
   fn test_queue_ordering(a: Priority, b: Priority, expected: Ordering) { ... }
   ```

2. **Effect execution** - Similar patterns for different effect types
3. **Guard evaluation** - Tests for different guard conditions
4. **CLI output formats** - JSON vs text output tests

**Conversion Process**:

1. Identify groups of 3+ similar tests
2. Extract common test logic into parametrized function
3. Define parameter sets with descriptive names
4. Remove original test functions
5. Verify all test cases still covered

**Verification**:
- `cargo test --all` passes
- Test count may decrease but coverage remains same
- No test logic changes (consolidation only)

---

### Phase 5: Code Duplication Analysis

**Goal**: Identify and extract copy-paste patterns into shared utilities.

**Analysis Tools**:

```bash
# Check for duplicate string patterns
grep -r "fn " crates/core/src crates/cli/src --include="*.rs" -A 5 | \
  awk '/^--$/{next} {print}' | sort | uniq -d

# Check test setup duplication
grep -r "fn setup" crates/core/tests crates/cli/tests --include="*.rs" -A 10
```

**Common Patterns to Extract**:

1. **Test fixture setup** (in integration tests):
   ```rust
   // BEFORE: Duplicated in multiple test files
   fn setup_test_workspace() -> TempDir { ... }
   fn setup_fake_adapters() -> FakeAdapters { ... }

   // AFTER: Shared in common module
   // crates/cli/tests/common/fixtures.rs
   pub fn test_workspace() -> TempDir { ... }
   pub fn fake_adapters() -> FakeAdapters { ... }
   ```

2. **JSON serialization helpers**:
   ```rust
   // If multiple places have similar JSON building patterns
   // Extract to a builder or helper module
   ```

3. **Path manipulation** (if duplicated):
   ```rust
   // crates/core/src/util/paths.rs
   pub fn normalize_project_path(path: &Path) -> String { ... }
   ```

**Verification**:
- No functional changes
- Reduced LOC through deduplication
- Improved test maintainability

---

### Phase 6: CI Integration & Baseline

**Goal**: Add CI job for quality metrics and establish comparison baseline.

**Deliverables**:
1. New CI job running quality scripts
2. Artifacts upload for quality/benchmark reports
3. Baseline JSON file committed to repo
4. Comparison script for regression detection

**CI Configuration**:

```yaml
# .github/workflows/ci.yml - add new job

  quality:
    name: Quality Metrics
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install tools
        run: cargo install cargo-machete

      - name: Run quality evaluation
        run: |
          mkdir -p reports/quality
          ./checks/quality/evaluate.sh > reports/quality/current.json

      - name: Compare to baseline
        run: |
          ./checks/quality/compare.sh reports/quality/baseline.json reports/quality/current.json || true

      - name: Upload quality report
        uses: actions/upload-artifact@v4
        with:
          name: quality-report
          path: reports/quality/current.json
          retention-days: 30

  benchmark:
    name: Performance Benchmarks
    runs-on: macos-latest  # Consistent with dev machines
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Run benchmarks
        run: |
          mkdir -p reports/quality
          ./checks/quality/benchmark.sh > reports/quality/benchmarks.json

      - name: Upload benchmark report
        uses: actions/upload-artifact@v4
        with:
          name: benchmark-report
          path: reports/quality/benchmarks.json
          retention-days: 90
```

**Comparison Script**:

```bash
#!/bin/bash
# checks/quality/compare.sh
#
# Compares current metrics against baseline, highlights regressions

set -euo pipefail

BASELINE=$1
CURRENT=$2

if [ ! -f "$BASELINE" ]; then
    echo "No baseline found, skipping comparison"
    exit 0
fi

echo "=== Quality Comparison ==="
echo ""

# Compare file count over limit
baseline_over=$(jq '[.file_stats[].src.over_limit, .file_stats[].tests.over_limit] | add' "$BASELINE")
current_over=$(jq '[.file_stats[].src.over_limit, .file_stats[].tests.over_limit] | add' "$CURRENT")

if [ "$current_over" -gt "$baseline_over" ]; then
    echo "REGRESSION: Files over size limit increased from $baseline_over to $current_over"
    exit 1
fi

# Compare escape hatches
for hatch in unsafe unwrap expect allow_dead_code; do
    baseline_count=$(jq ".escape_hatches.$hatch" "$BASELINE")
    current_count=$(jq ".escape_hatches.$hatch" "$CURRENT")

    if [ "$current_count" -gt "$baseline_count" ]; then
        echo "WARNING: $hatch count increased from $baseline_count to $current_count"
    fi
done

# Compare test count (should not decrease)
baseline_tests=$(jq ".tests.count" "$BASELINE")
current_tests=$(jq ".tests.count" "$CURRENT")

if [ "$current_tests" -lt "$baseline_tests" ]; then
    echo "WARNING: Test count decreased from $baseline_tests to $current_tests"
fi

echo ""
echo "Comparison complete. No blocking regressions found."
```

**Makefile Targets**:

```makefile
# Add to Makefile

.PHONY: quality benchmark quality-compare

quality:
	@mkdir -p reports/quality
	@./checks/quality/evaluate.sh | tee reports/quality/current.json

benchmark:
	@mkdir -p reports/quality
	@./checks/quality/benchmark.sh | tee reports/quality/benchmarks.json

quality-compare:
	@./checks/quality/compare.sh reports/quality/baseline.json reports/quality/current.json

quality-baseline:
	@mkdir -p reports/quality
	@./checks/quality/evaluate.sh > reports/quality/baseline.json
	@echo "Baseline saved to reports/quality/baseline.json"
```

**Verification**:
- `make quality` produces valid JSON
- `make quality-compare` runs without error
- CI job uploads artifacts
- Baseline file committed to repo

## Key Implementation Details

### File Size Limits

| Type | Limit | Rationale |
|------|-------|-----------|
| Source files | ≤700 LOC | Fits in LLM context, manageable cognitive load |
| Test files | ≤1100 LOC | Tests often have more boilerplate, slightly higher limit |

### Dead Code Policy

1. **Delete** - Truly unused code with no future use case
2. **`#[cfg(test)]`** - Code only used in tests
3. **`#[allow(dead_code)]` + comment** - Code for future epics (must reference epic)

### Escape Hatch Policy

| Escape Hatch | Production Code | Test Code |
|--------------|-----------------|-----------|
| `unsafe` | Forbidden | Forbidden |
| `unwrap()` | Denied | Allowed |
| `expect()` | Denied | Allowed |
| `panic!()` | Denied | Allowed |

### Quality Metrics Schema

```json
{
  "timestamp": "ISO-8601",
  "git_sha": "short-sha",
  "loc": {
    "<crate>": { "source": number, "test": number }
  },
  "file_stats": {
    "<crate>": {
      "src": { "count": number, "avg": number, "max": number, "over_limit": number },
      "tests": { "count": number, "avg": number, "max": number, "over_limit": number }
    }
  },
  "escape_hatches": {
    "unsafe": number,
    "unwrap": number,
    "expect": number,
    "allow_dead_code": number
  },
  "tests": {
    "count": number
  }
}
```

### Benchmark Metrics Schema

```json
{
  "timestamp": "ISO-8601",
  "git_sha": "short-sha",
  "compile_time": {
    "cold_seconds": number,
    "incremental_seconds": number
  },
  "test_time": {
    "cold_seconds": number,
    "warm_seconds": number
  },
  "binary_size": {
    "oj": { "release": number, "stripped": number }
  }
}
```

## Verification Plan

### Phase 1 Verification
- [ ] `./checks/quality/evaluate.sh` produces valid JSON
- [ ] `./checks/quality/benchmark.sh` completes without error
- [ ] `make quality` and `make benchmark` work
- [ ] JSON output parseable with `jq`

### Phase 2 Verification
- [ ] `cargo machete` reports no unused dependencies
- [ ] `cargo build --all` succeeds
- [ ] All `#[allow(dead_code)]` have justifying comments
- [ ] Global `dead_code = "allow"` removed from Cargo.toml files

### Phase 3 Verification
- [ ] `over_limit` count is 0 in quality report
- [ ] No source file exceeds 700 LOC
- [ ] No test file exceeds 1100 LOC
- [ ] All tests pass after splits
- [ ] `_tests.rs` pattern established where applicable

### Phase 4 Verification
- [ ] `yare` parametrized tests compile and run
- [ ] Test coverage unchanged (same scenarios tested)
- [ ] Reduced test code duplication

### Phase 5 Verification
- [ ] Common test utilities extracted
- [ ] No identical code blocks across files
- [ ] Tests still pass after refactoring

### Phase 6 Verification
- [ ] CI quality job runs successfully
- [ ] CI benchmark job runs on main branch pushes
- [ ] Artifacts uploaded and downloadable
- [ ] Baseline committed to `reports/quality/baseline.json`
- [ ] `make quality-compare` detects regressions correctly

### Final Verification
- [ ] `make check` passes (includes all existing checks)
- [ ] All new scripts are executable
- [ ] Documentation updated (Makefile help, README if needed)
- [ ] No functional changes to existing code (refactor only)

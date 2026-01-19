# Epic: Use `.oj/` State Directory

**Root Feature:** `otters-9115`

## Overview

Migrate the state directory from `.build/operations/` to `.oj/` across the codebase. The current implementation incorrectly uses `.build/operations/` for persisting WAL state, snapshots, and related data. This change standardizes on the `.oj/` directory which better reflects the project name and avoids confusion with build artifacts.

**Change Summary:**
- `.build/operations/` → `.oj/`
- `.build/operations/pipelines/` → `.oj/pipelines/`
- `.build/` (gitignore) → `.oj/`

## Project Structure

### Affected Files by Category

**CLI Source Code (6 files):**
- `crates/cli/src/adapters.rs` - Engine factory, store discovery
- `crates/cli/src/commands/pipeline.rs` - Pipeline management
- `crates/cli/src/commands/workspace.rs` - Workspace management
- `crates/cli/src/commands/queue.rs` - Queue operations
- `crates/cli/src/commands/signal.rs` - Signal (done/checkpoint) commands
- `crates/cli/tests/common/mod.rs` - Test utilities

**Configuration (2 files):**
- `.gitignore` - Ignore patterns
- `crates/cli/CLAUDE.md` - CLI documentation

**Test Specifications (5 files):**
- `tests/specs/helpers/common.bash` - BATS test helpers
- `tests/specs/edge_cases/error_handling.bats`
- `tests/specs/integration/daemon_lifecycle.bats`
- `tests/specs/integration/workspace_management.bats`
- `tests/specs/integration/pipeline_create.bats`

**Scripts (2 files):**
- `scripts/smoke-test.sh`
- `scripts/migrate-from-bash.sh`

**Documentation (4 files):**
- `docs/05-operations/01-runbook.md`
- `docs/05-operations/02-troubleshooting.md`
- `docs/06-migration/01-from-bash.md`
- `docs/TEST_HAIKU_RUNBOOK.md`

## Dependencies

No external dependencies required. This is a path string replacement across the codebase.

## Implementation Phases

### Phase 1: Core CLI Source Code

Update all Rust source files to use `.oj` instead of `.build/operations`.

**Files to modify:**

1. **`crates/cli/src/adapters.rs`** (3 locations):
   ```rust
   // Line 58: make_engine_with_root
   let store_path = root.join(".oj");

   // Line 70: find_or_create_store
   let local_store = Path::new(".oj");

   // Line 78: find_or_create_store loop
   let store_path = dir.join(".oj");
   ```

2. **`crates/cli/src/commands/pipeline.rs`** (3 locations):
   ```rust
   // Lines 64, 91, 121
   WalStore::open_default(Path::new(".oj"))?
   ```

3. **`crates/cli/src/commands/workspace.rs`** (4 locations):
   ```rust
   // Lines 70, 105, 129, 143
   WalStore::open_default(Path::new(".oj"))?
   ```

4. **`crates/cli/src/commands/queue.rs`** (4 locations):
   ```rust
   // Lines 101, 159, 180, 228
   WalStore::open_default(Path::new(".oj"))?
   ```

5. **`crates/cli/src/commands/signal.rs`** (1 location):
   ```rust
   // Line 95: detect_workspace_id
   let store_path = dir.join(".oj/pipelines");
   // Note: Comment on line 92 should also be updated
   ```

6. **`crates/cli/tests/common/mod.rs`** (2 locations):
   ```rust
   // Lines 29, 68-69
   fs::create_dir_all(temp.path().join(".oj"))?
   ```

**Verification:**
```bash
cargo build -p otters-cli
cargo test -p otters-cli
```

### Phase 2: Test Specifications (BATS)

Update all BATS test specifications and helpers.

**Files to modify:**

1. **`tests/specs/helpers/common.bash`** (4 locations):
   ```bash
   # Line 68: Comment update
   # Initialize oj project structure (git + .oj)

   # Lines 72-74: Directory creation
   mkdir -p "$dir/.oj/pipelines"
   mkdir -p "$dir/.oj/workspaces"
   mkdir -p "$dir/.oj/queues"
   ```

2. **`tests/specs/edge_cases/error_handling.bats`** (4 locations):
   ```bash
   # Line 63-64
   @test "oj handles missing .oj directory" {
       rm -rf "$BATS_FILE_TMPDIR/.oj"

   # Lines 72-73
   mkdir -p "$BATS_FILE_TMPDIR/.oj/pipelines"
   echo "not valid json" > "$BATS_FILE_TMPDIR/.oj/pipelines/corrupted.json"
   ```

3. **`tests/specs/integration/daemon_lifecycle.bats`** (1 location):
   ```bash
   # Line 23
   rm -rf "$BATS_FILE_TMPDIR/.oj/pipelines"/*
   ```

4. **`tests/specs/integration/workspace_management.bats`** (1 location):
   ```bash
   # Line 21
   rm -rf "$BATS_FILE_TMPDIR/.oj/workspaces"/*
   ```

5. **`tests/specs/integration/pipeline_create.bats`** (2 locations):
   ```bash
   # Line 21
   assert_dir_exists ".oj/workspaces/test-workspace"

   # Line 37
   assert_file_exists ".oj/pipelines/test-state.json"
   ```

**Verification:**
```bash
./scripts/spec  # Run BATS tests
```

### Phase 3: Scripts and Configuration

Update utility scripts and configuration files.

**Files to modify:**

1. **`scripts/smoke-test.sh`** (2 locations):
   ```bash
   # Line 51
   mkdir -p .oj

   # Line 84
   if [[ -f ".oj/pipelines/build-smoke-test/state.json" ]]; then
   ```

2. **`scripts/migrate-from-bash.sh`** (1 location):
   ```bash
   # Line 64
   OJ_STATE_DIR="${PWD}/.oj"
   ```

3. **`.gitignore`** (1 location):
   ```
   # Line 4
   .oj/
   ```
   Note: Remove `.build/` from gitignore (or keep both during transition).

4. **`crates/cli/CLAUDE.md`** (3 locations):
   ```markdown
   # Line 52
   Pipeline and workspace state is stored in `.oj/`:

   # Lines 55-58
   .oj/
   ├── pipelines/
   ├── workspaces/
   └── queues/
   ```

**Verification:**
```bash
./scripts/smoke-test.sh
git status  # Verify .oj/ is ignored
```

### Phase 4: Documentation Updates

Update all documentation files referencing the old path.

**Files to modify:**

1. **`docs/05-operations/01-runbook.md`**:
   - Line 92: `ls -lh .oj/wal/`
   - Line 153: `Snapshots are stored in .oj/snapshots/`
   - Line 187: `.oj/`

2. **`docs/05-operations/02-troubleshooting.md`** (many locations):
   - Line 153: `df -h .oj/`
   - Line 158: `.oj/wal/`
   - Line 163: `du -sh .oj/wal/`
   - Line 179: `cat .oj/pipelines/<id>.json | jq .`
   - Line 182: `rm .oj/pipelines/<id>.json`
   - Line 264: `ls -la .oj/`
   - Line 275: `tail -100 .oj/logs/daemon.log`
   - Line 292: `tail -500 .oj/logs/daemon.log`

3. **`docs/06-migration/01-from-bash.md`**:
   - Line 181: `rm -rf .oj/`

4. **`docs/TEST_HAIKU_RUNBOOK.md`** (many locations):
   - Update all `.build/operations` references to `.oj`

**Verification:**
```bash
grep -r "\.build/operations" docs/  # Should return empty
grep -r "\.build/" docs/ | grep -v "pipeline.build"  # Should return empty
```

### Phase 5: Final Verification

Run the complete verification suite:

```bash
# Format check
cargo fmt --all -- --check

# Lint check
cargo clippy --all-targets --all-features -- -D warnings

# Unit tests
cargo test --all

# Build
cargo build --all

# BATS specs
./scripts/spec

# Smoke test
./scripts/smoke-test.sh

# Full check
make check
```

## Key Implementation Details

### Pattern Replacement Summary

| Old Path | New Path |
|----------|----------|
| `.build/operations` | `.oj` |
| `.build/operations/pipelines` | `.oj/pipelines` |
| `.build/operations/workspaces` | `.oj/workspaces` |
| `.build/operations/queues` | `.oj/queues` |
| `.build/wal` | `.oj/wal` |
| `.build/snapshots` | `.oj/snapshots` |
| `.build/logs` | `.oj/logs` |

### Files NOT to Change

The following files contain `.build` in contexts unrelated to the state directory (e.g., `pipeline.build`, `command.build`, method names like `.build()`):

- `runbooks/*.toml` - Contains `[pipeline.build]`, `[command.build]` sections
- `docs/01-concepts/RUNBOOKS.md` - Documents pipeline/command definitions
- `docs/10-example-runbooks/*.toml` - Example runbooks
- `crates/core/tests/*.rs` - Test cases with TOML snippets
- `crates/core/src/**/*.rs` - Method calls like `.build()`, `.build_context()`

### Migration Considerations

1. **Backward Compatibility**: Users with existing `.build/operations/` directories will need to either:
   - Manually move data: `mv .build/operations .oj`
   - Start fresh (state will be recreated)

2. **Gitignore**: Keep `.build/` in `.gitignore` temporarily to avoid committing old directories during migration period.

## Verification Plan

### Unit Tests
```bash
cargo test --all
```

### Integration Tests
```bash
./scripts/spec
```

### Manual Verification
1. Create a new project and run `oj pipeline list`
2. Verify `.oj/` directory is created
3. Verify `.oj/wal.jsonl` exists after state operations
4. Run `./done` signal and verify state persistence

### Regression Check
```bash
# Ensure no old paths remain in source code
grep -r "\.build/operations" crates/
# Should return empty

# Ensure no old paths in active scripts
grep -r "\.build/operations" scripts/
# Should return empty

# Ensure no old paths in active tests
grep -r "\.build/operations" tests/
# Should return empty
```

### Full Check
```bash
make check
./checks/lint.sh
```

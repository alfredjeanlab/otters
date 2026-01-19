# Plan: Complete Migration to Runbook-Only Pipelines

## Overview

Remove legacy pipeline constructors and make `oj run` a generic runbook-driven command. Pipelines will only be created via runbook definitions.

## Current State

- `Pipeline::new_build()` and `Pipeline::new_bugfix()` exist as legacy constructors
- CLI commands (`oj run build`, `oj run bugfix`) exist in `run.rs`
- `dynamic.rs:257` uses `new_build()` internally
- `PipelineKind` enum has `Build` and `Bugfix` variants
- ~40 tests use legacy constructors

## Changes

### Phase 1: Update Pipeline Core

**File: `crates/core/src/pipeline.rs`**

1. Replace `PipelineKind::Build` and `PipelineKind::Bugfix` with `PipelineKind::Dynamic`:
   ```rust
   pub enum PipelineKind {
       #[serde(alias = "Build", alias = "Bugfix")]
       Dynamic,
   }
   ```

2. Add `Pipeline::new_dynamic()` constructor:
   ```rust
   pub fn new_dynamic(
       id: impl Into<String>,
       name: impl Into<String>,
       inputs: HashMap<String, String>,
   ) -> Self
   ```

3. Remove `new_build()` (lines 143-164) and `new_bugfix()` (lines 167-188)

4. Update `phase_sequence()` to return `vec![Phase::Init, Phase::Done]` for Dynamic kind (actual phases come from runbook metadata in outputs)

5. Update `first_working_phase()` to check runbook metadata

### Phase 2: Update Dynamic Pipeline Creation

**File: `crates/core/src/pipelines/dynamic.rs`**

1. Replace `Pipeline::new_build()` at line 257 with `Pipeline::new_dynamic()`:
   ```rust
   let mut pipeline = Pipeline::new_dynamic(&id, &def.name, full_inputs);
   ```

### Phase 3: Make CLI Run Command Generic (Runbook-Driven)

**File: `crates/cli/src/commands/run.rs`**

Replace hardcoded `Build`/`Bugfix` variants with a generic runbook-driven command:

```rust
#[derive(Parser)]
pub struct RunCommand {
    /// Runbook name (e.g., "build", "bugfix")
    runbook: String,
    /// Pipeline name within the runbook (defaults to runbook name)
    #[arg(short, long)]
    pipeline: Option<String>,
    /// Pipeline inputs as key=value pairs
    #[arg(short, long, value_parser = parse_key_val)]
    input: Vec<(String, String)>,
}
```

Usage becomes:
- `oj run build --input name=auth --input prompt="Add authentication"`
- `oj run bugfix --input bug=123`

Changes:
1. Remove `RunCommand` enum with `Build`/`Bugfix` variants
2. Add generic `RunCommand` struct with runbook/pipeline/inputs
3. Remove `run_build()` and `run_bugfix()` functions
4. Add generic `run_pipeline()` that:
   - Loads runbook from registry
   - Creates pipeline via `engine.create_runbook_pipeline()`
   - Errors clearly if runbook not found
5. Keep `generate_claude_md()` helper but make it template-based from runbook

### Phase 4: Migrate Tests to Use Runbook Equivalents

**Files to update:**
- `crates/core/src/pipeline_tests.rs` (~21 usages)
- `crates/core/tests/engine_integration.rs` (~7 usages)
- `crates/core/src/engine/runtime_tests.rs` (~3 usages)
- `crates/core/src/storage/json_tests.rs` (1 usage)
- `crates/core/src/pipelines/dynamic_tests.rs` (2 usages)

**Strategy: Use runbook-based pipeline creation in tests**

Note: Follow project convention - tests in `*_tests.rs` sibling files, not nested modules.

1. Use `include_str!` to load the actual runbook files from `runbooks/`:
```rust
// crates/core/src/pipeline_tests.rs (at top of file)

const BUILD_RUNBOOK: &str = include_str!("../../../runbooks/build.toml");
const BUGFIX_RUNBOOK: &str = include_str!("../../../runbooks/bugfix.toml");
```

This ensures tests use the same runbook definitions as production, and validates the runbooks at compile time.

2. Add helper function in same `*_tests.rs` file:
```rust
fn create_test_pipeline(runbook_toml: &str, pipeline_name: &str, inputs: HashMap<String, String>) -> Pipeline {
    let raw = parse_runbook(runbook_toml).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();
    let def = runbook.pipelines.get(pipeline_name).unwrap();
    create_pipeline("test-id", def, inputs, &FakeClock::new()).unwrap()
}
```

3. Replace test usages:
```rust
// Before:
let pipeline = Pipeline::new_build("id", "name", "prompt");

// After:
let pipeline = create_test_pipeline(
    BUILD_RUNBOOK,
    "build",
    HashMap::from([
        ("name".to_string(), "name".to_string()),
        ("prompt".to_string(), "prompt".to_string()),
    ])
);
```

4. For simpler unit tests that just need a Pipeline struct (not full runbook behavior), use `Pipeline::new_dynamic()` directly:
```rust
// For tests that don't care about runbook semantics
let pipeline = Pipeline::new_dynamic("id", "name", HashMap::new());
```

### Phase 5: Cleanup

1. Update `Phase` enum - keep all variants for backward compatibility with stored pipelines
2. Remove any dead code flagged by clippy
3. Update `crates/core/src/pipelines/CLAUDE.md` to reflect changes

## File Summary

| File | Action |
|------|--------|
| `crates/core/src/pipeline.rs` | Add `new_dynamic`, remove `new_build`/`new_bugfix`, update `PipelineKind` |
| `crates/core/src/pipelines/dynamic.rs` | Use `new_dynamic` instead of `new_build` |
| `crates/cli/src/commands/run.rs` | Refactor to generic runbook-driven command |
| `crates/cli/src/main.rs` | Update `Run` command handling for new signature |
| `crates/core/src/pipeline_tests.rs` | Migrate tests to `new_dynamic` |
| `crates/core/tests/engine_integration.rs` | Migrate tests to `new_dynamic` |
| `crates/core/src/engine/runtime_tests.rs` | Migrate tests to `new_dynamic` |
| `crates/core/src/storage/json_tests.rs` | Migrate tests to `new_dynamic` |
| `crates/core/src/pipelines/dynamic_tests.rs` | Migrate tests to `new_dynamic` |
| `crates/core/src/pipelines/CLAUDE.md` | Update documentation |
| `crates/cli/CLAUDE.md` | Update run command docs for new syntax |

## Verification

```bash
./checks/lint.sh
make check
```

Manual verification:
- [ ] `oj run build --input name=test --input prompt="test"` works
- [ ] `oj run bugfix --input bug=123` works
- [ ] Clear error when runbook not found
- [ ] Old persisted pipelines still load (serde alias handles migration)
- [ ] `oj pipeline list` still works
- [ ] `oj done` still works for existing pipelines

# Epic 6 Part 2: Address Implementation Gaps

**Parent:** `plans/epic-6.md`
**Root Feature:** `otters-8079`

Address gaps identified in the Epic 6 implementation review.

## 1. Gaps to Address

### Gap 1: Missing `runbook/CLAUDE.md`

The plan specified `crates/core/src/runbook/CLAUDE.md` but it was not created.

**Action:** Create module documentation following existing CLAUDE.md patterns.

### Gap 2: Legacy Pipeline Files Not Removed

The plan specified removing `build.rs` and `bugfix.rs` after verification, but they remain for backward compatibility.

**Decision needed:** Either:
- (A) Remove them now that runbook-based pipelines work
- (B) Keep them and document the deprecation path
- (C) Keep them indefinitely as fallback

**Recommendation:** Option (A) - Remove them. The CLI already falls back gracefully if runbooks aren't loaded, but having two code paths increases maintenance burden.

### Gap 3: No Integration Tests in `tests/` Directory

The plan specified:
- `tests/runbook_pipeline.rs`
- `tests/strategy_integration.rs`

Unit tests exist but end-to-end integration tests were not created.

**Action:** Create integration tests that exercise the full pipeline from TOML parsing through execution.

## 2. Implementation

### Phase 1: Add `runbook/CLAUDE.md`

**File:** `crates/core/src/runbook/CLAUDE.md`

**Content outline:**
- Module overview (parser → validator → loader pipeline)
- Submodule responsibilities
- Type hierarchy (Raw → Validated → Runtime)
- Template syntax reference
- Input format reference
- Cross-runbook reference syntax
- Testing patterns

### Phase 2: Remove Legacy Pipeline Files

**Files to remove:**
- `crates/core/src/pipelines/build.rs`
- `crates/core/src/pipelines/build_tests.rs`
- `crates/core/src/pipelines/bugfix.rs`
- `crates/core/src/pipelines/bugfix_tests.rs`

**Files to update:**
- `crates/core/src/pipelines/mod.rs` - Remove imports/exports
- `crates/core/src/pipeline.rs` - Remove `new_build`, `new_bugfix` constructors if they exist

**Verification:**
- `cargo build` succeeds
- `cargo test` passes
- CLI still works with runbook-based pipelines

### Phase 3: Add Integration Tests

**File:** `crates/core/tests/runbook_pipeline.rs`

```rust
//! Integration tests for runbook-based pipelines

#[tokio::test]
async fn build_pipeline_from_runbook() {
    // Load test runbooks
    // Create pipeline from definition
    // Verify initial state
    // Drive through phases
    // Verify completion
}

#[tokio::test]
async fn pipeline_template_interpolation() {
    // Create pipeline with inputs
    // Verify templates are rendered correctly in phase configs
}

#[tokio::test]
async fn pipeline_with_strategy_phase() {
    // Create pipeline with strategy phase
    // Verify strategy is created and linked
}
```

**File:** `crates/core/tests/strategy_integration.rs`

```rust
//! Integration tests for strategy execution

#[tokio::test]
async fn strategy_tries_approaches_in_order() {
    // First attempt fails, second succeeds
}

#[tokio::test]
async fn strategy_rolls_back_on_failure() {
    // Verify rollback command runs with checkpoint
}

#[tokio::test]
async fn strategy_escalates_on_exhaust() {
    // All attempts fail, verify escalation
}
```

**File:** `crates/core/tests/runbook_validation.rs`

```rust
//! Integration tests for runbook validation

#[test]
fn all_example_runbooks_are_valid() {
    // Validate runbooks/build.toml
    // Validate runbooks/bugfix.toml
}

#[test]
fn invalid_runbook_produces_clear_errors() {
    // Test various invalid runbooks
    // Verify error messages are helpful
}
```

## 3. Verification

Before committing:

```bash
./checks/lint.sh
make check
```

Specific checks:
- [ ] `runbook/CLAUDE.md` exists and follows project patterns
- [ ] No references to removed `build.rs`/`bugfix.rs` files
- [ ] All integration tests pass
- [ ] `oj run build` still works with runbook-based pipeline
- [ ] `oj run bugfix` still works with runbook-based pipeline

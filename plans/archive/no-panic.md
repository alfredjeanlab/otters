# No Panic: Panic-Free Error Handling Refactor

## Overview

Refactor the codebase to enforce panic-free code by enabling `clippy::unwrap_used` and `clippy::expect_used` lints as `deny`. This involves:
- Uncommenting the lint configurations in both Cargo.toml files
- Adding `#[cfg_attr(test, allow(...))]` attributes for test code
- Refactoring ~70 production code instances of `unwrap()`/`expect()` to proper error handling

## Project Structure

Files requiring changes:

```
crates/
├── core/
│   ├── Cargo.toml                    # Uncomment lint denials
│   ├── src/
│   │   ├── lib.rs                    # Add allow attributes for tests
│   │   ├── adapters/
│   │   │   └── fake.rs               # Add module-level allow (test utility)
│   │   ├── engine/
│   │   │   └── runtime.rs            # Refactor 4 unwrap calls
│   │   ├── storage/
│   │   │   └── json.rs               # Refactor 1 unwrap call
│   │   ├── queue.rs                  # Test-only (already covered)
│   │   ├── events/
│   │   │   ├── bus.rs                # Refactor unwrap calls
│   │   │   └── log.rs                # Test-only (already covered)
│   │   ├── coordination/
│   │   │   ├── guard.rs              # Refactor unwrap calls
│   │   │   ├── storage.rs            # Refactor unwrap calls
│   │   │   └── manager.rs            # Refactor unwrap calls
│   │   ├── clock.rs                  # Refactor unwrap calls
│   │   ├── task.rs                   # Test-only (already covered)
│   │   ├── pipeline.rs               # Test-only (already covered)
│   │   └── pipelines/
│   │       ├── bugfix.rs             # Refactor unwrap calls
│   │       └── build.rs              # Refactor unwrap calls
│   └── tests/
│       └── engine_integration.rs     # Add file-level allow
├── cli/
│   ├── Cargo.toml                    # Uncomment lint denials
│   └── src/
│       ├── commands/
│       │   ├── signal.rs             # Refactor unwrap calls
│       │   ├── queue.rs              # Refactor 1 unwrap call
│       │   └── pipeline.rs           # Refactor 1 unwrap call
```

## Dependencies

No new dependencies required. The codebase already has:
- `thiserror = "2"` - For custom error type definitions
- `anyhow = "1"` - For CLI error handling (in cli crate only)

## Implementation Phases

### Phase 1: Enable Lints and Add Test Allows

**Goal:** Uncomment lint denials and add allow attributes to test code, making the lints enforceable.

1. **Update Cargo.toml files:**

   ```toml
   # crates/core/Cargo.toml and crates/cli/Cargo.toml
   [lints.clippy]
   unwrap_used = "deny"
   expect_used = "deny"
   panic = "deny"
   ```

2. **Update `crates/core/src/lib.rs`:**

   ```rust
   // Allow panic!/unwrap/expect in test code
   #![cfg_attr(test, allow(clippy::panic))]
   #![cfg_attr(test, allow(clippy::unwrap_used))]
   #![cfg_attr(test, allow(clippy::expect_used))]
   ```

3. **Add allow attribute to `crates/core/src/adapters/fake.rs`:**

   ```rust
   //! Fake adapter implementations for testing
   #![allow(clippy::unwrap_used)]
   #![allow(clippy::expect_used)]
   ```

4. **Add allow attribute to `crates/core/tests/engine_integration.rs`:**

   ```rust
   #![allow(clippy::unwrap_used)]
   #![allow(clippy::expect_used)]
   ```

5. **Run `cargo clippy` to identify all remaining violations.**

**Verification:** `cargo clippy --all-targets -- -D warnings` shows specific files needing refactoring.

---

### Phase 2: Refactor Core Engine (runtime.rs)

**Goal:** Fix the 4 unwrap calls in the engine runtime that maintain internal invariants.

**File:** `crates/core/src/engine/runtime.rs`

**Pattern:** Convert HashMap lookups from unwrap to proper error handling.

**Before (line 273):**
```rust
let task = self.tasks.get(task_id).unwrap();
```

**After:**
```rust
let task = self.tasks.get(task_id)
    .ok_or_else(|| EngineError::TaskNotFound(task_id.clone()))?;
```

**Lines to refactor:**
- Line 273: `self.tasks.get(task_id).unwrap()` in `process_task_event`
- Line 511: `self.recovery_states.get_mut(task_id).unwrap()` in recovery nudge
- Line 533: `self.recovery_states.get_mut(task_id).unwrap()` in recovery restart
- Line 548: `self.recovery_states.get_mut(task_id).unwrap()` in recovery escalate

**Verification:** `cargo test -p oj-core` passes.

---

### Phase 3: Refactor Storage and Events

**Goal:** Fix unwrap calls in storage and event handling code.

**Files:**
- `crates/core/src/storage/json.rs` (1 production unwrap)
- `crates/core/src/events/bus.rs` (several unwrap calls)

**storage/json.rs (line 49):**

**Before:**
```rust
fs::create_dir_all(path.parent().unwrap())?;
```

**After:**
```rust
if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
}
```

**events/bus.rs pattern:** Convert channel operations to handle errors gracefully:

**Before:**
```rust
sender.send(event.clone()).unwrap();
```

**After:**
```rust
// Channel closed means subscriber dropped - just skip
let _ = sender.send(event.clone());
```

**Verification:** `cargo test -p oj-core -- storage events` passes.

---

### Phase 4: Refactor Coordination Module

**Goal:** Fix unwrap calls in coordination guards, storage, and manager.

**Files:**
- `crates/core/src/coordination/guard.rs`
- `crates/core/src/coordination/storage.rs`
- `crates/core/src/coordination/manager.rs`

**Pattern for guard.rs:** Many unwrap calls are on lock acquisitions. These should propagate errors:

**Before:**
```rust
let mut state = self.state.lock().unwrap();
```

**After (if lock poisoning is unrecoverable):**
```rust
// Lock poisoning indicates a panic in another thread - propagate
let mut state = self.state.lock()
    .map_err(|_| CoordinationError::LockPoisoned)?;
```

**Or (if we want to recover from poison):**
```rust
let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
```

**Note:** Since the application uses `panic = "abort"`, lock poisoning cannot occur in practice. However, for API correctness, we should use `.unwrap_or_else(|e| e.into_inner())` which recovers from poisoning.

**Verification:** `cargo test -p oj-core -- coordination` passes.

---

### Phase 5: Refactor Clock, Pipelines, and CLI

**Goal:** Fix remaining unwrap calls in clock, pipeline implementations, and CLI commands.

**Files:**
- `crates/core/src/clock.rs`
- `crates/core/src/pipelines/bugfix.rs`
- `crates/core/src/pipelines/build.rs`
- `crates/cli/src/commands/signal.rs`
- `crates/cli/src/commands/queue.rs`
- `crates/cli/src/commands/pipeline.rs`

**clock.rs pattern:** FakeClock uses Mutex, apply same pattern as Phase 4.

**CLI commands pattern:** Since CLI uses anyhow, convert:

**Before:**
```rust
uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
```

**After:**
```rust
uuid::Uuid::new_v4().to_string().split('-').next()
    .expect("UUID always has at least one segment")
```

Wait - we're denying expect too. Better:

```rust
// UUID v4 format is xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx, always has first segment
uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
```

Or even simpler with anyhow context:
```rust
uuid::Uuid::new_v4().to_string().split('-').next()
    .context("UUID format error")?
```

**Verification:** `cargo clippy --all-targets -- -D warnings` shows no violations.

---

### Phase 6: Final Verification

**Goal:** Ensure all lints pass and tests succeed.

1. Run full CI check: `make check`
2. Verify no unwrap/expect in production code: `cargo clippy --all-targets -- -D warnings`
3. Run all tests: `cargo test --all`
4. Build release: `cargo build --release`

## Key Implementation Details

### Error Handling Patterns

1. **HashMap lookups (internal invariants):** Use `ok_or_else` with appropriate error:
   ```rust
   self.map.get(key).ok_or_else(|| Error::NotFound(key.clone()))?
   ```

2. **Mutex locks (with panic=abort):** Use `unwrap_or_else` to handle poisoning:
   ```rust
   self.state.lock().unwrap_or_else(|e| e.into_inner())
   ```

3. **Channel sends (subscriber may have dropped):** Ignore errors:
   ```rust
   let _ = sender.send(event);
   ```

4. **Path operations:** Use if-let or match:
   ```rust
   if let Some(parent) = path.parent() {
       fs::create_dir_all(parent)?;
   }
   ```

5. **String operations with known format:** Restructure to avoid fallibility:
   ```rust
   // Instead of split().next().unwrap()
   uuid.simple().to_string()[..8].to_string()
   ```

### New Error Variants (if needed)

If existing error types don't cover new failure modes, add variants:

```rust
// In coordination/mod.rs or a new error.rs
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    #[error("lock poisoned")]
    LockPoisoned,

    #[error("semaphore not found: {0}")]
    SemaphoreNotFound(String),

    // ... other variants
}
```

### Test File Pattern

All test modules and files should have allow attributes at the appropriate scope:

```rust
// For integration test files (e.g., tests/engine_integration.rs)
#![allow(clippy::unwrap_used, clippy::expect_used)]

// For inline test modules (automatic via lib.rs cfg_attr)
#[cfg(test)]
mod tests {
    // unwrap/expect allowed here
}
```

## Verification Plan

1. **Phase 1 complete:** `cargo clippy` identifies all remaining violations (expected ~70)
2. **Phase 2 complete:** `cargo test -p oj-core` passes, engine tests work
3. **Phase 3 complete:** Storage and event tests pass
4. **Phase 4 complete:** Coordination tests pass
5. **Phase 5 complete:** Full `cargo clippy --all-targets -- -D warnings` passes
6. **Phase 6 complete:** `make check` passes (fmt, clippy, test, build, audit, deny)

### Commands for Verification

```bash
# After each phase
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all

# Final verification
make check
```

### Expected Behavior After Refactor

- All production code paths return `Result` on fallible operations
- No panics possible in release builds (panic = abort + no unwrap/expect)
- Test code remains ergonomic with allowed unwrap/expect
- Error messages are descriptive and actionable

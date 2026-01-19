# Otters (oj)

An agentic development team to write software and automate other tasks.

## Directory Structure

```
otters/
├── crates/           # Rust workspace
│   ├── cli/          # Command-line interface
│   └── core/         # Core library
├── checks/           # Lint and quality scripts
├── docs/             # Architecture documentation
├── plans/            # Epic implementation plans
└── scripts/          # Build and utility scripts
```

## Development Policies

### Dead Code Policy
- All unused code must be removed, not commented out
- No `#[allow(dead_code)]` without documented justification
- Unused dependencies must be removed from Cargo.toml

### Escape Hatch Policy
- `unsafe` blocks require safety comment explaining invariants
- `unwrap()`/`expect()` only in:
  - Tests
  - Infallible cases with comment explaining why
  - CLI parsing where panic is acceptable
- `#[allow(...)]` requires justification comment above

### Test Conventions
- Unit tests in `*_tests.rs` files, imported via `#[cfg(test)]`
- Integration tests in `tests/` directory
- Use `FakeClock`, `FakeAdapters` for deterministic tests
- Property tests for state machine transitions

## Landing the Plane

Before committing changes:

- [ ] Run `./checks/lint.sh` (or `make lint`)
- [ ] Run `make check` for full verification
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all`
  - `cargo build --all`
  - `cargo audit`
  - `cargo deny check`

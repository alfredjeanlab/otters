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
- Unused dependencies must be removed from Cargo.toml

### Test Conventions
- Unit tests in `*_tests.rs` files, imported from the module under test:
  ```rust
  // In protocol.rs:
  #[cfg(test)]
  #[path = "protocol_tests.rs"]
  mod tests;
  ```
- Integration tests in `tests/` directory
- Use `FakeClock`, `FakeAdapters` for deterministic tests
- Property tests for state machine transitions

## Commits

Use conventional commit format: `type(scope): description`
Types: feat, fix, chore, docs, test, refactor

## Landing the Plane

Before committing changes:

- [ ] Run `./scripts/lint` (or `make lint`)
- [ ] Run `make check` for full verification
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `scripts/policy`
  - `quench check`
  - `cargo test --all`
  - `cargo build --all`
  - `cargo audit`
  - `cargo deny check licenses bans sources`

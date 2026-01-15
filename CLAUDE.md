# Otters (oj)

An agentic development team to write software and automate other tasks.

## Directory Structure

```
otters/
├── crates/           # Rust workspace
│   ├── cli/          # Command-line interface
│   └── core/         # Core library
├── docs/             # Architecture documentation
├── plans/            # Epic implementation plans
└── scripts/          # Build and utility scripts
```

## Landing the Plane

Before committing changes:

- [ ] Run `make check` which will
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all`
  - `cargo build --all`
  - `cargo audit`
  - `cargo deny check`

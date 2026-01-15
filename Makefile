# Makefile for otters project

SHELL := /bin/bash

.PHONY: check install license outdated coverage spec lint-specs

# Build and install oj to ~/.local/bin
install:
	@scripts/install

# Run all CI checks
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all
	cargo build --all
	cargo audit
	cargo deny check licenses bans sources

# Add license headers
license:
	@scripts/license

# Check for outdated dependencies
outdated:
	cargo outdated

# Generate code coverage report
FMT := --html
coverage:
	@cargo llvm-cov clean --workspace
	@if [ -t 1 ] && [ "$(FMT)" = "--html" ]; then cargo llvm-cov $(FMT) --open; else cargo llvm-cov $(FMT); fi

# Run BATS specs
spec:
	@./scripts/spec

# Lint shell scripts
lint-specs:
	@shellcheck -x -S warning tests/specs/helpers/*.bash
	@shellcheck -x -S warning scripts/spec
	@shellcheck -x -S warning scripts/init-worktree
	@shellcheck -x -S warning tests/specs/bats/install.sh

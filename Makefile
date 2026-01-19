# Makefile for otters project

SHELL := /bin/bash

.PHONY: check install license outdated coverage spec lint-specs quality benchmark quality-compare quality-baseline lint lint-fix

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

# Quality metrics
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

# Lint checks
lint:
	./checks/lint.sh

lint-fix:
	./checks/lint.sh --fix

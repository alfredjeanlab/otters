.PHONY: check fmt install license lint outdated

# Run all CI checks
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	scripts/policy
	quench check
	cargo test --all
	cargo build --all
	cargo audit
	cargo deny check licenses bans sources

# Format code
fmt:
	cargo fmt --all

# Build and install oj to ~/.local/bin
install:
	@scripts/install

# Add license headers
license:
	@scripts/license

# Check for outdated dependencies
outdated:
	cargo outdated

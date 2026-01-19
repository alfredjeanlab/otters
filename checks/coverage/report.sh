#!/bin/bash
# checks/coverage/report.sh
#
# Generate detailed coverage reports including HTML and per-file breakdown.
# Useful for identifying modules that need more test coverage.

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo "error: cargo-llvm-cov not installed"
    echo "Install with: cargo install cargo-llvm-cov"
    exit 1
fi

# Generate HTML report
echo "Generating HTML coverage report..."
cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    --html --output-dir target/coverage-html

# Generate per-file coverage summary
echo ""
echo "=== Per-File Coverage Summary ==="
echo ""

# Use text output and parse for file coverage
cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    2>&1 | grep -E '^\s+[0-9]+\.[0-9]+%|^[0-9]+\.[0-9]+%' | head -50 || true

# Generate module-level breakdown from JSON if available
if [ -f target/coverage.json ]; then
    echo ""
    echo "=== Module Coverage Breakdown ==="
    echo ""

    # Extract per-file coverage from JSON and group by module
    jq -r '.data[0].files[] | "\(.filename):\(.summary.lines.percent // 0)%"' target/coverage.json 2>/dev/null \
        | grep -v '_tests\.rs' \
        | sort -t':' -k2 -n \
        | head -30 || true
fi

echo ""
echo "HTML report: target/coverage-html/index.html"
echo ""
echo "To view: open target/coverage-html/index.html"

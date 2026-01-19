#!/bin/bash
# checks/coverage/measure.sh
#
# Measure code coverage using cargo-llvm-cov.
# Outputs coverage percentage and detailed JSON report.

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo "error: cargo-llvm-cov not installed"
    echo "Install with: cargo install cargo-llvm-cov"
    exit 1
fi

# Create target directory if needed
mkdir -p target

# Generate coverage with JSON output
cargo llvm-cov --all-features --workspace \
    --ignore-filename-regex='_tests\.rs$' \
    --json --output-path target/coverage.json

# Extract and display coverage percentage
COVERAGE=$(cat target/coverage.json | jq -r '.data[0].totals.lines.percent // 0')
echo "$COVERAGE" > target/coverage-percent.txt

echo "Coverage: ${COVERAGE}%"

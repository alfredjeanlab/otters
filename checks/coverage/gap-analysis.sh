#!/bin/bash
# checks/coverage/gap-analysis.sh
#
# Analyze coverage gaps and produce a prioritized list of modules needing tests.
# Uses the JSON coverage report to identify:
# - Files below 80% coverage
# - Uncovered functions
# - Suggested test priorities

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

COVERAGE_FILE="target/coverage.json"
THRESHOLD="${1:-80}"

# Ensure coverage data exists
if [ ! -f "$COVERAGE_FILE" ]; then
    echo "No coverage data found. Running measurement first..."
    ./checks/coverage/measure.sh
fi

echo "=== Coverage Gap Analysis (threshold: ${THRESHOLD}%) ==="
echo ""

# Files below threshold (excluding test files)
echo "=== Files Below ${THRESHOLD}% Coverage ==="
echo ""

jq -r --argjson threshold "$THRESHOLD" '
    .data[0].files[]
    | select(.filename | test("_tests\\.rs$") | not)
    | select(.summary.lines.percent < $threshold)
    | "\(.summary.lines.percent | . * 100 | floor / 100)%\t\(.filename)"
' "$COVERAGE_FILE" 2>/dev/null | sort -n | column -t || echo "No files below threshold"

echo ""
echo "=== Priority Modules for Testing ==="
echo ""

# Priority based on PLAN.md categories:
# High: scheduling/, coordination/, engine/runtime.rs
# Medium: storage/wal/, runbook/

jq -r '
    .data[0].files[]
    | select(.filename | test("_tests\\.rs$") | not)
    | {
        file: .filename,
        coverage: .summary.lines.percent,
        priority: (
            if .filename | test("scheduling/|coordination/|engine/runtime") then "HIGH"
            elif .filename | test("storage/wal|runbook/") then "MEDIUM"
            else "LOW"
            end
        )
    }
    | select(.coverage < 90)
    | "\(.priority)\t\(.coverage | . * 100 | floor / 100)%\t\(.file)"
' "$COVERAGE_FILE" 2>/dev/null | sort -k1,1 -k2,2n | column -t || echo "No gaps found"

echo ""
echo "=== Coverage Categories ==="
echo ""

# Categorize uncovered code
jq -r '
    .data[0].files[]
    | select(.filename | test("_tests\\.rs$") | not)
    | select(.summary.lines.percent < 100)
    | .filename
' "$COVERAGE_FILE" 2>/dev/null | while read -r file; do
    # Determine category based on patterns
    if echo "$file" | grep -q "error"; then
        echo "Error paths: $file"
    elif echo "$file" | grep -q "recovery"; then
        echo "Recovery: $file"
    fi
done | sort -u | head -20 || true

echo ""
echo "=== Summary ==="
echo ""

TOTAL_FILES=$(jq '.data[0].files | length' "$COVERAGE_FILE" 2>/dev/null || echo "0")
LOW_COV_FILES=$(jq --argjson threshold "$THRESHOLD" '[.data[0].files[] | select(.filename | test("_tests\\.rs$") | not) | select(.summary.lines.percent < $threshold)] | length' "$COVERAGE_FILE" 2>/dev/null || echo "0")
OVERALL=$(cat target/coverage-percent.txt 2>/dev/null || echo "unknown")

echo "Total source files: $TOTAL_FILES"
echo "Files below ${THRESHOLD}%: $LOW_COV_FILES"
echo "Overall coverage: ${OVERALL}%"

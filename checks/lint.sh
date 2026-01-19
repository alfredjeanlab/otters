#!/usr/bin/env bash
# checks/lint.sh - Unified lint enforcement for otters
#
# Usage: ./checks/lint.sh [--fix]
#
# Runs all lint checks. With --fix, attempts to auto-fix issues.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

failed=0

echo "Running lint checks..."
echo ""

# 1. Format check
echo "Checking formatting..."
if $FIX_MODE; then
    cargo fmt --all
    echo -e "${GREEN}Formatted${NC}"
else
    if ! cargo fmt --all -- --check; then
        echo -e "${RED}Format check failed. Run './checks/lint.sh --fix'${NC}"
        failed=1
    else
        echo -e "${GREEN}Format OK${NC}"
    fi
fi

# 2. Clippy
echo ""
echo "Running clippy..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo -e "${RED}Clippy found issues${NC}"
    failed=1
else
    echo -e "${GREEN}Clippy OK${NC}"
fi

# 3. Dead code check (custom)
echo ""
echo "Checking for unauthorized dead code..."
DEAD_CODE=$(grep -r '#\[allow(dead_code)\]' crates/ --include="*.rs" | grep -v "// JUSTIFIED:" || true)
if [[ -n "$DEAD_CODE" ]]; then
    echo -e "${YELLOW}Found #[allow(dead_code)] without justification:${NC}"
    echo "$DEAD_CODE"
    echo -e "${YELLOW}  Add '// JUSTIFIED: <reason>' comment to suppress${NC}"
    failed=1
else
    echo -e "${GREEN}No unauthorized dead code${NC}"
fi

# 4. Unsafe check
echo ""
echo "Checking unsafe blocks..."
UNSAFE=$(grep -rn 'unsafe {' crates/ --include="*.rs" | grep -v "// SAFETY:" || true)
if [[ -n "$UNSAFE" ]]; then
    echo -e "${YELLOW}Found unsafe blocks without SAFETY comment:${NC}"
    echo "$UNSAFE"
    failed=1
else
    echo -e "${GREEN}All unsafe blocks documented${NC}"
fi

# 5. Unwrap check (outside tests)
echo ""
echo "Checking unwrap() usage..."
# Find unwrap/expect outside test files and test modules
UNWRAP=$(grep -rn '\.unwrap()' crates/ --include="*.rs" | \
    grep -v '_tests\.rs' | \
    grep -v '#\[cfg(test)\]' | \
    grep -v '// OK:' | \
    grep -v 'test' || true)
if [[ -n "$UNWRAP" ]]; then
    echo -e "${YELLOW}Found unwrap() in non-test code without '// OK:' comment:${NC}"
    echo "$UNWRAP" | head -10
    if [[ $(echo "$UNWRAP" | wc -l) -gt 10 ]]; then
        echo "  ... and more"
    fi
    # This is a warning, not a failure (for now)
fi
echo -e "${GREEN}Unwrap check complete${NC}"

# 6. Test file check
echo ""
echo "Verifying test file conventions..."
# Check that all *_tests.rs files are imported via #[cfg(test)]
for test_file in $(find crates/ -name "*_tests.rs"); do
    module_name=$(basename "$test_file" .rs)
    parent_dir=$(dirname "$test_file")
    mod_file="$parent_dir/mod.rs"

    if [[ -f "$mod_file" ]]; then
        if ! grep -q "#\[cfg(test)\]" "$mod_file" || ! grep -q "mod $module_name" "$mod_file"; then
            echo -e "${YELLOW}$test_file may not be imported correctly${NC}"
        fi
    fi
done
echo -e "${GREEN}Test conventions OK${NC}"

# Summary
echo ""
echo "--------------------------------------"
if [[ $failed -eq 0 ]]; then
    echo -e "${GREEN}All lint checks passed${NC}"
    exit 0
else
    echo -e "${RED}Some lint checks failed${NC}"
    exit 1
fi

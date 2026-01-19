#!/bin/bash
# checks/quality/compare.sh
#
# Compares current metrics against baseline, highlights regressions

set -euo pipefail

BASELINE=${1:-}
CURRENT=${2:-}

if [ -z "$BASELINE" ] || [ -z "$CURRENT" ]; then
    echo "Usage: compare.sh <baseline.json> <current.json>"
    exit 1
fi

if [ ! -f "$BASELINE" ]; then
    echo "No baseline found, skipping comparison"
    exit 0
fi

if [ ! -f "$CURRENT" ]; then
    echo "No current metrics found"
    exit 1
fi

echo "=== Quality Comparison ==="
echo ""

# Compare file count over limit
baseline_over=$(jq '[.file_stats[].src.over_limit, .file_stats[].tests.over_limit] | add // 0' "$BASELINE")
current_over=$(jq '[.file_stats[].src.over_limit, .file_stats[].tests.over_limit] | add // 0' "$CURRENT")

if [ "${current_over:-0}" -gt "${baseline_over:-0}" ]; then
    echo "REGRESSION: Files over size limit increased from $baseline_over to $current_over"
    exit 1
fi

# Compare escape hatches (src is more critical than tests)
for type in src tests; do
    for hatch in unsafe unwrap expect allow_dead_code; do
        baseline_count=$(jq ".escape_hatches.$type.$hatch // 0" "$BASELINE")
        current_count=$(jq ".escape_hatches.$type.$hatch // 0" "$CURRENT")

        if [ "${current_count:-0}" -gt "${baseline_count:-0}" ]; then
            echo "WARNING: $type $hatch count increased from $baseline_count to $current_count"
        fi
    done
done

# Compare test count (should not decrease)
baseline_tests=$(jq ".tests.count // 0" "$BASELINE")
current_tests=$(jq ".tests.count // 0" "$CURRENT")

if [ "${current_tests:-0}" -lt "${baseline_tests:-0}" ]; then
    echo "WARNING: Test count decreased from $baseline_tests to $current_tests"
fi

echo ""
echo "Comparison complete. No blocking regressions found."

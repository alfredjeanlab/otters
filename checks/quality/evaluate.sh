#!/bin/bash
# checks/quality/evaluate.sh
#
# Produces JSON metrics for code quality tracking

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Coverage collection using cargo llvm-cov
# Always uses debug build to ensure accurate coverage
get_coverage() {
    # Check if llvm-cov is available
    if ! cargo llvm-cov --version &>/dev/null; then
        echo "{\"available\": false, \"line_percent\": 0, \"by_crate\": {}}" >&2
        echo '{"available": false, "line_percent": 0, "by_crate": {}}'
        return
    fi

    # Clean workspace to avoid stale data
    cargo llvm-cov clean --workspace &>/dev/null || true

    # Run coverage and capture JSON output
    # Note: cargo llvm-cov always uses debug builds for accurate coverage
    local json_output
    json_output=$(cargo llvm-cov --workspace --json 2>/dev/null) || {
        echo '{"available": false, "line_percent": 0, "by_crate": {}}'
        return
    }

    # Extract overall coverage percentage from the JSON
    local total_lines total_covered line_percent
    total_lines=$(echo "$json_output" | jq -r '.data[0].totals.lines.count // 0')
    total_covered=$(echo "$json_output" | jq -r '.data[0].totals.lines.covered // 0')

    if [ "$total_lines" -gt 0 ]; then
        line_percent=$(echo "scale=2; $total_covered * 100 / $total_lines" | bc)
    else
        line_percent=0
    fi

    # Extract per-crate coverage
    # The JSON has coverage data per file, we aggregate by crate
    local core_lines core_covered core_percent
    local cli_lines cli_covered cli_percent

    core_lines=$(echo "$json_output" | jq '[.data[0].files[] | select(.filename | contains("crates/core/")) | .summary.lines.count] | add // 0')
    core_covered=$(echo "$json_output" | jq '[.data[0].files[] | select(.filename | contains("crates/core/")) | .summary.lines.covered] | add // 0')
    if [ "$core_lines" -gt 0 ]; then
        core_percent=$(echo "scale=2; $core_covered * 100 / $core_lines" | bc)
    else
        core_percent=0
    fi

    cli_lines=$(echo "$json_output" | jq '[.data[0].files[] | select(.filename | contains("crates/cli/")) | .summary.lines.count] | add // 0')
    cli_covered=$(echo "$json_output" | jq '[.data[0].files[] | select(.filename | contains("crates/cli/")) | .summary.lines.covered] | add // 0')
    if [ "$cli_lines" -gt 0 ]; then
        cli_percent=$(echo "scale=2; $cli_covered * 100 / $cli_lines" | bc)
    else
        cli_percent=0
    fi

    cat << COVERAGE_EOF
{
    "available": true,
    "line_percent": $line_percent,
    "lines_total": $total_lines,
    "lines_covered": $total_covered,
    "by_crate": {
      "core": {"line_percent": $core_percent, "lines_total": $core_lines, "lines_covered": $core_covered},
      "cli": {"line_percent": $cli_percent, "lines_total": $cli_lines, "lines_covered": $cli_covered}
    }
  }
COVERAGE_EOF
}

# Collect LOC by crate
get_loc_by_crate() {
    local crate=$1
    # Source: all .rs in src/ excluding *_tests.rs
    local src_loc=$(find "crates/$crate/src" -name "*.rs" ! -name "*_tests.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    # Tests: *_tests.rs in src/ plus all .rs in tests/
    local test_loc_src=$(find "crates/$crate/src" -name "*_tests.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    local test_loc_dir=$(find "crates/$crate/tests" -name "*.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}' || echo 0)
    local test_loc=$((test_loc_src + test_loc_dir))
    echo "{\"source\": $src_loc, \"test\": $test_loc}"
}

# File size statistics
get_file_stats() {
    local crate=$1
    local type=$2  # "src" or "tests"

    local files=""
    local limit=700
    if [ "$type" = "src" ]; then
        # Source files: all .rs in src/ excluding *_tests.rs
        files=$(find "crates/$crate/src" -name "*.rs" ! -name "*_tests.rs" 2>/dev/null || true)
    else
        # Test files: *_tests.rs in src/ plus all .rs in tests/
        files=$(find "crates/$crate/src" -name "*_tests.rs" 2>/dev/null || true)
        local test_dir_files=$(find "crates/$crate/tests" -name "*.rs" 2>/dev/null || true)
        [ -n "$test_dir_files" ] && files="$files $test_dir_files"
        limit=1100
    fi

    if [ -z "$files" ]; then
        echo "{\"count\": 0, \"avg\": 0, \"max\": 0, \"over_limit\": 0}"
        return
    fi

    local count=0
    local total=0
    local max=0
    local over_limit=0

    for f in $files; do
        local lines=$(wc -l < "$f" | tr -d ' ')
        count=$((count + 1))
        total=$((total + lines))
        [ "$lines" -gt "$max" ] && max=$lines
        [ "$lines" -gt "$limit" ] && over_limit=$((over_limit + 1))
    done

    local avg=$((total / count))
    echo "{\"count\": $count, \"avg\": $avg, \"max\": $max, \"over_limit\": $over_limit}"
}

# Escape hatch counts - separated by src vs tests
count_escape_hatches() {
    local pattern=$1
    local type=$2  # "src" or "tests"

    local count=0
    for crate in core cli; do
        if [ "$type" = "src" ]; then
            # Source files: all .rs in src/ excluding *_tests.rs
            local matches=$(grep -r "$pattern" "crates/$crate/src" --include="*.rs" 2>/dev/null | grep -v "_tests\.rs:" | wc -l | tr -d ' ')
        else
            # Test files: *_tests.rs in src/ plus all .rs in tests/
            local src_tests=$(grep -r "$pattern" "crates/$crate/src" --include="*_tests.rs" 2>/dev/null | wc -l | tr -d ' ')
            local dir_tests=$(grep -r "$pattern" "crates/$crate/tests" --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
            local matches=$((src_tests + dir_tests))
        fi
        count=$((count + matches))
    done
    echo "$count"
}

# Test counts
count_tests() {
    grep -r "#\[test\]" crates/core crates/cli --include="*.rs" 2>/dev/null | wc -l | tr -d ' '
}

# Build the JSON output
cat << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_sha": "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')",
  "loc": {
    "core": $(get_loc_by_crate core),
    "cli": $(get_loc_by_crate cli)
  },
  "file_stats": {
    "core": {
      "src": $(get_file_stats core src),
      "tests": $(get_file_stats core tests)
    },
    "cli": {
      "src": $(get_file_stats cli src),
      "tests": $(get_file_stats cli tests)
    }
  },
  "escape_hatches": {
    "src": {
      "unsafe": $(count_escape_hatches "unsafe " src),
      "unwrap": $(count_escape_hatches "\.unwrap()" src),
      "expect": $(count_escape_hatches "\.expect(" src),
      "allow_dead_code": $(count_escape_hatches "#\[allow(dead_code)\]" src)
    },
    "tests": {
      "unsafe": $(count_escape_hatches "unsafe " tests),
      "unwrap": $(count_escape_hatches "\.unwrap()" tests),
      "expect": $(count_escape_hatches "\.expect(" tests),
      "allow_dead_code": $(count_escape_hatches "#\[allow(dead_code)\]" tests)
    }
  },
  "tests": {
    "count": $(count_tests)
  },
  "coverage": $(get_coverage)
}
EOF

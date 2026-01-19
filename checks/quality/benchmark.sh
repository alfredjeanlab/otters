#!/bin/bash
# checks/quality/benchmark.sh
#
# Measures compile time, test time, binary size, and basic performance

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Cold compile time (from clean)
measure_cold_compile() {
    cargo clean 2>/dev/null
    local start=$(date +%s.%N)
    cargo build --release 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Incremental compile time (touch and rebuild)
measure_incremental_compile() {
    touch crates/cli/src/main.rs
    local start=$(date +%s.%N)
    cargo build --release 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Cold test time
measure_cold_test() {
    cargo clean 2>/dev/null
    local start=$(date +%s.%N)
    cargo test --all 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Warm test time
measure_warm_test() {
    local start=$(date +%s.%N)
    cargo test --all 2>/dev/null
    local end=$(date +%s.%N)
    echo "scale=2; $end - $start" | bc
}

# Binary sizes
measure_binary_sizes() {
    cargo build --release 2>/dev/null
    local oj_size=$(stat -f%z target/release/oj 2>/dev/null || stat -c%s target/release/oj 2>/dev/null || echo 0)

    # Stripped sizes
    cp target/release/oj target/release/oj-stripped 2>/dev/null || true
    strip target/release/oj-stripped 2>/dev/null || true
    local oj_stripped=$(stat -f%z target/release/oj-stripped 2>/dev/null || stat -c%s target/release/oj-stripped 2>/dev/null || echo 0)

    echo "{\"oj\": {\"release\": $oj_size, \"stripped\": $oj_stripped}}"
}

echo "Running benchmarks (this may take several minutes)..." >&2

cold_compile=$(measure_cold_compile)
echo "Cold compile: ${cold_compile}s" >&2

incremental_compile=$(measure_incremental_compile)
echo "Incremental compile: ${incremental_compile}s" >&2

cold_test=$(measure_cold_test)
echo "Cold test: ${cold_test}s" >&2

warm_test=$(measure_warm_test)
echo "Warm test: ${warm_test}s" >&2

binary_sizes=$(measure_binary_sizes)
echo "Binary sizes collected" >&2

cat << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_sha": "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')",
  "compile_time": {
    "cold_seconds": $cold_compile,
    "incremental_seconds": $incremental_compile
  },
  "test_time": {
    "cold_seconds": $cold_test,
    "warm_seconds": $warm_test
  },
  "binary_size": $binary_sizes
}
EOF

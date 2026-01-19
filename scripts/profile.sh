#!/bin/bash
# scripts/profile.sh
#
# Performance profiling utilities for the oj CLI.
# Supports CPU profiling, memory profiling, and timing benchmarks.
#
# Usage:
#   ./scripts/profile.sh cpu     # Generate CPU flamegraph
#   ./scripts/profile.sh memory  # Generate heap profile
#   ./scripts/profile.sh time    # Benchmark common operations
#   ./scripts/profile.sh all     # Run all profiles

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Build release binary
build_release() {
    info "Building release binary..."
    cargo build --release --quiet
}

# CPU profiling with flamegraph
profile_cpu() {
    info "Running CPU profiling..."

    if ! command -v flamegraph &> /dev/null; then
        warn "flamegraph not found. Install with: cargo install flamegraph"
        warn "Skipping CPU profiling"
        return 1
    fi

    build_release

    info "Generating flamegraph..."
    # Note: flamegraph requires dtrace/perf permissions
    flamegraph --bin oj -- pipeline list 2>/dev/null || {
        warn "flamegraph failed (may need sudo/dtrace permissions)"
        warn "On macOS, run: sudo flamegraph --bin oj -- pipeline list"
        return 1
    }

    info "Flamegraph saved to flamegraph.svg"
}

# Memory profiling
profile_memory() {
    info "Running memory profiling..."

    if ! command -v heaptrack &> /dev/null; then
        warn "heaptrack not found. Install with your package manager."
        warn "On macOS: brew install heaptrack"
        warn "Skipping memory profiling"
        return 1
    fi

    build_release

    info "Running heaptrack..."
    heaptrack ./target/release/oj pipeline list

    info "Analyze with: heaptrack --analyze heaptrack.oj.*.gz"
}

# Time benchmarks using hyperfine
profile_time() {
    info "Running timing benchmarks..."

    build_release

    if command -v hyperfine &> /dev/null; then
        info "Using hyperfine for benchmarks..."
        hyperfine --warmup 3 --min-runs 10 \
            './target/release/oj pipeline list' \
            './target/release/oj workspace list' \
            './target/release/oj --help' \
            2>&1 | tee target/benchmark-results.txt
    else
        warn "hyperfine not found. Using basic timing."
        warn "Install hyperfine for better benchmarks: cargo install hyperfine"

        echo ""
        echo "=== Basic Timing Results ==="
        echo ""

        echo "oj pipeline list:"
        time ./target/release/oj pipeline list 2>/dev/null || true
        echo ""

        echo "oj workspace list:"
        time ./target/release/oj workspace list 2>/dev/null || true
        echo ""

        echo "oj --help:"
        time ./target/release/oj --help > /dev/null
        echo ""
    fi
}

# Cold start benchmark
profile_cold_start() {
    info "Measuring cold start time..."

    build_release

    # Clear any caches
    rm -rf target/cold-start-test 2>/dev/null || true
    mkdir -p target/cold-start-test

    # Measure startup with empty state
    echo "Cold start (first run):"
    time (cd target/cold-start-test && ../release/oj --help > /dev/null 2>&1)

    # Measure with existing state
    echo ""
    echo "Warm start (with state):"
    time (cd target/cold-start-test && ../release/oj --help > /dev/null 2>&1)
}

# All profiles
profile_all() {
    profile_time
    profile_cold_start
    profile_cpu || true
    profile_memory || true
}

# Show help
show_help() {
    echo "Performance profiling utilities for oj"
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  cpu         Generate CPU flamegraph (requires flamegraph)"
    echo "  memory      Generate heap profile (requires heaptrack)"
    echo "  time        Benchmark common operations"
    echo "  cold        Measure cold start time"
    echo "  all         Run all profiles"
    echo "  help        Show this help"
    echo ""
    echo "Prerequisites:"
    echo "  cargo install flamegraph hyperfine"
    echo "  brew install heaptrack  # macOS"
}

# Main
case "${1:-help}" in
    cpu)
        profile_cpu
        ;;
    memory)
        profile_memory
        ;;
    time)
        profile_time
        ;;
    cold)
        profile_cold_start
        ;;
    all)
        profile_all
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        error "Unknown command: $1"
        show_help
        exit 1
        ;;
esac

#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC

# scripts/smoke-test.sh - E2E smoke test for oj
#
# Usage:
#   ./scripts/smoke-test.sh --model simulated  # CI mode with claudeless
#   ./scripts/smoke-test.sh --model haiku      # Manual validation with real Claude

set -euo pipefail

MODEL="${1:---model}"
MODEL_VALUE="${2:-simulated}"

if [[ "$MODEL" != "--model" ]]; then
    echo "Usage: $0 --model [simulated|haiku]"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TEMP_DIR="$(mktemp -d)"

cleanup() {
    echo "Cleaning up..."
    # Kill any tmux sessions we created
    tmux list-sessions 2>/dev/null | grep "^oj-smoke" | cut -d: -f1 | xargs -I{} tmux kill-session -t {} 2>/dev/null || true
    rm -rf "$TEMP_DIR"
    echo "Cleanup complete"
}
trap cleanup EXIT

echo "========================================"
echo "OJ Smoke Test"
echo "========================================"
echo "Model: $MODEL_VALUE"
echo "Temp dir: $TEMP_DIR"
echo ""

cd "$TEMP_DIR"

# Initialize git repo
git init --quiet
git config user.email "test@test.com"
git config user.name "Test"
echo "# Test" > README.md
git add README.md
git commit -m "Initial commit" --quiet

mkdir -p .build/operations

# Setup PATH for simulated mode
if [[ "$MODEL_VALUE" == "simulated" ]]; then
    # Verify claudeless is available
    if ! command -v claudeless &>/dev/null; then
        echo "ERROR: claudeless not found in PATH. Install claudeless globally first."
        exit 1
    fi
    # Create a temp directory with a claude -> claudeless symlink
    CLAUDE_BIN_DIR="$TEMP_DIR/bin"
    mkdir -p "$CLAUDE_BIN_DIR"
    ln -sf "$(which claudeless)" "$CLAUDE_BIN_DIR/claude"
    export PATH="$CLAUDE_BIN_DIR:$PROJECT_ROOT/target/debug:$PATH"
    export CLAUDE_SIM_RESPONSE="Task completed successfully"
    echo "Using claudeless from: $(which claudeless)"
fi

# Build the project if needed
if [[ ! -f "$PROJECT_ROOT/target/debug/oj" ]]; then
    echo "Building oj..."
    cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml"
fi

OJ="$PROJECT_ROOT/target/debug/oj"

echo ""
echo "--- Test 1: Create build pipeline ---"
$OJ run build smoke-test "Create a hello world program"
echo "PASS: Pipeline created"

echo ""
echo "--- Test 2: Verify pipeline state ---"
if [[ -f ".build/operations/pipelines/build-smoke-test/state.json" ]]; then
    echo "PASS: Pipeline state file exists"
else
    echo "FAIL: Pipeline state file missing"
    exit 1
fi

echo ""
echo "--- Test 3: Verify workspace created ---"
if [[ -d ".worktrees/build-smoke-test" ]]; then
    echo "PASS: Workspace directory exists"
else
    echo "FAIL: Workspace directory missing"
    exit 1
fi

echo ""
echo "--- Test 4: Verify CLAUDE.md generated ---"
if [[ -f ".worktrees/build-smoke-test/CLAUDE.md" ]]; then
    echo "PASS: CLAUDE.md exists"
    if grep -q "hello world" ".worktrees/build-smoke-test/CLAUDE.md"; then
        echo "PASS: CLAUDE.md contains prompt"
    else
        echo "FAIL: CLAUDE.md missing prompt content"
        exit 1
    fi
else
    echo "FAIL: CLAUDE.md missing"
    exit 1
fi

echo ""
echo "--- Test 5: Signal phase completion ---"
cd ".worktrees/build-smoke-test"
export OTTER_PIPELINE="build-smoke-test"
$OJ done
echo "PASS: Phase completion signaled"
cd "$TEMP_DIR"

echo ""
echo "--- Test 6: Run daemon iteration ---"
$OJ daemon --once
echo "PASS: Daemon iteration completed"

echo ""
echo "--- Test 7: List pipelines ---"
OUTPUT=$($OJ pipeline list)
if echo "$OUTPUT" | grep -q "smoke-test"; then
    echo "PASS: Pipeline appears in list"
else
    echo "FAIL: Pipeline not in list"
    exit 1
fi

if [[ "$MODEL_VALUE" == "haiku" ]]; then
    echo ""
    echo "--- Test 8 (haiku only): Verify real Claude responded ---"
    # Check tmux session has output
    if tmux capture-pane -t oj-build-smoke-test-init -p 2>/dev/null | grep -q "Claude"; then
        echo "PASS: Claude responded in session"
    else
        echo "WARN: Could not verify Claude response (session may have ended)"
    fi
fi

echo ""
echo "========================================"
echo "ALL TESTS PASSED"
echo "========================================"

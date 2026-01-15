#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Shared test utilities for oj specs

# Compute paths relative to this file
HELPERS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$(dirname "$HELPERS_DIR")"
PROJECT_ROOT="$(cd "$SPECS_DIR/../.." && pwd)"

# Load bats libraries
load "$SPECS_DIR/bats/bats-support/load"
load "$SPECS_DIR/bats/bats-assert/load"

# Build binaries (don't auto-detect - just build them)
cargo build -p oj --manifest-path "$PROJECT_ROOT/Cargo.toml" --quiet 2>/dev/null || true
export OJ_BIN="$PROJECT_ROOT/target/debug/oj"

# Setup claudeless (globally installed)
if command -v claudeless &>/dev/null; then
    export CLAUDELESS_BIN="$(which claudeless)"
    # Create a temp directory with claude -> claudeless symlink for tests
    CLAUDE_SYMLINK_DIR="$(mktemp -d)"
    ln -sf "$CLAUDELESS_BIN" "$CLAUDE_SYMLINK_DIR/claude"
    export PATH="$CLAUDE_SYMLINK_DIR:$PATH"
fi

# File-level setup: init git, add binaries to PATH
# Note: Uses BATS_FILE_TMPDIR for test isolation (automatically provided by bats)
file_setup() {
    # Add project binaries to PATH
    export PATH="$PROJECT_ROOT/target/debug:$PATH"
}

# File-level teardown: kill oj-* tmux sessions
file_teardown() {
    # Kill any oj-* tmux sessions created during tests
    if command -v tmux &>/dev/null; then
        local sessions
        sessions=$(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep '^oj-' || true)
        for session in $sessions; do
            tmux kill-session -t "$session" 2>/dev/null || true
        done
    fi
    # Note: BATS_FILE_TMPDIR is automatically cleaned up by bats
}

# Per-test setup: reset to temp dir
test_setup() {
    cd "$BATS_FILE_TMPDIR" || return 1
}

# Initialize git repo (idempotent)
init_git_repo() {
    local dir="${1:-.}"
    if [[ ! -d "$dir/.git" ]]; then
        git -C "$dir" init -q
        git -C "$dir" config user.email "test@example.com"
        git -C "$dir" config user.name "Test"
        # Create initial commit
        touch "$dir/.gitkeep"
        git -C "$dir" add .gitkeep
        git -C "$dir" commit -q -m "Initial commit"
    fi
}

# Initialize oj project structure (git + .build/operations)
init_oj_project() {
    local dir="${1:-$BATS_FILE_TMPDIR}"
    init_git_repo "$dir"
    mkdir -p "$dir/.build/operations/pipelines"
    mkdir -p "$dir/.build/operations/workspaces"
    mkdir -p "$dir/.build/operations/queues"
}

# Create claudeless scenario file
# Usage: path=$(create_scenario "name" "content")
create_scenario() {
    local name="$1"
    local content="$2"
    local scenario_file="$BATS_FILE_TMPDIR/scenarios/$name.toml"

    mkdir -p "$(dirname "$scenario_file")"
    echo "$content" > "$scenario_file"
    echo "$scenario_file"
}

# Create auto-done scenario (calls oj done immediately)
create_auto_done_scenario() {
    create_scenario "auto-done" '
name = "auto-done"
description = "Immediately signals done"

[[steps]]
type = "bash"
command = "oj done"
'
}

# Assert file exists
assert_file_exists() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        fail "Expected file to exist: $file"
    fi
}

# Assert directory exists
assert_dir_exists() {
    local dir="$1"
    if [[ ! -d "$dir" ]]; then
        fail "Expected directory to exist: $dir"
    fi
}

# Assert JSON field equals value (requires jq)
assert_json_field() {
    local json="$1"
    local field="$2"
    local expected="$3"

    if ! command -v jq &>/dev/null; then
        skip "jq not installed"
    fi

    local actual
    actual=$(echo "$json" | jq -r "$field")
    if [[ "$actual" != "$expected" ]]; then
        fail "Expected $field to be '$expected', got '$actual'"
    fi
}

# Wait for a condition with timeout
# Usage: wait_for <timeout_secs> <command>
wait_for() {
    local timeout="$1"
    shift
    local cmd="$*"
    local elapsed=0

    while ! eval "$cmd" 2>/dev/null; do
        sleep 0.5
        elapsed=$((elapsed + 1))
        if [[ $elapsed -ge $((timeout * 2)) ]]; then
            return 1
        fi
    done
    return 0
}

# Default setup/teardown hooks
setup_file() {
    file_setup
    init_oj_project
}

teardown_file() {
    file_teardown
}

setup() {
    test_setup
}

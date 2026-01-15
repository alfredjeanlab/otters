#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for daemon lifecycle

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

# Basic operation
@test "daemon --once completes successfully" {
    run "$OJ_BIN" daemon --once
    assert_success
}

@test "daemon --once with no pipelines exits cleanly" {
    # Ensure no pipelines
    rm -rf "$BATS_FILE_TMPDIR/.build/operations/pipelines"/*

    run "$OJ_BIN" daemon --once
    assert_success
}

@test "daemon --once with pipeline processes it" {
    skip "requires valid pipeline state file schema"
}

# Logging
@test "daemon logs startup message" {
    # Daemon outputs "Starting oj daemon" on startup
    run "$OJ_BIN" daemon --once 2>&1
    # Success depends on having valid state files
    assert_output --partial "Starting oj daemon"
}

@test "daemon logs interval configuration" {
    skip "requires verbose logging inspection"
}

# Signal handling
@test "daemon responds to SIGINT" {
    # Start daemon in background
    "$OJ_BIN" daemon &
    local pid=$!

    # Give it time to start
    sleep 0.5

    # Send SIGINT
    kill -INT $pid 2>/dev/null || true

    # Wait for exit
    wait $pid 2>/dev/null || true

    # Process should have exited
    ! kill -0 $pid 2>/dev/null
}

@test "daemon responds to SIGTERM" {
    # Start daemon in background
    "$OJ_BIN" daemon &
    local pid=$!

    # Give it time to start
    sleep 0.5

    # Send SIGTERM
    kill -TERM $pid 2>/dev/null || true

    # Wait for exit
    wait $pid 2>/dev/null || true

    # Process should have exited
    ! kill -0 $pid 2>/dev/null
}

# Custom intervals
@test "daemon accepts --poll-interval" {
    # Verify the flag is accepted by checking output contains the configured interval
    run "$OJ_BIN" daemon --once --poll-interval 1000 2>&1
    assert_output --partial "Poll interval: 1000s"
}

@test "daemon accepts --tick-interval" {
    # Verify the flag is accepted by checking output contains the configured interval
    run "$OJ_BIN" daemon --once --tick-interval 500 2>&1
    assert_output --partial "Tick interval: 500s"
}

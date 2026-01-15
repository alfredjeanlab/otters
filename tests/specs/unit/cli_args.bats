#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for CLI argument validation

load '../helpers/common'

# Signal commands
@test "oj done without context fails gracefully" {
    run "$OJ_BIN" done
    assert_failure
}

@test "oj done shows helpful error message" {
    run "$OJ_BIN" done
    assert_failure
    # Should indicate missing context/environment
    assert_output --partial "OTTER" || assert_output --partial "context" || assert_output --partial "pipeline"
}

@test "oj checkpoint without context fails gracefully" {
    run "$OJ_BIN" checkpoint
    assert_failure
}

# Daemon commands
@test "oj daemon accepts --once flag" {
    run "$OJ_BIN" daemon --help
    assert_success
    assert_output --partial "once"
}

@test "oj daemon accepts --poll-interval" {
    run "$OJ_BIN" daemon --help
    assert_success
    assert_output --partial "poll-interval"
}

@test "oj daemon accepts --tick-interval" {
    run "$OJ_BIN" daemon --help
    assert_success
    assert_output --partial "tick-interval"
}

# List commands (durable)
@test "oj pipeline list works without pipelines" {
    run "$OJ_BIN" pipeline list
    assert_success
}

@test "oj workspace list works without workspaces" {
    run "$OJ_BIN" workspace list
    assert_success
}

@test "oj session list works without sessions" {
    run "$OJ_BIN" session list
    assert_success
}

#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for session failure scenarios

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

@test "oj session kill nonexistent session fails gracefully" {
    run "$OJ_BIN" session kill "oj-nonexistent-session-abc123"
    # CLI returns success but outputs "can't find session" message
    assert_success
    assert_output --partial "can't find session"
}

@test "oj session nudge nonexistent session fails gracefully" {
    run "$OJ_BIN" session nudge "oj-nonexistent-session-xyz789"
    # CLI returns success but outputs "can't find pane" message
    assert_success
    assert_output --partial "can't find pane"
}

@test "daemon detects manually killed session" {
    skip "requires daemon session monitoring"
}

@test "daemon handles session that exits immediately" {
    skip "requires daemon session monitoring"
}

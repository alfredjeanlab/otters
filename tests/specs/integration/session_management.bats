#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for session management

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

# Session listing
@test "oj session list shows active sessions" {
    # Create a test tmux session with oj- prefix
    tmux new-session -d -s "oj-test-session-list" "sleep 30" 2>/dev/null || skip "tmux not available"

    run "$OJ_BIN" session list
    assert_success
    assert_output --partial "oj-test-session-list"

    # Cleanup
    tmux kill-session -t "oj-test-session-list" 2>/dev/null || true
}

@test "oj session list empty when no sessions" {
    # Kill any existing oj- sessions
    for session in $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep '^oj-' || true); do
        tmux kill-session -t "$session" 2>/dev/null || true
    done

    run "$OJ_BIN" session list
    assert_success
    # Should not fail, may output empty or "no sessions"
}

@test "oj session list shows multiple sessions" {
    tmux new-session -d -s "oj-multi-a" "sleep 30" 2>/dev/null || skip "tmux not available"
    tmux new-session -d -s "oj-multi-b" "sleep 30" 2>/dev/null || skip "tmux not available"

    run "$OJ_BIN" session list
    assert_success
    assert_output --partial "oj-multi-a"
    assert_output --partial "oj-multi-b"

    # Cleanup
    tmux kill-session -t "oj-multi-a" 2>/dev/null || true
    tmux kill-session -t "oj-multi-b" 2>/dev/null || true
}

# Session inspection
@test "oj session show displays session details" {
    tmux new-session -d -s "oj-show-test" "sleep 30" 2>/dev/null || skip "tmux not available"

    run "$OJ_BIN" session show "oj-show-test"
    assert_success

    # Cleanup
    tmux kill-session -t "oj-show-test" 2>/dev/null || true
}

@test "oj session show nonexistent fails gracefully" {
    run "$OJ_BIN" session show "oj-nonexistent-session-12345"
    assert_failure
}

# Session control
@test "oj session kill terminates session" {
    tmux new-session -d -s "oj-kill-test" "sleep 30" 2>/dev/null || skip "tmux not available"

    # Verify session exists
    run tmux has-session -t "oj-kill-test"
    assert_success

    # Kill via oj
    run "$OJ_BIN" session kill "oj-kill-test"
    assert_success

    # Verify session is gone
    run tmux has-session -t "oj-kill-test"
    assert_failure
}

@test "oj session nudge sends input to session" {
    tmux new-session -d -s "oj-nudge-test" "sleep 30" 2>/dev/null || skip "tmux not available"

    run "$OJ_BIN" session nudge "oj-nudge-test"
    # Should succeed or indicate no input needed
    [[ $status -eq 0 ]] || [[ $output =~ "nudge" ]]

    # Cleanup
    tmux kill-session -t "oj-nudge-test" 2>/dev/null || true
}

# Session naming
@test "session naming follows oj-{type}-{name}-{phase} pattern" {
    # This is a documentation/convention test
    # Session names should match: oj-build-myfeature-implement
    local pattern="^oj-[a-z]+-[a-z0-9_-]+-[a-z]+$"

    # Valid examples
    [[ "oj-build-myfeature-implement" =~ $pattern ]]
    [[ "oj-bugfix-issue123-review" =~ $pattern ]]

    # Invalid examples should not match
    ! [[ "myfeature" =~ $pattern ]]
    ! [[ "oj-myfeature" =~ $pattern ]]
}

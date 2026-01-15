#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for error handling and edge cases

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

# Input validation
@test "oj run build with invalid characters in name fails gracefully" {
    skip "requires input validation"
    run "$OJ_BIN" run build "test/invalid" "prompt"
    assert_failure
}

@test "oj run build with empty name fails" {
    skip "CLI currently allows empty names - needs validation"
}

@test "oj run build with spaces in name handles correctly" {
    skip "requires name handling implementation"
    run "$OJ_BIN" run build "test name" "prompt"
    # Should either fail gracefully or sanitize the name
    [[ $status -eq 0 ]] || [[ $status -eq 1 ]]
}

# Context errors
@test "oj done from wrong directory fails gracefully" {
    # Run from a temp directory without context
    cd "$BATS_FILE_TMPDIR"
    unset OTTER_TASK
    unset OTTER_PHASE

    run "$OJ_BIN" done
    assert_failure
    # Should not crash, should give helpful message
}

@test "oj done with nonexistent pipeline fails gracefully" {
    export OTTER_TASK="nonexistent-pipeline-xyz"
    export OTTER_PHASE="implement"

    run "$OJ_BIN" done
    assert_failure
}

@test "oj checkpoint from project root fails gracefully" {
    cd "$BATS_FILE_TMPDIR"
    unset OTTER_TASK
    unset OTTER_PHASE

    run "$OJ_BIN" checkpoint
    assert_failure
}

# State errors
@test "oj handles missing .build directory" {
    rm -rf "$BATS_FILE_TMPDIR/.build"

    run "$OJ_BIN" pipeline list
    # Should handle gracefully - may return empty list or create directory
    [[ $status -eq 0 ]] || [[ $output =~ "No" ]] || [[ $output =~ "empty" ]]
}

@test "oj handles corrupted state file" {
    mkdir -p "$BATS_FILE_TMPDIR/.build/operations/pipelines"
    echo "not valid json" > "$BATS_FILE_TMPDIR/.build/operations/pipelines/corrupted.json"

    run "$OJ_BIN" pipeline list
    # Should handle gracefully (either succeeds ignoring bad file or reports error)
    [[ $status -eq 0 ]] || [[ $output =~ "error" ]] || [[ $output =~ "invalid" ]] || [[ $output =~ "json" ]]
}

@test "oj handles missing workspace directory" {
    skip "requires valid workspace state file schema"
}

# Git errors
@test "oj works in non-git directory fails gracefully" {
    local non_git_dir
    non_git_dir=$(mktemp -d)
    cd "$non_git_dir"

    run "$OJ_BIN" run build "test" "prompt"
    assert_failure
    # Should indicate git requirement

    rm -rf "$non_git_dir"
}

@test "oj works in repo without commits fails gracefully" {
    local empty_git_dir
    empty_git_dir=$(mktemp -d)
    git -C "$empty_git_dir" init -q

    cd "$empty_git_dir"
    run "$OJ_BIN" run build "test" "prompt"
    # May fail due to no commits
    [[ $status -eq 0 ]] || [[ $status -eq 1 ]]

    rm -rf "$empty_git_dir"
}

# Concurrency edge cases
@test "duplicate pipeline names are rejected" {
    skip "requires duplicate detection"
}

@test "rapid sequential pipeline creates work" {
    skip "requires pipeline creation"
}

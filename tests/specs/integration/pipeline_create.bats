#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for pipeline creation

load '../helpers/common'

# Use longer timeout for integration tests
setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

# Core infrastructure (durable)
@test "oj run build creates workspace directory" {
    skip "requires claudeless setup"
    run "$OJ_BIN" run build test-workspace "test prompt"
    assert_success
    assert_dir_exists ".build/operations/workspaces/test-workspace"
}

@test "oj run build spawns tmux session" {
    skip "requires claudeless setup"
    run "$OJ_BIN" run build test-tmux "test prompt"
    assert_success
    # Check for tmux session
    run tmux has-session -t "oj-build-test-tmux-implement"
    assert_success
}

@test "oj run build creates pipeline state file" {
    skip "requires claudeless setup"
    run "$OJ_BIN" run build test-state "test prompt"
    assert_success
    assert_file_exists ".build/operations/pipelines/test-state.json"
}

@test "oj pipeline list shows created pipelines" {
    skip "requires valid pipeline state file schema"
}

@test "oj pipeline show displays pipeline details" {
    skip "requires valid pipeline state file schema"
}

@test "multiple pipelines can coexist" {
    skip "requires valid pipeline state file schema"
}

# Spot check hardcoded pipelines (minimal, Epic 6 replaces)
@test "oj run build works end-to-end" {
    skip "requires full claudeless integration"
}

@test "oj run bugfix works end-to-end" {
    skip "requires full claudeless integration"
}

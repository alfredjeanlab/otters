#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for workspace management

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

@test "oj workspace list shows workspaces" {
    skip "requires valid workspace state file schema"
}

@test "oj workspace list empty when no workspaces" {
    # Ensure no workspaces exist
    rm -rf "$BATS_FILE_TMPDIR/.build/operations/workspaces"/*

    run "$OJ_BIN" workspace list
    assert_success
    # Should not fail, may output empty or "no workspaces"
}

@test "oj workspace show displays workspace details" {
    skip "requires valid workspace state file schema"
}

@test "workspace is valid git worktree" {
    skip "requires full workspace creation"
}

@test "workspace contains CLAUDE.md" {
    skip "requires full workspace creation"
}

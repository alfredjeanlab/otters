#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for pipeline signaling (done/checkpoint)

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=10
    file_setup
    init_oj_project
}

@test "oj done with OTTER_TASK succeeds" {
    skip "requires valid pipeline state file schema"
}

@test "oj done autodetects pipeline from workspace" {
    skip "requires workspace detection implementation"
}

@test "oj done --error marks pipeline failed" {
    skip "requires valid pipeline state file schema"
}

@test "oj done --error records error message" {
    skip "requires state file inspection"
}

@test "oj checkpoint saves state" {
    skip "requires valid pipeline state file schema"
}

@test "oj checkpoint updates heartbeat" {
    skip "requires heartbeat inspection"
}

#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for claudeless integration

load '../helpers/common'

setup_file() {
    export BATS_TEST_TIMEOUT=15
    file_setup
    init_oj_project
    export CLAUDELESS_SCENARIO=$(create_auto_done_scenario)
}

@test "pipeline with claudeless scenario completes" {
    skip "requires claudeless binary and full integration"
}

@test "claudeless receives prompt from CLAUDE.md" {
    skip "requires claudeless binary and full integration"
}

@test "claudeless can signal oj done" {
    skip "requires claudeless binary and full integration"
}

@test "claudeless failure scenario triggers error state" {
    skip "requires claudeless binary and full integration"
}

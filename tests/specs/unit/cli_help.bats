#!/usr/bin/env bats
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Tests for CLI help and version output

load '../helpers/common'

@test "oj --help exits 0" {
    run "$OJ_BIN" --help
    assert_success
}

@test "oj --help shows Otter Jobs" {
    run "$OJ_BIN" --help
    assert_success
    assert_output --partial "Otter Jobs"
}

@test "oj --version exits 0" {
    run "$OJ_BIN" --version
    assert_success
}

@test "oj run --help shows subcommands" {
    run "$OJ_BIN" run --help
    assert_success
    assert_output --partial "build"
    assert_output --partial "bugfix"
}

@test "oj daemon --help shows interval options" {
    run "$OJ_BIN" daemon --help
    assert_success
    assert_output --partial "poll-interval"
    assert_output --partial "tick-interval"
}

@test "oj pipeline --help shows subcommands" {
    run "$OJ_BIN" pipeline --help
    assert_success
    assert_output --partial "list"
    assert_output --partial "show"
}

@test "oj workspace --help shows subcommands" {
    run "$OJ_BIN" workspace --help
    assert_success
    assert_output --partial "list"
}

@test "oj session --help shows subcommands" {
    run "$OJ_BIN" session --help
    assert_success
    assert_output --partial "list"
    assert_output --partial "kill"
    assert_output --partial "nudge"
}

@test "oj queue --help shows subcommands" {
    run "$OJ_BIN" queue --help
    assert_success
    assert_output --partial "list"
}

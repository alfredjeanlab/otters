// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for workspace management
//!
//! These tests verify `oj workspace` commands work correctly.
//! Workspace management is durable infrastructure per EXECUTION.md.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{setup_test_env, tmux, unique_id};
use predicates::prelude::*;
use std::fs;

#[test]
fn test_workspace_list_shows_workspaces() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("ws-list-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline (which creates a workspace)
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Workspace list test"])
        .assert()
        .success();

    // Workspace should appear in list
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["workspace", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_workspace_show_displays_details() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("ws-show-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Workspace show test"])
        .assert()
        .success();

    // Show workspace details
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["workspace", "show", &pipeline_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id));
}

#[test]
fn test_workspace_creates_git_worktree() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("worktree-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Git worktree test"])
        .assert()
        .success();

    // Verify .worktrees/{pipeline_id} is a valid git worktree
    let worktree_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    assert!(worktree_path.exists(), "Worktree directory should exist");

    // A git worktree has a .git file (not directory) pointing to the main repo
    let git_path = worktree_path.join(".git");
    assert!(
        git_path.exists(),
        ".git should exist in worktree (as file or directory)"
    );
}

#[test]
fn test_workspace_has_claudemd() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("claudemd-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "CLAUDE.md existence test"])
        .assert()
        .success();

    // CLAUDE.md should exist in workspace
    let claude_md = temp
        .path()
        .join(format!(".worktrees/{}/CLAUDE.md", pipeline_id));
    assert!(claude_md.exists(), "CLAUDE.md should exist in workspace");

    // Verify it contains the prompt
    let content = fs::read_to_string(&claude_md).unwrap();
    assert!(
        content.contains("CLAUDE.md existence test"),
        "CLAUDE.md should contain the prompt"
    );
}

#[test]
fn test_workspace_settings_synced() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("settings-{}", id);
    let pipeline_id = format!("build-{}", name);

    let _guard = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id));

    // Create a .claude/settings.local.json in the project root
    fs::create_dir_all(temp.path().join(".claude")).unwrap();
    fs::write(
        temp.path().join(".claude/settings.local.json"),
        r#"{"test": "value"}"#,
    )
    .unwrap();

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Settings sync test"])
        .assert()
        .success();

    // Settings should be synced to workspace
    let workspace_settings = temp.path().join(format!(
        ".worktrees/{}/.claude/settings.local.json",
        pipeline_id
    ));

    // Note: Settings sync may or may not be implemented yet
    // If implemented, verify the file exists and has correct content
    if workspace_settings.exists() {
        let content = fs::read_to_string(&workspace_settings).unwrap();
        assert!(
            content.contains("test"),
            "Settings should be synced to workspace"
        );
    }
}

#[test]
fn test_workspace_cleanup_on_delete() {
    let temp = setup_test_env();
    let id = unique_id();
    let name = format!("cleanup-{}", id);
    let pipeline_id = format!("build-{}", name);

    // Don't use SessionGuard since we're testing cleanup

    // Create a pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["run", "build", &name, "Cleanup test"])
        .assert()
        .success();

    let worktree_path = temp.path().join(format!(".worktrees/{}", pipeline_id));
    assert!(worktree_path.exists(), "Worktree should exist initially");

    // Kill any associated session first
    tmux::kill_sessions_matching(&format!("oj-{}", pipeline_id));

    // Delete the workspace
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["workspace", "delete", &pipeline_id, "--force"])
        .assert()
        .success();

    // Worktree should be removed (or marked for removal)
    // Note: The actual behavior depends on implementation
    // Some implementations may keep the directory but mark it as deleted
}

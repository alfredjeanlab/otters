// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn prepare_creates_claude_md() {
    let workspace = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();

    prepare_for_agent(
        workspace.path(),
        project.path(),
        "test-pipeline",
        "Do the thing",
    )
    .unwrap();

    let claude_md = workspace.path().join("CLAUDE.md");
    assert!(claude_md.exists());

    let content = fs::read_to_string(&claude_md).unwrap();
    assert!(content.contains("# test-pipeline"));
    assert!(content.contains("Do the thing"));
    assert!(content.contains("oj done"));
}

#[test]
fn prepare_copies_settings_if_present() {
    let workspace = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();

    // Create project settings
    let settings_dir = project.path().join(".claude");
    fs::create_dir_all(&settings_dir).unwrap();
    fs::write(settings_dir.join("settings.json"), r#"{"key": "value"}"#).unwrap();

    prepare_for_agent(workspace.path(), project.path(), "test", "prompt").unwrap();

    let local_settings = workspace.path().join(".claude/settings.local.json");
    assert!(local_settings.exists());
    let content = fs::read_to_string(&local_settings).unwrap();
    assert_eq!(content, r#"{"key": "value"}"#);
}

#[test]
fn prepare_skips_settings_if_absent() {
    let workspace = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();

    // No project settings
    prepare_for_agent(workspace.path(), project.path(), "test", "prompt").unwrap();

    let local_settings = workspace.path().join(".claude/settings.local.json");
    assert!(!local_settings.exists());
}

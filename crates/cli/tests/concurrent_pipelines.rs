// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! CLI integration tests for concurrent pipeline execution
//!
//! These tests verify that multiple pipelines can run simultaneously
//! without interfering with each other. They test workspace isolation,
//! state file isolation, and signal handling between pipelines.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(deprecated)]

mod common;

use assert_cmd::Command;
use common::{setup_test_env, tmux, unique_id};
use predicates::prelude::*;
use std::fs;

#[test]
fn test_two_pipelines_run_concurrently() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("concurrent-a-{}", id);
    let name2 = format!("concurrent-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    // Setup guards for both sessions
    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create first pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=First concurrent pipeline",
        ])
        .assert()
        .success();

    // Create second pipeline
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=Second concurrent pipeline",
        ])
        .assert()
        .success();

    // Both should exist in tmux
    assert!(
        tmux::session_matches(&format!("oj-{}", pipeline_id1)),
        "First pipeline session should exist"
    );
    assert!(
        tmux::session_matches(&format!("oj-{}", pipeline_id2)),
        "Second pipeline session should exist"
    );

    // Both should be listed
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&name1))
        .stdout(predicate::str::contains(&name2));
}

#[test]
fn test_three_pipelines_run_concurrently() {
    let temp = setup_test_env();
    let id = unique_id();

    let names: Vec<_> = (0..3).map(|i| format!("triple-{}-{}", i, id)).collect();
    let pipeline_ids: Vec<_> = names.iter().map(|n| format!("build-{}", n)).collect();

    // Setup guards
    let _guards: Vec<_> = pipeline_ids
        .iter()
        .map(|pid| tmux::SessionGuard::new(&format!("oj-{}", pid)))
        .collect();

    // Create all three pipelines
    for name in &names {
        Command::cargo_bin("oj")
            .unwrap()
            .current_dir(temp.path())
            .args([
                "run",
                "build",
                "--input",
                &format!("name={}", name),
                "--input",
                "prompt=Multi-concurrent pipeline",
            ])
            .assert()
            .success();
    }

    // All three should exist
    for pipeline_id in &pipeline_ids {
        assert!(
            tmux::session_matches(&format!("oj-{}", pipeline_id)),
            "Pipeline {} session should exist",
            pipeline_id
        );
    }
}

#[test]
fn test_pipelines_have_isolated_workspaces() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("isolated-a-{}", id);
    let name2 = format!("isolated-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create pipelines with different prompts
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=First unique prompt for isolation test",
        ])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=Second unique prompt for isolation test",
        ])
        .assert()
        .success();

    // Verify each workspace has separate CLAUDE.md with its own prompt
    let ws1_claude = temp
        .path()
        .join(format!(".worktrees/{}/CLAUDE.md", pipeline_id1));
    let ws2_claude = temp
        .path()
        .join(format!(".worktrees/{}/CLAUDE.md", pipeline_id2));

    assert!(
        ws1_claude.exists(),
        "First workspace CLAUDE.md should exist"
    );
    assert!(
        ws2_claude.exists(),
        "Second workspace CLAUDE.md should exist"
    );

    let content1 = fs::read_to_string(&ws1_claude).unwrap();
    let content2 = fs::read_to_string(&ws2_claude).unwrap();

    assert!(
        content1.contains("First unique prompt"),
        "First CLAUDE.md should have first prompt"
    );
    assert!(
        content2.contains("Second unique prompt"),
        "Second CLAUDE.md should have second prompt"
    );
    assert!(
        !content1.contains("Second unique prompt"),
        "First CLAUDE.md should NOT have second prompt"
    );
    assert!(
        !content2.contains("First unique prompt"),
        "Second CLAUDE.md should NOT have first prompt"
    );
}

#[test]
fn test_pipelines_have_isolated_state_files() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("state-a-{}", id);
    let name2 = format!("state-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create both pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=State isolation test A",
        ])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=State isolation test B",
        ])
        .assert()
        .success();

    // Verify both pipelines exist via CLI
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id1))
        .stdout(predicate::str::contains(&pipeline_id2));
}

#[test]
fn test_done_signal_affects_correct_pipeline() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("signal-a-{}", id);
    let name2 = format!("signal-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create both pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=Signal test A",
        ])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=Signal test B",
        ])
        .assert()
        .success();

    // Signal done only for the first pipeline
    let workspace1 = temp.path().join(format!(".worktrees/{}", pipeline_id1));
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace1)
        .env("OTTER_PIPELINE", &pipeline_id1)
        .arg("done")
        .assert()
        .success()
        .stdout(predicate::str::contains("phase complete"));

    // Second pipeline should still be listed as active
    // (First pipeline might still be listed depending on state)
    // Verify second pipeline still exists and is unchanged
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id2));
}

#[test]
fn test_error_in_one_doesnt_affect_other() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("error-a-{}", id);
    let name2 = format!("error-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create both pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=Error test A",
        ])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=Error test B",
        ])
        .assert()
        .success();

    // Signal error for first pipeline
    let workspace1 = temp.path().join(format!(".worktrees/{}", pipeline_id1));
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace1)
        .env("OTTER_PIPELINE", &pipeline_id1)
        .args(["done", "--error", "Intentional test failure"])
        .assert()
        .success();

    // Second pipeline should still work normally
    let workspace2 = temp.path().join(format!(".worktrees/{}", pipeline_id2));
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(&workspace2)
        .env("OTTER_PIPELINE", &pipeline_id2)
        .arg("done")
        .assert()
        .success()
        .stdout(predicate::str::contains("phase complete"));
}

#[test]
fn test_daemon_processes_multiple_pipelines() {
    let temp = setup_test_env();
    let id = unique_id();

    let name1 = format!("daemon-a-{}", id);
    let name2 = format!("daemon-b-{}", id);
    let pipeline_id1 = format!("build-{}", name1);
    let pipeline_id2 = format!("build-{}", name2);

    let _guard1 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id1));
    let _guard2 = tmux::SessionGuard::new(&format!("oj-{}", pipeline_id2));

    // Create both pipelines
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name1),
            "--input",
            "prompt=Multi-pipeline daemon test A",
        ])
        .assert()
        .success();

    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "run",
            "build",
            "--input",
            &format!("name={}", name2),
            "--input",
            "prompt=Multi-pipeline daemon test B",
        ])
        .assert()
        .success();

    // Daemon should process both pipelines without error
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["daemon", "--once"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Running single daemon iteration"));

    // Both pipelines should still exist
    Command::cargo_bin("oj")
        .unwrap()
        .current_dir(temp.path())
        .args(["pipeline", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&pipeline_id1))
        .stdout(predicate::str::contains(&pipeline_id2));
}

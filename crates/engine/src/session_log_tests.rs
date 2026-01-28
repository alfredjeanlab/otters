// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::io::Write;
use tempfile::TempDir;

fn create_log_file(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("test.jsonl");
    let mut file = File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    path
}

#[test]
fn detects_waiting_for_input() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"user","message":{"content":"Fix bug"}}
{"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Done!"}]}}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::WaitingForInput);
}

#[test]
fn detects_working_on_tool() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"user","message":{"content":"Fix bug"}}
{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"Bash"}]}}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::Working);
}

#[test]
fn detects_working_on_user_message() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"user","message":{"content":"Fix the bug please"}}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::Working);
}

#[test]
fn detects_unauthorized() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"error","error":"Invalid API key - unauthorized"}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert!(matches!(
        watcher.check_state(),
        SessionState::Failed(FailureReason::Unauthorized)
    ));
}

#[test]
fn detects_out_of_credits() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"error","error":"You have exceeded your current quota"}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert!(matches!(
        watcher.check_state(),
        SessionState::Failed(FailureReason::OutOfCredits)
    ));
}

#[test]
fn detects_rate_limited() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"error","error":"Rate limit exceeded - too many requests"}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert!(matches!(
        watcher.check_state(),
        SessionState::Failed(FailureReason::RateLimited)
    ));
}

#[test]
fn detects_network_error() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"error","error":"Network connection failed"}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert!(matches!(
        watcher.check_state(),
        SessionState::Failed(FailureReason::NoInternet)
    ));
}

#[test]
fn detects_other_error() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"error","error":"Something went wrong"}"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert!(matches!(
        watcher.check_state(),
        SessionState::Failed(FailureReason::Other(_))
    ));
}

#[test]
fn returns_unknown_for_missing_file() {
    let watcher = SessionLogWatcher::new(PathBuf::from("/nonexistent/path.jsonl"));
    assert_eq!(watcher.check_state(), SessionState::Unknown);
}

#[test]
fn returns_unknown_for_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = create_log_file(&dir, "");
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::Unknown);
}

#[test]
fn returns_unknown_for_invalid_json() {
    let dir = TempDir::new().unwrap();
    let path = create_log_file(&dir, "not valid json");
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::Unknown);
}

#[test]
fn skips_empty_lines() {
    let dir = TempDir::new().unwrap();
    let log = r#"{"type":"user","message":{"content":"Fix bug"}}

{"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Done!"}]}}

"#;

    let path = create_log_file(&dir, log);
    let watcher = SessionLogWatcher::new(path);

    assert_eq!(watcher.check_state(), SessionState::WaitingForInput);
}

#[test]
fn finds_session_log_by_session_id() {
    let tmp = TempDir::new().unwrap();

    // Create fake project dir with session log
    let project_path = Path::new("/test/project");
    let project_hash = hash_project_path(project_path);
    let project_dir = tmp.path().join("projects").join(&project_hash);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("session-123.jsonl"), "{}").unwrap();

    let log_path = find_session_log_in(project_path, "session-123", tmp.path());
    assert!(log_path.is_some());
    assert!(log_path.unwrap().ends_with("session-123.jsonl"));
}

#[test]
fn finds_most_recent_log_as_fallback() {
    let tmp = TempDir::new().unwrap();

    // Create fake project dir with multiple session logs
    let project_path = Path::new("/test/project2");
    let project_hash = hash_project_path(project_path);
    let project_dir = tmp.path().join("projects").join(&project_hash);
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create older file
    std::fs::write(project_dir.join("old-session.jsonl"), "{}").unwrap();
    // Small delay to ensure different modified times
    std::thread::sleep(std::time::Duration::from_millis(10));
    // Create newer file
    std::fs::write(project_dir.join("new-session.jsonl"), "{}").unwrap();

    // Request non-existent session - should get most recent
    let log_path = find_session_log_in(project_path, "nonexistent", tmp.path());
    assert!(log_path.is_some());
    assert!(log_path.unwrap().ends_with("new-session.jsonl"));
}

#[test]
fn returns_none_for_missing_project_dir() {
    let tmp = TempDir::new().unwrap();

    let log_path =
        find_session_log_in(Path::new("/nonexistent/project"), "session-123", tmp.path());
    assert!(log_path.is_none());
}

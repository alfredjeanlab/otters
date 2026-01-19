// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::path::Path;
use std::time::Duration;

// ============================================================================
// Basic parsing
// ============================================================================

#[test]
fn parse_empty_runbook() {
    let runbook = parse_runbook("").unwrap();
    assert!(runbook.command.is_empty());
    assert!(runbook.worker.is_empty());
    assert!(runbook.pipeline.is_empty());
}

#[test]
fn parse_command() {
    let toml = r#"
[command.build]
args = "<name> <prompt>"
run = "echo {name} {prompt}"

[command.build.aliases]
n = "name"
p = "prompt"

[command.build.defaults]
priority = 2
"#;

    let runbook = parse_runbook(toml).unwrap();
    assert!(runbook.command.contains_key("build"));

    let cmd = &runbook.command["build"];
    assert_eq!(cmd.args, Some("<name> <prompt>".to_string()));
    assert_eq!(cmd.run, Some("echo {name} {prompt}".to_string()));
    assert_eq!(cmd.aliases.get("n"), Some(&"name".to_string()));
    assert_eq!(cmd.aliases.get("p"), Some(&"prompt".to_string()));
}

#[test]
fn parse_worker() {
    let toml = r#"
[worker.builds]
queue = "builds"
handler = "pipeline.build"
concurrency = 2
idle_action = "wait:30s"
wake_on = ["build:queued", "build:retry"]
on_unhealthy = "restart"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let worker = &runbook.worker["builds"];

    assert_eq!(worker.queue, Some("builds".to_string()));
    assert_eq!(worker.handler, Some("pipeline.build".to_string()));
    assert_eq!(worker.concurrency, Some(2));
    assert_eq!(worker.idle_action, Some("wait:30s".to_string()));
    assert_eq!(worker.wake_on, vec!["build:queued", "build:retry"]);
    assert_eq!(worker.on_unhealthy, Some("restart".to_string()));
}

#[test]
fn parse_queue() {
    let toml = r#"
[queue.builds]
order = "priority DESC"
visibility_timeout = "4h"
max_retries = 3
on_exhaust = "dead"

[queue.builds.dead]
retention = "7d"
on_add = "notify admin"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let queue = &runbook.queue["builds"];

    assert_eq!(queue.order, Some("priority DESC".to_string()));
    assert_eq!(
        queue.visibility_timeout,
        Some(Duration::from_secs(4 * 3600))
    );
    assert_eq!(queue.max_retries, Some(3));
    assert_eq!(queue.on_exhaust, Some("dead".to_string()));

    let dead = queue.dead.as_ref().unwrap();
    assert_eq!(dead.retention, Some("7d".to_string()));
    assert_eq!(dead.on_add, Some("notify admin".to_string()));
}

#[test]
fn parse_queue_with_source() {
    let toml = r#"
[queue.bugs]
source = "wk list -l bug -s todo --json"
filter = "not has_label('assigned')"
order = "priority"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let queue = &runbook.queue["bugs"];

    assert_eq!(
        queue.source,
        Some("wk list -l bug -s todo --json".to_string())
    );
    assert_eq!(queue.filter, Some("not has_label('assigned')".to_string()));
}

// ============================================================================
// Pipeline parsing
// ============================================================================

#[test]
fn parse_pipeline() {
    let toml = r#"
[pipeline.build]
inputs = ["name", "prompt"]

[pipeline.build.defaults]
workspace = ".worktrees/{name}"
branch = "feature/{name}"

[[pipeline.build.phase]]
name = "init"
run = "git worktree add {workspace}"
next = "plan"

[[pipeline.build.phase]]
name = "plan"
task = "planning"
semaphore = "agents"
post = ["plan_exists"]
next = "execute"

[[pipeline.build.phase]]
name = "execute"
task = "execution"
next = "merge"
on_fail = "escalate"

[[pipeline.build.phase]]
name = "merge"
strategy = "merge"
lock = "main_branch"
next = "done"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let pipeline = &runbook.pipeline["build"];

    assert_eq!(pipeline.inputs, vec!["name", "prompt"]);
    assert_eq!(
        pipeline.defaults.get("workspace"),
        Some(&".worktrees/{name}".to_string())
    );

    assert_eq!(pipeline.phase.len(), 4);

    let init = &pipeline.phase[0];
    assert_eq!(init.name, "init");
    assert_eq!(init.run, Some("git worktree add {workspace}".to_string()));
    assert_eq!(init.next, Some("plan".to_string()));

    let plan = &pipeline.phase[1];
    assert_eq!(plan.name, "plan");
    assert_eq!(plan.task, Some("planning".to_string()));
    assert_eq!(plan.semaphore, Some("agents".to_string()));
    assert_eq!(plan.post, vec!["plan_exists"]);

    let execute = &pipeline.phase[2];
    assert_eq!(execute.on_fail, Some("escalate".to_string()));

    let merge = &pipeline.phase[3];
    assert_eq!(merge.strategy, Some("merge".to_string()));
    assert_eq!(merge.lock, Some("main_branch".to_string()));
}

// ============================================================================
// Task parsing
// ============================================================================

#[test]
fn parse_task_with_prompt_file() {
    let toml = r#"
[task.planning]
command = "claude --print"
prompt_file = "templates/plan.md"
cwd = "{workspace}"
heartbeat = "output"
timeout = "30m"
idle_timeout = "2m"
on_stuck = "restart"

[task.planning.env]
OTTER_PIPELINE = "{name}"
OTTER_PHASE = "plan"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let task = &runbook.task["planning"];

    assert_eq!(task.command, Some("claude --print".to_string()));
    assert_eq!(task.prompt_file, Some("templates/plan.md".to_string()));
    assert!(task.prompt.is_none());
    assert_eq!(task.cwd, Some("{workspace}".to_string()));
    assert_eq!(task.heartbeat, Some("output".to_string()));
    assert_eq!(task.timeout, Some(Duration::from_secs(30 * 60)));
    assert_eq!(task.idle_timeout, Some(Duration::from_secs(2 * 60)));
    assert_eq!(task.on_stuck, vec!["restart"]);
    assert_eq!(task.env.get("OTTER_PIPELINE"), Some(&"{name}".to_string()));
}

#[test]
fn parse_task_with_inline_prompt() {
    let toml = r#"
[task.fix]
command = "claude --print"
prompt = """
Fix the bug in {workspace}:

{bug.description}
"""
timeout = "1h"
on_stuck = ["nudge", "restart"]
"#;

    let runbook = parse_runbook(toml).unwrap();
    let task = &runbook.task["fix"];

    assert!(task.prompt.is_some());
    assert!(task.prompt.as_ref().unwrap().contains("Fix the bug"));
    assert_eq!(task.on_stuck, vec!["nudge", "restart"]);
}

#[test]
fn parse_task_with_checkpoint() {
    let toml = r#"
[task.execution]
command = "claude --print"
prompt_file = "templates/execute.md"
timeout = "4h"
checkpoint_interval = "10m"
checkpoint = "wk note {epic} 'checkpoint'"
on_timeout = "checkpoint_and_escalate"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let task = &runbook.task["execution"];

    assert_eq!(task.checkpoint_interval, Some(Duration::from_secs(10 * 60)));
    assert_eq!(
        task.checkpoint,
        Some("wk note {epic} 'checkpoint'".to_string())
    );
    assert_eq!(task.on_timeout, Some("checkpoint_and_escalate".to_string()));
}

// ============================================================================
// Guard parsing
// ============================================================================

#[test]
fn parse_guard() {
    let toml = r#"
[guard.plan_exists]
condition = "test -f plans/{name}.md"
timeout = "30m"
on_timeout = "escalate"

[guard.blocker_merged]
condition = "oj pipeline show {after} --phase | grep -q merged"
wake_on = ["pipeline:{after}:merged"]
"#;

    let runbook = parse_runbook(toml).unwrap();

    let plan_guard = &runbook.guard["plan_exists"];
    assert_eq!(
        plan_guard.condition,
        Some("test -f plans/{name}.md".to_string())
    );
    assert_eq!(plan_guard.timeout, Some(Duration::from_secs(30 * 60)));
    assert_eq!(plan_guard.on_timeout, Some("escalate".to_string()));

    let blocker_guard = &runbook.guard["blocker_merged"];
    assert_eq!(blocker_guard.wake_on, vec!["pipeline:{after}:merged"]);
}

#[test]
fn parse_guard_with_retry() {
    let toml = r#"
[guard.issues_closed]
condition = "wk list -l plan:{name} -s todo --count | grep -q '^0$'"
timeout = "5m"
on_timeout = "escalate"

[guard.issues_closed.retry]
max = 3
interval = "10s"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let guard = &runbook.guard["issues_closed"];

    let retry = guard.retry.as_ref().unwrap();
    assert_eq!(retry.max, Some(3));
    assert_eq!(retry.interval, Some(Duration::from_secs(10)));
}

// ============================================================================
// Strategy parsing
// ============================================================================

#[test]
fn parse_strategy() {
    let toml = r#"
[strategy.merge]
checkpoint = "git rev-parse HEAD"
on_exhaust = "escalate"

[[strategy.merge.attempt]]
name = "fast-forward"
run = "git merge --ff-only"
timeout = "1m"

[[strategy.merge.attempt]]
name = "rebase"
run = "git rebase main"
timeout = "5m"
rollback = "git rebase --abort"

[[strategy.merge.attempt]]
name = "agent-resolve"
task = "conflict_resolution"
timeout = "30m"
rollback = "git reset --hard {checkpoint}"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let strategy = &runbook.strategy["merge"];

    assert_eq!(strategy.checkpoint, Some("git rev-parse HEAD".to_string()));
    assert_eq!(strategy.on_exhaust, Some("escalate".to_string()));
    assert_eq!(strategy.attempt.len(), 3);

    let ff = &strategy.attempt[0];
    assert_eq!(ff.name, "fast-forward");
    assert_eq!(ff.run, Some("git merge --ff-only".to_string()));
    assert_eq!(ff.timeout, Some(Duration::from_secs(60)));
    assert!(ff.rollback.is_none());

    let rebase = &strategy.attempt[1];
    assert_eq!(rebase.name, "rebase");
    assert_eq!(rebase.rollback, Some("git rebase --abort".to_string()));

    let agent = &strategy.attempt[2];
    assert_eq!(agent.task, Some("conflict_resolution".to_string()));
    assert!(agent.run.is_none());
}

// ============================================================================
// Lock and semaphore parsing
// ============================================================================

#[test]
fn parse_lock() {
    let toml = r#"
[lock.main_branch]
timeout = "30m"
heartbeat = "30s"
on_stale = ["release", "rollback", "escalate"]
"#;

    let runbook = parse_runbook(toml).unwrap();
    let lock = &runbook.lock["main_branch"];

    assert_eq!(lock.timeout, Some(Duration::from_secs(30 * 60)));
    assert_eq!(lock.heartbeat, Some(Duration::from_secs(30)));
    assert_eq!(lock.on_stale, vec!["release", "rollback", "escalate"]);
}

#[test]
fn parse_lock_on_stale_string() {
    let toml = r#"
[lock.simple]
timeout = "10m"
on_stale = "release"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let lock = &runbook.lock["simple"];
    assert_eq!(lock.on_stale, vec!["release"]);
}

#[test]
fn parse_semaphore() {
    let toml = r#"
[semaphore.agents]
max = 4
slot_timeout = "4h"
slot_heartbeat = "1m"
on_orphan = "reclaim"
on_orphan_work = "requeue"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let sem = &runbook.semaphore["agents"];

    assert_eq!(sem.max, Some(4));
    assert_eq!(sem.slot_timeout, Some(Duration::from_secs(4 * 3600)));
    assert_eq!(sem.slot_heartbeat, Some(Duration::from_secs(60)));
    assert_eq!(sem.on_orphan, Some("reclaim".to_string()));
    assert_eq!(sem.on_orphan_work, Some("requeue".to_string()));
}

// ============================================================================
// Events parsing
// ============================================================================

#[test]
fn parse_events() {
    let toml = r#"
[events]
on_phase_change = "oj emit pipeline:phase --id {name}"
on_complete = "oj emit pipeline:complete --id {name}"
on_fail = "oj emit pipeline:fail --id {name} --error '{error}'"
"#;

    let runbook = parse_runbook(toml).unwrap();
    let events = runbook.events.unwrap();

    assert_eq!(events.handlers.len(), 3);
    assert!(events.handlers.contains_key("on_phase_change"));
    assert!(events.handlers.contains_key("on_complete"));
    assert!(events.handlers.contains_key("on_fail"));
}

// ============================================================================
// Error handling
// ============================================================================

#[test]
fn parse_invalid_toml_returns_error() {
    let result = parse_runbook("this is not valid toml [[[");
    assert!(result.is_err());
    assert!(matches!(result, Err(ParseError::Toml(_))));
}

#[test]
fn parse_runbook_file_not_found() {
    let result = parse_runbook_file(Path::new("/nonexistent/file.toml"));
    assert!(result.is_err());
    assert!(matches!(result, Err(ParseError::Io { .. })));
}

// ============================================================================
// Example runbook parsing
// ============================================================================

#[test]
fn parse_build_example_runbook() {
    let content = include_str!("../../../../docs/10-example-runbooks/build.toml");
    let runbook = parse_runbook(content).unwrap();

    // Commands
    assert!(runbook.command.contains_key("build"));

    // Workers
    assert!(runbook.worker.contains_key("builds"));

    // Queues
    assert!(runbook.queue.contains_key("builds"));

    // Pipelines
    assert!(runbook.pipeline.contains_key("build"));
    let pipeline = &runbook.pipeline["build"];
    assert!(!pipeline.phase.is_empty());

    // Tasks
    assert!(runbook.task.contains_key("planning"));
    assert!(runbook.task.contains_key("decomposition"));
    assert!(runbook.task.contains_key("execution"));

    // Guards
    assert!(runbook.guard.contains_key("plan_exists"));

    // Strategies
    assert!(runbook.strategy.contains_key("merge"));

    // Locks
    assert!(runbook.lock.contains_key("main_branch"));

    // Semaphores
    assert!(runbook.semaphore.contains_key("agents"));
}

#[test]
fn parse_bugfix_example_runbook() {
    let content = include_str!("../../../../docs/10-example-runbooks/bugfix.toml");
    let runbook = parse_runbook(content).unwrap();

    // Commands
    assert!(runbook.command.contains_key("bugfix"));

    // Pipelines
    assert!(runbook.pipeline.contains_key("fix"));
    let pipeline = &runbook.pipeline["fix"];
    assert_eq!(pipeline.inputs, vec!["bug"]);

    // Task with inline prompt
    let task = &runbook.task["fix_task"];
    assert!(task.prompt.is_some());
}

// ============================================================================
// Utility functions
// ============================================================================

#[test]
fn runbook_name_extracts_stem() {
    assert_eq!(runbook_name(Path::new("build.toml")), Some("build"));
    assert_eq!(
        runbook_name(Path::new("runbooks/build.toml")),
        Some("build")
    );
    assert_eq!(runbook_name(Path::new("/path/to/foo.toml")), Some("foo"));
}

// ============================================================================
// System runbook parsing (scheduling primitives)
// ============================================================================

#[test]
fn parse_watchdog_runbook() {
    let content = include_str!("../../../../runbooks/watchdog.toml");
    let runbook = parse_runbook(content).unwrap();

    // Meta
    assert!(runbook.meta.is_some());
    let meta = runbook.meta.as_ref().unwrap();
    assert_eq!(meta.name, Some("watchdog".to_string()));

    // Actions
    assert!(runbook.action.contains_key("nudge"));
    assert!(runbook.action.contains_key("restart"));
    assert!(runbook.action.contains_key("escalate"));

    let nudge = &runbook.action["nudge"];
    assert_eq!(nudge.cooldown, Some(Duration::from_secs(30)));
    assert_eq!(nudge.command, Some("send_to_session".to_string()));
    assert!(nudge.args.contains_key("message"));

    // Watchers
    assert!(runbook.watcher.contains_key("agent-idle"));

    let watcher = &runbook.watcher["agent-idle"];
    assert_eq!(watcher.source.source_type, "session");
    assert_eq!(watcher.source.pattern, Some("*".to_string()));
    assert_eq!(watcher.condition.condition_type, "idle");
    assert_eq!(watcher.condition.threshold, Some(Duration::from_secs(300)));
    assert_eq!(watcher.check_interval, Some(Duration::from_secs(60)));
    assert_eq!(watcher.response.len(), 3);

    // Response chain
    assert_eq!(watcher.response[0].action, "nudge");
    assert_eq!(watcher.response[1].action, "restart");
    assert_eq!(watcher.response[1].delay, Some(Duration::from_secs(120)));
    assert!(watcher.response[1].requires_previous_failure);
}

#[test]
fn parse_janitor_runbook() {
    let content = include_str!("../../../../runbooks/janitor.toml");
    let runbook = parse_runbook(content).unwrap();

    // Meta
    assert!(runbook.meta.is_some());
    let meta = runbook.meta.as_ref().unwrap();
    assert_eq!(meta.name, Some("janitor".to_string()));

    // Scanners
    assert!(!runbook.scanner.is_empty());
    assert!(runbook.scanner.contains_key("stale-locks"));
    assert!(runbook.scanner.contains_key("dead-queue-items"));
    assert!(runbook.scanner.contains_key("orphan-worktrees"));

    let stale_locks = &runbook.scanner["stale-locks"];
    assert_eq!(stale_locks.source.source_type, "locks");
    assert_eq!(stale_locks.condition.condition_type, "stale");
    assert_eq!(
        stale_locks.condition.threshold,
        Some(Duration::from_secs(3600))
    );
    assert_eq!(stale_locks.cleanup.action_type, "release");
    assert_eq!(stale_locks.interval, Some(Duration::from_secs(600)));

    let orphan_worktrees = &runbook.scanner["orphan-worktrees"];
    assert_eq!(orphan_worktrees.source.source_type, "worktrees");
    assert_eq!(orphan_worktrees.condition.condition_type, "orphaned");
    assert_eq!(orphan_worktrees.cleanup.action_type, "delete");
}

#[test]
fn parse_triager_runbook() {
    let content = include_str!("../../../../runbooks/triager.toml");
    let runbook = parse_runbook(content).unwrap();

    // Meta
    assert!(runbook.meta.is_some());
    let meta = runbook.meta.as_ref().unwrap();
    assert_eq!(meta.name, Some("triager".to_string()));

    // Actions
    assert!(!runbook.action.is_empty());
    assert!(runbook.action.contains_key("notify-team"));
    assert!(runbook.action.contains_key("pause-pipelines"));
    assert!(runbook.action.contains_key("analyze-failure"));

    // Watchers
    assert!(!runbook.watcher.is_empty());
    assert!(runbook.watcher.contains_key("build-failures"));
    assert!(runbook.watcher.contains_key("test-flakiness"));

    let build_failures = &runbook.watcher["build-failures"];
    assert_eq!(build_failures.source.source_type, "events");
    assert_eq!(
        build_failures.source.pattern,
        Some("pipeline:failed:*".to_string())
    );
    assert_eq!(
        build_failures.condition.condition_type,
        "consecutive_failures"
    );
    assert_eq!(build_failures.condition.count, Some(3));

    // resource-exhaustion uses count for exceeds
    let resource_exhaustion = &runbook.watcher["resource-exhaustion"];
    assert_eq!(resource_exhaustion.condition.condition_type, "exceeds");
    assert_eq!(resource_exhaustion.condition.count, Some(10));

    // Crons
    assert!(!runbook.cron.is_empty());
    assert!(runbook.cron.contains_key("daily-health-check"));
    assert!(runbook.cron.contains_key("weekly-cleanup-report"));

    let daily = &runbook.cron["daily-health-check"];
    assert_eq!(daily.interval, Some(Duration::from_secs(24 * 3600)));
    assert!(daily.enabled);
    assert!(daily.run.is_some());
}

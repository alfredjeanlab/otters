use std::collections::BTreeMap;
// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::runbook::{parse_runbook, validate_runbook};

// ============================================================================
// Basic loading
// ============================================================================

#[test]
fn load_minimal_runbook() {
    let raw = parse_runbook(
        r#"
[command.hello]
run = "echo hello"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    assert_eq!(runbook.commands.len(), 1);
    assert!(runbook.commands.contains_key("hello"));
}

#[test]
fn load_command_with_fields() {
    let raw = parse_runbook(
        r#"
[command.complex]
args = "<name> [--flag]"
run = "echo $name"

[command.complex.aliases]
f = "flag"

[command.complex.defaults]
name = "default"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let cmd = runbook.commands.get("complex").unwrap();
    assert_eq!(cmd.args, Some("<name> [--flag]".to_string()));
    assert_eq!(cmd.run, Some("echo $name".to_string()));
    assert_eq!(cmd.aliases.get("f"), Some(&"flag".to_string()));
}

// ============================================================================
// Pipeline loading
// ============================================================================

#[test]
fn load_pipeline_with_phases() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "claude --print"

[pipeline.build]
inputs = ["name"]

[pipeline.build.defaults]
name = "default"

[[pipeline.build.phase]]
name = "init"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let pipeline = runbook.pipelines.get("build").unwrap();
    assert_eq!(pipeline.name, "build");
    assert_eq!(pipeline.inputs, vec!["name"]);
    assert_eq!(pipeline.defaults.get("name"), Some(&"default".to_string()));
    assert_eq!(pipeline.initial_phase, "init");

    let phase = pipeline.phases.get("init").unwrap();
    assert_eq!(phase.name, "init");
    assert!(matches!(&phase.action, PhaseAction::Task { name } if name == "work"));
    assert!(matches!(&phase.next, PhaseNext::Done));
}

#[test]
fn load_pipeline_phase_with_run() {
    let raw = parse_runbook(
        r#"
[pipeline.test]
[[pipeline.test.phase]]
name = "build"
run = "make build"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let phase = runbook
        .pipelines
        .get("test")
        .unwrap()
        .phases
        .get("build")
        .unwrap();
    assert!(matches!(&phase.action, PhaseAction::Run { command } if command == "make build"));
}

#[test]
fn load_pipeline_phase_with_strategy() {
    let raw = parse_runbook(
        r#"
[task.primary]
command = "echo primary"

[strategy.fallback]
[[strategy.fallback.attempt]]
name = "first"
task = "primary"

[pipeline.test]
[[pipeline.test.phase]]
name = "work"
strategy = "fallback"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let phase = runbook
        .pipelines
        .get("test")
        .unwrap()
        .phases
        .get("work")
        .unwrap();
    assert!(matches!(&phase.action, PhaseAction::Strategy { name } if name == "fallback"));
}

#[test]
fn load_pipeline_phase_with_guards() {
    let raw = parse_runbook(
        r#"
[guard.ready]
condition = "test -f ready.txt"

[guard.done]
condition = "test -f done.txt"

[task.work]
command = "claude"

[pipeline.test]
[[pipeline.test.phase]]
name = "work"
task = "work"
pre = ["ready"]
post = ["done"]
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let phase = runbook
        .pipelines
        .get("test")
        .unwrap()
        .phases
        .get("work")
        .unwrap();
    assert_eq!(phase.pre_guards, vec!["ready"]);
    assert_eq!(phase.post_guards, vec!["done"]);
}

#[test]
fn load_pipeline_phase_with_lock_and_semaphore() {
    let raw = parse_runbook(
        r#"
[lock.exclusive]
timeout = "5m"

[semaphore.concurrent]
max = 3

[task.work]
command = "claude"

[pipeline.test]
[[pipeline.test.phase]]
name = "work"
task = "work"
lock = "exclusive"
semaphore = "concurrent"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let phase = runbook
        .pipelines
        .get("test")
        .unwrap()
        .phases
        .get("work")
        .unwrap();
    assert_eq!(phase.lock, Some("exclusive".to_string()));
    assert_eq!(phase.semaphore, Some("concurrent".to_string()));
}

#[test]
fn load_pipeline_auto_next() {
    // When next is explicitly specified, it should be used
    let raw = parse_runbook(
        r#"
[task.work]
command = "echo"

[pipeline.test]
[[pipeline.test.phase]]
name = "first"
task = "work"
next = "second"

[[pipeline.test.phase]]
name = "second"
task = "work"
next = "third"

[[pipeline.test.phase]]
name = "third"
task = "work"
next = "done"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let pipeline = runbook.pipelines.get("test").unwrap();
    assert!(
        matches!(&pipeline.phases.get("first").unwrap().next, PhaseNext::Phase(p) if p == "second")
    );
    assert!(
        matches!(&pipeline.phases.get("second").unwrap().next, PhaseNext::Phase(p) if p == "third")
    );
    assert!(matches!(
        &pipeline.phases.get("third").unwrap().next,
        PhaseNext::Done
    ));
}

// ============================================================================
// Task loading
// ============================================================================

#[test]
fn load_task_with_all_fields() {
    let raw = parse_runbook(
        r#"
[task.complex]
command = "claude --print"
prompt = "Do something"
prompt_file = "prompts/task.md"
cwd = "/tmp"
heartbeat = "output"
timeout = "1h"
idle_timeout = "5m"
on_stuck = ["nudge", "kill"]
on_timeout = "escalate"

[task.complex.env]
FOO = "bar"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let task = runbook.tasks.get("complex").unwrap();
    assert_eq!(task.name, "complex");
    assert_eq!(task.command, Some("claude --print".to_string()));
    assert_eq!(task.prompt, Some("Do something".to_string()));
    assert_eq!(task.prompt_file, Some("prompts/task.md".to_string()));
    assert_eq!(task.cwd, Some("/tmp".to_string()));
    assert_eq!(task.heartbeat, Some("output".to_string()));
    assert_eq!(task.timeout, Some(Duration::from_secs(3600)));
    assert_eq!(task.idle_timeout, Some(Duration::from_secs(300)));
    assert_eq!(task.on_stuck, vec!["nudge", "kill"]);
    assert_eq!(task.on_timeout, Some("escalate".to_string()));
    assert_eq!(task.env.get("FOO"), Some(&"bar".to_string()));
}

// ============================================================================
// Guard loading
// ============================================================================

#[test]
fn load_guard_with_all_fields() {
    let raw = parse_runbook(
        r#"
[guard.ready]
condition = "test -f ready.txt"
wake_on = ["file_created"]
timeout = "5m"
on_timeout = "escalate"

[guard.ready.retry]
max = 5
interval = "10s"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let guard = runbook.guards.get("ready").unwrap();
    assert_eq!(guard.name, "ready");
    assert_eq!(guard.condition, Some("test -f ready.txt".to_string()));
    assert_eq!(guard.wake_on, vec!["file_created"]);
    assert_eq!(guard.timeout, Some(Duration::from_secs(300)));
    assert_eq!(guard.on_timeout, Some("escalate".to_string()));
    assert_eq!(guard.retry_max, Some(5));
    assert_eq!(guard.retry_interval, Some(Duration::from_secs(10)));
}

// ============================================================================
// Strategy loading
// ============================================================================

#[test]
fn load_strategy_with_attempts() {
    let raw = parse_runbook(
        r#"
[task.primary]
command = "echo primary"

[task.fallback]
command = "echo fallback"

[strategy.recovery]
checkpoint = "git rev-parse HEAD"
on_exhaust = "escalate"

[[strategy.recovery.attempt]]
name = "primary"
task = "primary"
timeout = "5m"
rollback = "git reset --hard"

[[strategy.recovery.attempt]]
name = "fallback"
task = "fallback"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let strategy = runbook.strategies.get("recovery").unwrap();
    assert_eq!(strategy.name, "recovery");
    assert_eq!(strategy.checkpoint, Some("git rev-parse HEAD".to_string()));
    assert!(matches!(strategy.on_exhausted, ExhaustedAction::Escalate));
    assert_eq!(strategy.attempts.len(), 2);

    let first = &strategy.attempts[0];
    assert_eq!(first.name, "primary");
    assert_eq!(first.task, Some("primary".to_string()));
    assert_eq!(first.timeout, Some(Duration::from_secs(300)));
    assert_eq!(first.rollback, Some("git reset --hard".to_string()));

    let second = &strategy.attempts[1];
    assert_eq!(second.name, "fallback");
    assert_eq!(second.task, Some("fallback".to_string()));
}

#[test]
fn load_strategy_on_exhausted_goto() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "echo"

[strategy.recovery]
on_exhaust = "cleanup_phase"

[[strategy.recovery.attempt]]
name = "try"
task = "work"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let strategy = runbook.strategies.get("recovery").unwrap();
    assert!(
        matches!(&strategy.on_exhausted, ExhaustedAction::GotoPhase(p) if p == "cleanup_phase")
    );
}

// ============================================================================
// Lock loading
// ============================================================================

#[test]
fn load_lock_with_all_fields() {
    let raw = parse_runbook(
        r#"
[lock.exclusive]
timeout = "5m"
heartbeat = "30s"
on_stale = ["release", "notify"]
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let lock = runbook.locks.get("exclusive").unwrap();
    assert_eq!(lock.name, "exclusive");
    assert_eq!(lock.timeout, Some(Duration::from_secs(300)));
    assert_eq!(lock.heartbeat, Some(Duration::from_secs(30)));
    assert_eq!(lock.on_stale, vec!["release", "notify"]);
}

// ============================================================================
// Semaphore loading
// ============================================================================

#[test]
fn load_semaphore() {
    let raw = parse_runbook(
        r#"
[semaphore.concurrent]
max = 5
slot_timeout = "2m"
slot_heartbeat = "20s"
on_orphan = "release"
on_orphan_work = "requeue"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let sem = runbook.semaphores.get("concurrent").unwrap();
    assert_eq!(sem.name, "concurrent");
    assert_eq!(sem.max, Some(5));
    assert_eq!(sem.slot_timeout, Some(Duration::from_secs(120)));
    assert_eq!(sem.slot_heartbeat, Some(Duration::from_secs(20)));
    assert_eq!(sem.on_orphan, Some("release".to_string()));
    assert_eq!(sem.on_orphan_work, Some("requeue".to_string()));
}

// ============================================================================
// Worker and Queue loading
// ============================================================================

#[test]
fn load_worker() {
    let raw = parse_runbook(
        r#"
[task.work]
command = "echo"

[queue.jobs]
source = "wk list --json"

[worker.processor]
queue = "jobs"
handler = "work"
concurrency = 4
idle_action = "wait:30s"
wake_on = ["new_job"]
on_unhealthy = "restart"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let worker = runbook.workers.get("processor").unwrap();
    assert_eq!(worker.name, "processor");
    assert_eq!(worker.concurrency, 4);
    assert_eq!(worker.handler, Some("work".to_string()));
    assert_eq!(worker.queue, Some("jobs".to_string()));
    assert_eq!(worker.idle_action, Some("wait:30s".to_string()));
    assert_eq!(worker.wake_on, vec!["new_job"]);
    assert_eq!(worker.on_unhealthy, Some("restart".to_string()));
}

#[test]
fn load_queue_with_dead_letter() {
    let raw = parse_runbook(
        r#"
[queue.main]
source = "wk list"
max_retries = 5
on_exhaust = "dead"

[queue.main.dead]
retention = "7d"
on_add = "notify"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let queue = runbook.queues.get("main").unwrap();
    assert_eq!(queue.max_retries, Some(5));
    assert_eq!(queue.on_exhaust, Some("dead".to_string()));

    let dl = queue.dead_letter.as_ref().unwrap();
    assert_eq!(dl.retention, Some("7d".to_string()));
    assert_eq!(dl.on_add, Some("notify".to_string()));
}

// ============================================================================
// Fail action parsing
// ============================================================================

#[test]
fn parse_fail_action_escalate() {
    assert!(matches!(
        parse_fail_action(Some("escalate")),
        FailAction::Escalate
    ));
    assert!(matches!(parse_fail_action(None), FailAction::Escalate));
}

#[test]
fn parse_fail_action_goto() {
    assert!(
        matches!(parse_fail_action(Some("cleanup")), FailAction::GotoPhase(p) if p == "cleanup")
    );
}

#[test]
fn parse_fail_action_strategy() {
    assert!(
        matches!(parse_fail_action(Some("strategy:recovery")), FailAction::UseStrategy(s) if s == "recovery")
    );
}

#[test]
fn parse_fail_action_retry() {
    if let FailAction::Retry { max, interval } = parse_fail_action(Some("retry:3")) {
        assert_eq!(max, 3);
        assert_eq!(interval, Duration::from_secs(60));
    } else {
        panic!("Expected Retry");
    }

    if let FailAction::Retry { max, interval } = parse_fail_action(Some("retry:5:30s")) {
        assert_eq!(max, 5);
        assert_eq!(interval, Duration::from_secs(30));
    } else {
        panic!("Expected Retry");
    }
}

// ============================================================================
// Duration parsing
// ============================================================================

#[test]
fn parse_duration_variants() {
    assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
    assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
    assert_eq!(parse_duration("2h"), Some(Duration::from_secs(7200)));
    assert_eq!(parse_duration("1d"), Some(Duration::from_secs(86400)));
    assert_eq!(parse_duration(""), None);
}

// ============================================================================
// Registry
// ============================================================================

#[test]
fn registry_add_and_get() {
    let mut registry = RunbookRegistry::new();

    let runbook = Runbook {
        name: "test".to_string(),
        ..Default::default()
    };

    registry.add("test", runbook);

    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
    assert!(registry.get("test").is_some());
    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn registry_resolve_task() {
    let mut registry = RunbookRegistry::new();

    let mut runbook = Runbook {
        name: "common".to_string(),
        ..Default::default()
    };

    runbook.tasks.insert(
        "hello".to_string(),
        TaskDef {
            name: "hello".to_string(),
            command: Some("echo hello".to_string()),
            prompt: None,
            prompt_file: None,
            env: BTreeMap::new(),
            cwd: None,
            heartbeat: None,
            timeout: None,
            idle_timeout: None,
            on_stuck: vec![],
            on_timeout: None,
            checkpoint_interval: None,
            checkpoint: None,
        },
    );

    registry.add("common", runbook);

    // Same runbook reference
    let task = registry.resolve_task("common", "task.hello").unwrap();
    assert_eq!(task.name, "hello");

    // Cross-runbook reference
    let task = registry.resolve_task("other", "common.task.hello").unwrap();
    assert_eq!(task.name, "hello");

    // Missing reference
    assert!(registry.resolve_task("common", "task.missing").is_none());
}

#[test]
fn registry_resolve_guard() {
    let mut registry = RunbookRegistry::new();

    let mut runbook = Runbook {
        name: "shared".to_string(),
        ..Default::default()
    };

    runbook.guards.insert(
        "ready".to_string(),
        GuardDef {
            name: "ready".to_string(),
            condition: Some("test -f ready.txt".to_string()),
            wake_on: vec![],
            timeout: None,
            on_timeout: None,
            retry_max: None,
            retry_interval: None,
        },
    );

    registry.add("shared", runbook);

    let guard = registry.resolve_guard("shared", "guard.ready").unwrap();
    assert_eq!(guard.condition, Some("test -f ready.txt".to_string()));

    let guard = registry
        .resolve_guard("other", "shared.guard.ready")
        .unwrap();
    assert_eq!(guard.name, "ready");
}

#[test]
fn registry_names() {
    let mut registry = RunbookRegistry::new();

    registry.add(
        "first",
        Runbook {
            name: "first".to_string(),
            ..Default::default()
        },
    );
    registry.add(
        "second",
        Runbook {
            name: "second".to_string(),
            ..Default::default()
        },
    );

    let names: Vec<&String> = registry.names().collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&&"first".to_string()));
    assert!(names.contains(&&"second".to_string()));
}

// ============================================================================
// Reference parsing
// ============================================================================

#[test]
fn parse_reference_same_runbook() {
    let (rb, name) = parse_reference("current", "task.hello").unwrap();
    assert_eq!(rb, "current");
    assert_eq!(name, "hello");
}

#[test]
fn parse_reference_cross_runbook() {
    let (rb, name) = parse_reference("current", "other.task.hello").unwrap();
    assert_eq!(rb, "other");
    assert_eq!(name, "hello");
}

#[test]
fn parse_reference_simple() {
    let (rb, name) = parse_reference("current", "hello").unwrap();
    assert_eq!(rb, "current");
    assert_eq!(name, "hello");
}

// ============================================================================
// Config loading
// ============================================================================

#[test]
fn load_config() {
    let raw = parse_runbook(
        r#"
[config]
timeout = "30m"
max_retries = 3
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    assert!(runbook.config.contains_key("timeout"));
    assert!(runbook.config.contains_key("max_retries"));
}

// ============================================================================
// Functions loading
// ============================================================================

#[test]
fn load_functions() {
    let raw = parse_runbook(
        r#"
[functions]
cleanup = "rm -rf /tmp/work"
setup = "mkdir -p /tmp/work"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let cleanup = runbook.functions.get("cleanup").unwrap();
    assert_eq!(cleanup.body, "rm -rf /tmp/work");

    let setup = runbook.functions.get("setup").unwrap();
    assert_eq!(setup.body, "mkdir -p /tmp/work");
}

// ============================================================================
// Scheduling primitives loading
// ============================================================================

#[test]
fn load_cron() {
    let raw = parse_runbook(
        r#"
[cron.daily]
interval = "24h"
enabled = true
run = "echo daily"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let cron = runbook.crons.get("daily").unwrap();
    assert_eq!(cron.name, "daily");
    assert_eq!(cron.interval, Duration::from_secs(24 * 3600));
    assert!(cron.enabled);
}

#[test]
fn load_cron_with_watchers_and_scanners() {
    let raw = parse_runbook(
        r#"
[action.nudge]
cooldown = "30s"
command = "echo nudge"

[watcher.agent-idle]
check_interval = "1m"

[watcher.agent-idle.source]
type = "session"

[watcher.agent-idle.condition]
type = "idle"
threshold = "5m"

[[watcher.agent-idle.response]]
action = "nudge"

[scanner.stale-locks]
interval = "10m"

[scanner.stale-locks.source]
type = "locks"

[scanner.stale-locks.condition]
type = "stale"
threshold = "1h"

[scanner.stale-locks.cleanup]
type = "release"

[cron.maintenance]
interval = "1h"
enabled = true
watchers = ["agent-idle"]
scanners = ["stale-locks"]
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let cron = runbook.crons.get("maintenance").unwrap();
    assert_eq!(cron.watchers.len(), 1);
    assert_eq!(cron.watchers[0].0, "agent-idle");
    assert_eq!(cron.scanners.len(), 1);
    assert_eq!(cron.scanners[0].0, "stale-locks");
}

#[test]
fn load_action_with_command() {
    let raw = parse_runbook(
        r#"
[action.nudge]
cooldown = "30s"
command = "send_to_session"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let action = runbook.actions.get("nudge").unwrap();
    assert_eq!(action.name, "nudge");
    assert_eq!(action.cooldown, Duration::from_secs(30));
    match &action.execution {
        crate::scheduling::ActionExecution::Command { run, .. } => {
            assert_eq!(run, "send_to_session");
        }
        _ => panic!("Expected Command execution"),
    }
}

#[test]
fn load_action_with_task() {
    let raw = parse_runbook(
        r#"
[action.analyze]
cooldown = "5m"
task = "analysis_task"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let action = runbook.actions.get("analyze").unwrap();
    match &action.execution {
        crate::scheduling::ActionExecution::Task { task, .. } => {
            assert_eq!(task, "analysis_task");
        }
        _ => panic!("Expected Task execution"),
    }
}

#[test]
fn load_watcher_with_responses() {
    let raw = parse_runbook(
        r#"
[action.nudge]
cooldown = "30s"
command = "echo nudge"

[action.restart]
cooldown = "5m"
command = "echo restart"

[watcher.agent-idle]
check_interval = "1m"

[watcher.agent-idle.source]
type = "session"
pattern = "*"

[watcher.agent-idle.condition]
type = "idle"
threshold = "5m"

[[watcher.agent-idle.response]]
action = "nudge"

[[watcher.agent-idle.response]]
action = "restart"
delay = "2m"
requires_previous_failure = true
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let watcher = runbook.watchers.get("agent-idle").unwrap();
    assert_eq!(watcher.name, "agent-idle");
    assert_eq!(watcher.check_interval, Duration::from_secs(60));
    assert_eq!(watcher.response_chain.len(), 2);

    // First response - immediate
    assert_eq!(watcher.response_chain[0].action.0, "nudge");
    assert_eq!(watcher.response_chain[0].delay, Duration::ZERO);
    assert!(!watcher.response_chain[0].requires_previous_failure);

    // Second response - delayed, requires previous failure
    assert_eq!(watcher.response_chain[1].action.0, "restart");
    assert_eq!(watcher.response_chain[1].delay, Duration::from_secs(120));
    assert!(watcher.response_chain[1].requires_previous_failure);
}

#[test]
fn load_watcher_source_types() {
    // Session source
    let raw = parse_runbook(
        r#"
[watcher.session-watcher]
check_interval = "1m"

[watcher.session-watcher.source]
type = "session"
pattern = "*"

[watcher.session-watcher.condition]
type = "idle"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let watcher = runbook.watchers.get("session-watcher").unwrap();
    match &watcher.source {
        crate::scheduling::WatcherSource::Session { name } => {
            assert_eq!(name, "*");
        }
        _ => panic!("Expected Session source"),
    }

    // Events source
    let raw = parse_runbook(
        r#"
[watcher.event-watcher]
check_interval = "1m"

[watcher.event-watcher.source]
type = "events"
pattern = "pipeline:failed:*"

[watcher.event-watcher.condition]
type = "consecutive_failures"
count = 3
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let watcher = runbook.watchers.get("event-watcher").unwrap();
    match &watcher.source {
        crate::scheduling::WatcherSource::Events { pattern } => {
            assert_eq!(pattern, "pipeline:failed:*");
        }
        _ => panic!("Expected Events source"),
    }
}

#[test]
fn load_watcher_condition_types() {
    // Idle condition
    let raw = parse_runbook(
        r#"
[watcher.idle-watcher]
check_interval = "1m"

[watcher.idle-watcher.source]
type = "session"

[watcher.idle-watcher.condition]
type = "idle"
threshold = "10m"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let watcher = runbook.watchers.get("idle-watcher").unwrap();
    match &watcher.condition {
        crate::scheduling::WatcherCondition::Idle { threshold } => {
            assert_eq!(*threshold, Duration::from_secs(600));
        }
        _ => panic!("Expected Idle condition"),
    }

    // Exceeds condition
    let raw = parse_runbook(
        r#"
[watcher.exceeds-watcher]
check_interval = "1m"

[watcher.exceeds-watcher.source]
type = "events"

[watcher.exceeds-watcher.condition]
type = "exceeds"
count = 50
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let watcher = runbook.watchers.get("exceeds-watcher").unwrap();
    match &watcher.condition {
        crate::scheduling::WatcherCondition::Exceeds { threshold } => {
            assert_eq!(*threshold, 50);
        }
        _ => panic!("Expected Exceeds condition"),
    }
}

#[test]
fn load_scanner() {
    let raw = parse_runbook(
        r#"
[scanner.stale-locks]
interval = "10m"

[scanner.stale-locks.source]
type = "locks"

[scanner.stale-locks.condition]
type = "stale"
threshold = "1h"

[scanner.stale-locks.cleanup]
type = "release"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let scanner = runbook.scanners.get("stale-locks").unwrap();
    assert_eq!(scanner.name, "stale-locks");
    assert_eq!(scanner.scan_interval, Duration::from_secs(600));

    match &scanner.source {
        crate::scheduling::ScannerSource::Locks => {}
        _ => panic!("Expected Locks source"),
    }

    match &scanner.condition {
        crate::scheduling::ScannerCondition::Stale { threshold } => {
            assert_eq!(*threshold, Duration::from_secs(3600));
        }
        _ => panic!("Expected Stale condition"),
    }

    match &scanner.cleanup_action {
        crate::scheduling::CleanupAction::Release => {}
        _ => panic!("Expected Release cleanup"),
    }
}

#[test]
fn load_scanner_source_types() {
    // Queue source
    let raw = parse_runbook(
        r#"
[scanner.queue-scanner]
interval = "5m"

[scanner.queue-scanner.source]
type = "queue"
name = "merges"

[scanner.queue-scanner.condition]
type = "exceeded_attempts"
max = 3

[scanner.queue-scanner.cleanup]
type = "dead_letter"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let scanner = runbook.scanners.get("queue-scanner").unwrap();
    match &scanner.source {
        crate::scheduling::ScannerSource::Queue { name } => {
            assert_eq!(name, "merges");
        }
        _ => panic!("Expected Queue source"),
    }

    // Worktrees source
    let raw = parse_runbook(
        r#"
[scanner.worktree-scanner]
interval = "30m"

[scanner.worktree-scanner.source]
type = "worktrees"

[scanner.worktree-scanner.condition]
type = "orphaned"

[scanner.worktree-scanner.cleanup]
type = "delete"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let scanner = runbook.scanners.get("worktree-scanner").unwrap();
    match &scanner.source {
        crate::scheduling::ScannerSource::Worktrees => {}
        _ => panic!("Expected Worktrees source"),
    }

    match &scanner.condition {
        crate::scheduling::ScannerCondition::Orphaned => {}
        _ => panic!("Expected Orphaned condition"),
    }

    match &scanner.cleanup_action {
        crate::scheduling::CleanupAction::Delete => {}
        _ => panic!("Expected Delete cleanup"),
    }
}

#[test]
fn load_scanner_cleanup_types() {
    // Archive cleanup
    let raw = parse_runbook(
        r#"
[scanner.archive-scanner]
interval = "1h"

[scanner.archive-scanner.source]
type = "pipelines"

[scanner.archive-scanner.condition]
type = "terminal_for"
threshold = "24h"

[scanner.archive-scanner.cleanup]
type = "archive"
destination = ".oj/archive"
"#,
    )
    .unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    let scanner = runbook.scanners.get("archive-scanner").unwrap();
    match &scanner.cleanup_action {
        crate::scheduling::CleanupAction::Archive { destination } => {
            assert_eq!(destination, ".oj/archive");
        }
        _ => panic!("Expected Archive cleanup"),
    }

    match &scanner.condition {
        crate::scheduling::ScannerCondition::TerminalFor { threshold } => {
            assert_eq!(*threshold, Duration::from_secs(86400));
        }
        _ => panic!("Expected TerminalFor condition"),
    }
}

// ============================================================================
// System runbook loading
// ============================================================================

#[test]
fn load_watchdog_runbook() {
    let content = include_str!("../../../../runbooks/watchdog.toml");
    let raw = parse_runbook(content).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    // Check actions loaded
    assert!(runbook.actions.contains_key("nudge"));
    assert!(runbook.actions.contains_key("restart"));
    assert!(runbook.actions.contains_key("escalate"));

    let nudge = runbook.actions.get("nudge").unwrap();
    assert_eq!(nudge.cooldown, Duration::from_secs(30));

    // Check watcher loaded
    assert!(runbook.watchers.contains_key("agent-idle"));

    let watcher = runbook.watchers.get("agent-idle").unwrap();
    assert_eq!(watcher.check_interval, Duration::from_secs(60));
    assert_eq!(watcher.response_chain.len(), 3);
}

#[test]
fn load_janitor_runbook() {
    let content = include_str!("../../../../runbooks/janitor.toml");
    let raw = parse_runbook(content).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    // Check scanners loaded
    assert!(runbook.scanners.contains_key("stale-locks"));
    assert!(runbook.scanners.contains_key("dead-queue-items"));
    assert!(runbook.scanners.contains_key("orphan-worktrees"));
    assert!(runbook.scanners.contains_key("old-completed-pipelines"));
    assert!(runbook.scanners.contains_key("stale-sessions"));

    let stale_locks = runbook.scanners.get("stale-locks").unwrap();
    assert_eq!(stale_locks.scan_interval, Duration::from_secs(600));
}

#[test]
fn load_triager_runbook() {
    let content = include_str!("../../../../runbooks/triager.toml");
    let raw = parse_runbook(content).unwrap();
    let validated = validate_runbook(&raw).unwrap();
    let runbook = load_runbook(&validated).unwrap();

    // Check actions loaded
    assert!(runbook.actions.contains_key("notify-team"));
    assert!(runbook.actions.contains_key("pause-pipelines"));
    assert!(runbook.actions.contains_key("analyze-failure"));

    // Check watchers loaded
    assert!(runbook.watchers.contains_key("build-failures"));
    assert!(runbook.watchers.contains_key("test-flakiness"));
    assert!(runbook.watchers.contains_key("merge-conflicts"));
    assert!(runbook.watchers.contains_key("resource-exhaustion"));

    // Check crons loaded
    assert!(runbook.crons.contains_key("daily-health-check"));
    assert!(runbook.crons.contains_key("weekly-cleanup-report"));

    let daily = runbook.crons.get("daily-health-check").unwrap();
    assert_eq!(daily.interval, Duration::from_secs(86400));
    assert!(daily.enabled);
}

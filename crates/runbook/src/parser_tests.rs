// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

// New format - uses RunDirective tables and args string syntax
const SAMPLE_RUNBOOK_NEW: &str = r#"
[command.build]
args = "<name> <prompt>"
run = { pipeline = "build" }
[command.build.defaults]
branch = "main"

[worker.builds]
concurrency = 1
pipelines = ["build"]

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "init"
run = "git worktree add worktrees/{name} -b feature/{name}"

[[pipeline.build.phase]]
name = "plan"
run = { agent = "planner" }

[[pipeline.build.phase]]
name = "execute"
run = { agent = "executor" }
next = "done"
on_fail = "failed"

[[pipeline.build.phase]]
name = "done"
run = "echo done"

[[pipeline.build.phase]]
name = "failed"
run = "echo failed"

[agent.planner]
run = "claude -p \"Plan: {prompt}\""
[agent.planner.env]
OJ_PIPELINE = "{pipeline_id}"

[agent.executor]
run = "claude --print-plan {plan_file}"
cwd = "worktrees/{name}"
"#;

// Legacy format - uses old struct args and legacy phase.agent field
const SAMPLE_RUNBOOK_LEGACY: &str = r#"
[command.build]
run = { pipeline = "build" }
[command.build.args]
positional = ["name", "prompt"]
[command.build.defaults]
branch = "main"

[worker.builds]
concurrency = 1
pipelines = ["build"]

[pipeline.build]
inputs = ["name", "prompt"]
[[pipeline.build.phases]]
name = "init"
run = "git worktree add worktrees/{name} -b feature/{name}"

[[pipeline.build.phases]]
name = "plan"
agent = "planner"

[[pipeline.build.phases]]
name = "execute"
agent = "executor"
next = "done"
on_fail = "failed"

[[pipeline.build.phases]]
name = "done"
run = "echo done"

[[pipeline.build.phases]]
name = "failed"
run = "echo failed"

[agent.planner]
run = "claude -p \"Plan: {prompt}\""
[agent.planner.env]
OJ_PIPELINE = "{pipeline_id}"

[agent.executor]
run = "claude --print-plan {plan_file}"
cwd = "worktrees/{name}"
"#;

#[test]
fn parse_new_format_runbook() {
    let runbook = parse_runbook(SAMPLE_RUNBOOK_NEW).unwrap();

    // Commands
    assert!(runbook.commands.contains_key("build"));
    let cmd = &runbook.commands["build"];
    assert!(cmd.run.is_pipeline());
    assert_eq!(cmd.run.pipeline_name(), Some("build"));
    assert_eq!(cmd.args.positional.len(), 2);
    assert_eq!(cmd.args.positional[0].name, "name");
    assert_eq!(cmd.args.positional[1].name, "prompt");
    assert_eq!(cmd.defaults.get("branch"), Some(&"main".to_string()));

    // Workers
    assert!(runbook.workers.contains_key("builds"));
    let worker = &runbook.workers["builds"];
    assert_eq!(worker.concurrency, 1);
    assert!(worker.pipelines.contains(&"build".to_string()));

    // Pipelines
    assert!(runbook.pipelines.contains_key("build"));
    let pipeline = &runbook.pipelines["build"];
    assert_eq!(pipeline.inputs, vec!["name", "prompt"]);
    assert_eq!(pipeline.phases.len(), 5);

    // Phase checks
    assert_eq!(pipeline.phases[0].name, "init");
    assert!(pipeline.phases[0].run.is_shell());

    assert_eq!(pipeline.phases[1].name, "plan");
    assert!(pipeline.phases[1].run.is_agent());
    assert_eq!(pipeline.phases[1].agent_name(), Some("planner"));

    // Agents
    assert!(runbook.agents.contains_key("planner"));
    let agent = &runbook.agents["planner"];
    assert!(agent.run.contains("claude"));
    assert!(agent.env.contains_key("OJ_PIPELINE"));
}

#[test]
fn parse_legacy_format_runbook() {
    let runbook = parse_runbook(SAMPLE_RUNBOOK_LEGACY).unwrap();

    // Commands with legacy args format
    let cmd = &runbook.commands["build"];
    assert!(cmd.run.is_pipeline());
    assert_eq!(cmd.args.positional.len(), 2);
    assert_eq!(cmd.args.positional[0].name, "name");

    // Pipelines with legacy agent field
    let pipeline = &runbook.pipelines["build"];
    assert_eq!(pipeline.phases.len(), 5);

    // Legacy agent field converted to RunDirective::Agent
    assert!(pipeline.phases[1].run.is_agent());
    assert_eq!(pipeline.phases[1].agent_name(), Some("planner"));
}

#[test]
fn parse_empty_runbook() {
    let runbook = parse_runbook("").unwrap();
    assert!(runbook.commands.is_empty());
    assert!(runbook.pipelines.is_empty());
}

#[test]
fn parse_command_with_args_string() {
    let toml = r#"
[command.deploy]
args = "<env> [-t/--tag <version>] [-f/--force] [targets...]"
run = "deploy.sh"
[command.deploy.defaults]
tag = "latest"
"#;
    let runbook = parse_runbook(toml).unwrap();
    let cmd = &runbook.commands["deploy"];

    assert_eq!(cmd.args.positional.len(), 1);
    assert_eq!(cmd.args.positional[0].name, "env");
    assert_eq!(cmd.args.options.len(), 1);
    assert_eq!(cmd.args.options[0].name, "tag");
    assert_eq!(cmd.args.flags.len(), 1);
    assert_eq!(cmd.args.flags[0].name, "force");
    assert!(cmd.args.variadic.is_some());
    assert_eq!(cmd.args.variadic.as_ref().unwrap().name, "targets");

    assert!(cmd.run.is_shell());
    assert_eq!(cmd.run.shell_command(), Some("deploy.sh"));
}

#[test]
fn parse_build_minimal_toml() {
    // Integration test: parse the documented example runbook
    let content = include_str!("../../../docs/10-runbooks/build.minimal.toml");
    let runbook = parse_runbook(content).unwrap();

    // Verify command
    let cmd = runbook
        .get_command("build")
        .expect("build command should exist");
    assert_eq!(cmd.args.positional.len(), 2);
    assert_eq!(cmd.args.positional[0].name, "name");
    assert_eq!(cmd.args.positional[1].name, "prompt");
    assert!(cmd.run.is_pipeline());
    assert_eq!(cmd.run.pipeline_name(), Some("build"));

    // Verify pipeline
    let pipeline = runbook
        .get_pipeline("build")
        .expect("build pipeline should exist");
    assert_eq!(pipeline.phases.len(), 5);

    // Phase 0: init - shell command
    assert_eq!(pipeline.phases[0].name, "init");
    assert!(pipeline.phases[0].run.is_shell());
    assert!(pipeline.phases[0].shell_command().unwrap().contains("echo"));

    // Phase 1: plan - agent
    assert_eq!(pipeline.phases[1].name, "plan");
    assert!(pipeline.phases[1].run.is_agent());
    assert_eq!(pipeline.phases[1].agent_name(), Some("planning"));

    // Phase 2: execute - agent
    assert_eq!(pipeline.phases[2].name, "execute");
    assert!(pipeline.phases[2].run.is_agent());
    assert_eq!(pipeline.phases[2].agent_name(), Some("execution"));

    // Phase 3: merge - shell command
    assert_eq!(pipeline.phases[3].name, "merge");
    assert!(pipeline.phases[3].run.is_shell());

    // Phase 4: done - shell command
    assert_eq!(pipeline.phases[4].name, "done");
    assert!(pipeline.phases[4].run.is_shell());

    // Verify agents
    let planning = runbook
        .get_agent("planning")
        .expect("planning agent should exist");
    assert!(planning.run.contains("claude"));

    let execution = runbook
        .get_agent("execution")
        .expect("execution agent should exist");
    assert!(execution.run.contains("claude"));
}

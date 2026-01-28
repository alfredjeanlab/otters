//! Agent configuration integration tests.
//!
//! Verify agent configuration is correctly loaded and accessible at runtime.

use crate::prelude::*;

const SCENARIO_SIMPLE: &str = r#"
name = "simple"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "Done."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"
tools.Bash.auto_approve = true
"#;

/// Generate runbook with full agent action configuration.
/// Tests all action config syntax variants:
/// - Simple action string: on_exit = "escalate"
/// - Action with message: on_idle = { action = "nudge", message = "..." }
/// - Per-error actions: [[agent.name.on_error]] with match field
fn runbook_with_action_config(scenario_path: &std::path::Path) -> String {
    format!(
        r#"
[command.build]
args = "<name> <prompt>"
run = {{ pipeline = "build" }}

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "execute"
run = "claudeless --scenario {} --print '{{prompt}}'"

[agent.worker]
run = "claudeless --scenario {} --print '{{prompt}}'"
prompt = "Do the task."
on_idle = {{ action = "nudge", message = "Keep going, remember oj done" }}
on_exit = "escalate"

[agent.worker.env]
OJ_PIPELINE = "{{pipeline_id}}"

[[agent.worker.on_error]]
match = "no_internet"
action = "recover"
message = "Network restored, try again."

[[agent.worker.on_error]]
match = "rate_limited"
action = "recover"
message = "Rate limit cleared."

[[agent.worker.on_error]]
action = "escalate"
"#,
        scenario_path.display(),
        scenario_path.display()
    )
}

/// Verify that runbooks with agent action configurations can be parsed and loaded.
/// The daemon should start without errors when the runbook contains:
/// - on_idle, on_exit with simple and complex syntax
/// - on_error with per-error-type matching
#[test]
fn runbook_with_agent_config_loads() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/simple.toml", SCENARIO_SIMPLE);

    let scenario_path = temp.path().join(".oj/scenarios/simple.toml");
    let runbook = runbook_with_action_config(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    // Daemon should start without errors (config parses correctly)
    temp.oj().args(&["daemon", "start"]).passes();

    // Pipeline should run to verify agent config doesn't break runtime
    temp.oj().args(&["run", "build", "test", "Task"]).passes();

    // Wait for pipeline to complete (uses direct shell command, not agent spawn)
    let done = wait_for(SPEC_WAIT_MAX_MS, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should complete");
}

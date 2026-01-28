//! Agent spawn tests - verifies agent session spawning via tmux works correctly.
//!
//! These tests exercise the Effect::Spawn -> TmuxAdapter::spawn() path,
//! which is triggered by `run = { agent = "..." }` directives.

use crate::prelude::*;

/// Agent scenario that immediately succeeds
const SCENARIO_SPAWN_SUCCESS: &str = r#"
name = "spawn-success"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "I'll complete this task now."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"

[tool_execution.tools.Bash]
auto_approve = true
"#;

/// Generate runbook with absolute scenario path embedded
fn agent_spawn_runbook(scenario_path: &std::path::Path) -> String {
    format!(
        r#"
[command.build]
args = "<name>"
run = {{ pipeline = "build" }}

[pipeline.build]
inputs = ["name"]

[[pipeline.build.phase]]
name = "work"
run = {{ agent = "worker" }}

[agent.worker]
run = "claudeless --scenario {} -p 'Complete the task and call oj done.'"
prompt = "Complete the task and call oj done."
env = {{ OJ_PIPELINE = "{{pipeline_id}}" }}
"#,
        scenario_path.display()
    )
}

/// Verifies that the agent spawn flow works correctly:
/// - Pipeline starts and reaches agent phase
/// - Effect::Spawn creates tmux session via TmuxAdapter
/// - Workspace is prepared with CLAUDE.md
/// - Agent (claudeless) runs in tmux session
/// - Agent calls `oj done` to signal completion
/// - Pipeline advances to done status
#[test]
fn agent_spawn_creates_session_and_completes() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/spawn.toml", SCENARIO_SPAWN_SUCCESS);

    // Generate runbook with absolute path to scenario
    let scenario_path = temp.path().join(".oj/scenarios/spawn.toml");
    let runbook = agent_spawn_runbook(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    // Start daemon
    temp.oj().args(&["daemon", "start"]).passes();

    // Run pipeline - this triggers agent spawn
    temp.oj().args(&["run", "build", "spawn-test"]).passes();

    // Wait for pipeline to complete (verifies full spawn path worked)
    let done = wait_for(SPEC_WAIT_MAX_MS, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("Completed")
    });
    assert!(done, "pipeline should complete via agent spawn path");

    // Verify final state
    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("Completed");
}

//! Agent session monitoring tests using claudeless simulator.
//!
//! Tests for on_idle, on_exit, and on_error action handling.

use crate::prelude::*;

/// Agent that stops at end_turn (no tool calls)
const SCENARIO_END_TURN: &str = r#"
name = "end-turn"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "I've analyzed the task and here's my response."
"#;

/// Agent that calls oj done after being nudged
const SCENARIO_NUDGE_THEN_DONE: &str = r#"
name = "nudge-then-done"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "I'm thinking about this..."

[[responses]]
pattern = { type = "user_message", contains = "continue" }

[responses.response]
text = "Ah, right! Let me complete this."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"

[tool_execution.tools.Bash]
auto_approve = true
"#;

/// Runbook with agent that has on_idle = done (completes when idle)
fn runbook_idle_done(scenario_path: &std::path::Path) -> String {
    format!(
        r#"
[command.build]
args = "<name> <prompt>"
run = {{ pipeline = "build" }}

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "execute"
run = {{ agent = "worker" }}

[agent.worker]
run = "claudeless --scenario {} --print '{{prompt}}'"
on_idle = "done"
"#,
        scenario_path.display()
    )
}

/// Runbook with agent that has on_idle = nudge (sends continue message)
fn runbook_idle_nudge(scenario_path: &std::path::Path) -> String {
    format!(
        r#"
[command.build]
args = "<name> <prompt>"
run = {{ pipeline = "build" }}

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "execute"
run = {{ agent = "worker" }}

[agent.worker]
run = "claudeless --scenario {} --print '{{prompt}}'"
on_idle = "nudge"
"#,
        scenario_path.display()
    )
}

/// Tests that on_idle = done completes the pipeline when agent finishes naturally
#[test]
#[ignore = "requires session log monitoring integration"]
fn on_idle_done_completes_pipeline() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/end-turn.toml", SCENARIO_END_TURN);

    let scenario_path = temp.path().join(".oj/scenarios/end-turn.toml");
    let runbook = runbook_idle_done(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["run", "build", "test-feature", "Do something"])
        .passes();

    // Pipeline should complete because on_idle = done treats end_turn as success
    let done = wait_for(SPEC_WAIT_MAX_MS * 3, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should complete via on_idle = done");
}

/// Tests that on_idle = nudge sends a message prompting the agent to continue
#[test]
#[ignore = "requires session log monitoring integration"]
fn on_idle_nudge_sends_continue_message() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(
        ".oj/scenarios/nudge-then-done.toml",
        SCENARIO_NUDGE_THEN_DONE,
    );

    let scenario_path = temp.path().join(".oj/scenarios/nudge-then-done.toml");
    let runbook = runbook_idle_nudge(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["run", "build", "test-feature", "Do something"])
        .passes();

    // Pipeline should complete after nudge triggers second response
    let done = wait_for(SPEC_WAIT_MAX_MS * 3, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(
        done,
        "pipeline should complete after nudge triggers oj done"
    );
}

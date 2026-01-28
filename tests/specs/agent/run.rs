//! Agent execution tests using claudeless simulator.

use crate::prelude::*;

/// Agent responds and calls `oj done`
const SCENARIO_SUCCESS: &str = r#"
name = "simple-success"

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

fn agent_runbook(scenario_path: &std::path::Path) -> String {
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
"#,
        scenario_path.display()
    )
}

#[test]
fn agent_completes_simple_task() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/success.toml", SCENARIO_SUCCESS);

    let scenario_path = temp.path().join(".oj/scenarios/success.toml");
    let runbook = agent_runbook(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["run", "build", "test-feature", "Do something simple"])
        .passes();

    let done = wait_for(SPEC_WAIT_MAX_MS, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should complete via claudeless");

    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("done")
        .stdout_has("Completed");
}

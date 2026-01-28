//! Agent error handling tests using claudeless simulator.

use crate::prelude::*;

/// First request rate limited, then succeeds
const SCENARIO_RATE_LIMIT: &str = r#"
name = "rate-limit-recovery"

[[responses]]
pattern = { type = "any" }
failure = { type = "rate_limit", retry_after = 1 }
max_matches = 1

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "Recovered from rate limit. Completing task."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"

[tool_execution.tools.Bash]
auto_approve = true
"#;

/// All requests fail with network error
const SCENARIO_NETWORK_FAILURE: &str = r#"
name = "network-failure"

[[responses]]
pattern = { type = "any" }
failure = { type = "network_unreachable" }
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
#[ignore = "requires retry logic in engine"]
fn agent_recovers_from_rate_limit() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/rate-limit.toml", SCENARIO_RATE_LIMIT);

    let scenario_path = temp.path().join(".oj/scenarios/rate-limit.toml");
    let runbook = agent_runbook(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["run", "build", "test-feature", "Rate limited task"])
        .passes();

    let done = wait_for(SPEC_WAIT_MAX_MS * 5, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should complete after rate limit recovery");
}

#[test]
fn agent_network_failure_marks_pipeline_failed() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(
        ".oj/scenarios/network-failure.toml",
        SCENARIO_NETWORK_FAILURE,
    );

    let scenario_path = temp.path().join(".oj/scenarios/network-failure.toml");
    let runbook = agent_runbook(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj()
        .args(&["run", "build", "test-feature", "Network fail task"])
        .passes();

    let failed = wait_for(SPEC_WAIT_MAX_MS * 5, || {
        let output = temp.oj().args(&["pipeline", "list"]).passes().stdout();
        output.contains("failed") || output.contains("Failed")
    });
    assert!(failed, "pipeline should fail after network errors");
}

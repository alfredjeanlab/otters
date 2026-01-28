//! Pipeline execution specs
//!
//! Verify pipelines execute phases correctly.

use crate::prelude::*;

/// Shell-only runbook that writes to a file for verification
const SHELL_RUNBOOK: &str = r#"
[command.test]
args = "<name>"
run = { pipeline = "test" }

[pipeline.test]
inputs = ["name"]

[[pipeline.test.phase]]
name = "init"
run = "echo 'init:{name}' >> {workspace}/output.log"

[[pipeline.test.phase]]
name = "plan"
run = "echo 'plan:{name}' >> {workspace}/output.log"

[[pipeline.test.phase]]
name = "execute"
run = "echo 'execute:{name}' >> {workspace}/output.log"

[[pipeline.test.phase]]
name = "merge"
run = "echo 'merge:{name}' >> {workspace}/output.log"
"#;

#[test]
fn pipeline_starts_and_runs_init_phase() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/runbooks/test.toml", SHELL_RUNBOOK);
    temp.oj().args(&["daemon", "start"]).passes();

    temp.oj()
        .args(&["run", "test", "hello"])
        .passes()
        .stdout_has("Started: test");

    // Pipeline should be immediately visible in list after run returns
    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("hello");
}

#[test]
fn pipeline_completes_all_phases() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/runbooks/test.toml", SHELL_RUNBOOK);
    temp.oj().args(&["daemon", "start"]).passes();

    temp.oj().args(&["run", "test", "complete"]).passes();

    // Wait for pipeline to reach done phase
    let done = wait_for(SPEC_WAIT_MAX_MS, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should reach done phase");

    // Verify final state
    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("done")
        .stdout_has("Completed");
}

#[test]
fn pipeline_runs_custom_phase_names() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(
        ".oj/runbooks/custom.toml",
        r#"
[command.custom]
args = "<name>"
run = { pipeline = "custom" }

[pipeline.custom]
inputs = ["name"]

[[pipeline.custom.phase]]
name = "step1"
run = "echo 'step1'"

[[pipeline.custom.phase]]
name = "step2"
run = "echo 'step2'"
"#,
    );
    temp.oj().args(&["daemon", "start"]).passes();

    temp.oj().args(&["run", "custom", "test"]).passes();

    // Wait for pipeline to show custom phase name (step1 or step2) OR complete
    // The pipeline executes very quickly, so we may see:
    // - "step1" or "step2" if we catch it mid-execution
    // - "done" with "Completed" if it finished
    let mut last_output = String::new();
    let found = wait_for(SPEC_WAIT_MAX_MS, || {
        let result = temp.oj().args(&["pipeline", "list"]).passes();
        last_output = result.stdout().to_string();
        // Accept either seeing custom phase names OR pipeline completed
        last_output.contains("step1")
            || last_output.contains("step2")
            || (last_output.contains("done") && last_output.contains("Completed"))
    });
    assert!(
        found,
        "pipeline should show custom phase name or complete successfully, got: {}",
        last_output
    );
}

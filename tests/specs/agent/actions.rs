//! Agent action handling tests.
//!
//! Tests for on_idle, on_exit, and on_error action handlers including
//! nudge, done, fail, restart, recover, and escalate.
//!
//! NOTE: Many tests are marked #[ignore] because they require claudeless
//! to actually exit the process, which is not currently supported.
//! The claudeless `exit_code` field simulates API response behavior,
//! not process termination.

use crate::prelude::*;

// Agent exits without calling oj done
#[allow(dead_code)]
const SCENARIO_EXIT_SILENT: &str = r#"
name = "exit-silent"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "I'm done here."
exit_code = 0
"#;

// Runbook with on_exit = "done" (trust agent)
fn runbook_exit_done(scenario_path: &std::path::Path) -> String {
    format!(
        r#"
[command.build]
args = "<name> <prompt>"
run = {{ pipeline = "build" }}

[pipeline.build]
inputs = ["name", "prompt"]

[[pipeline.build.phase]]
name = "execute"
run = {{ agent = "trusted" }}

[agent.trusted]
run = "claudeless --scenario {} --print '{{prompt}}'"
prompt = "Do a simple task."
on_idle = "nudge"
on_exit = "done"
on_error = "escalate"
"#,
        scenario_path.display()
    )
}

// Runbook with on_exit = "escalate" (requires human intervention)
fn runbook_exit_escalate(scenario_path: &std::path::Path) -> String {
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
prompt = "Complete the task."
on_idle = "nudge"
on_exit = "escalate"
on_error = "escalate"
"#,
        scenario_path.display()
    )
}

#[test]
#[ignore = "requires claudeless to support process termination (not just API exit_code)"]
fn on_exit_done_treats_exit_as_success() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/exit.toml", SCENARIO_EXIT_SILENT);

    let scenario_path = temp.path().join(".oj/scenarios/exit.toml");
    let runbook = runbook_exit_done(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj().args(&["run", "build", "test", "Task"]).passes();

    // on_exit = "done" should complete pipeline even without oj done
    // Session check runs every 10 seconds, so wait at least 15 seconds
    let done = wait_for(15000, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("done")
    });
    assert!(done, "pipeline should complete via on_exit=done");
}

#[test]
#[ignore = "requires claudeless to support process termination (not just API exit_code)"]
fn on_exit_escalate_sets_waiting_status() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/exit.toml", SCENARIO_EXIT_SILENT);

    let scenario_path = temp.path().join(".oj/scenarios/exit.toml");
    let runbook = runbook_exit_escalate(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj().args(&["run", "build", "test", "Task"]).passes();

    // on_exit = "escalate" should set status to Waiting
    // Session check runs every 10 seconds, so wait at least 15 seconds
    let waiting = wait_for(15000, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("Waiting")
    });
    assert!(
        waiting,
        "pipeline should be in Waiting status after escalation"
    );
}

#[test]
#[ignore = "requires pipeline fail command implementation in daemon server"]
fn pipeline_fail_marks_pipeline_failed() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/exit.toml", SCENARIO_EXIT_SILENT);

    let scenario_path = temp.path().join(".oj/scenarios/exit.toml");
    let runbook = runbook_exit_escalate(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj().args(&["run", "build", "test", "Task"]).passes();

    // Wait for escalation
    let waiting = wait_for(SPEC_WAIT_MAX_MS * 3, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("Waiting")
    });
    assert!(waiting, "pipeline should escalate");

    // Manually fail the pipeline
    temp.oj()
        .args(&["pipeline", "fail", "test", "--error", "manual failure"])
        .passes();

    // Pipeline should now be failed
    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("Failed");
}

#[test]
#[ignore = "requires pipeline resume command implementation in daemon server"]
fn pipeline_resume_restarts_monitoring() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/scenarios/exit.toml", SCENARIO_EXIT_SILENT);

    let scenario_path = temp.path().join(".oj/scenarios/exit.toml");
    let runbook = runbook_exit_escalate(&scenario_path);
    temp.file(".oj/runbooks/build.toml", &runbook);

    temp.oj().args(&["daemon", "start"]).passes();
    temp.oj().args(&["run", "build", "test", "Task"]).passes();

    // Wait for escalation
    let waiting = wait_for(SPEC_WAIT_MAX_MS * 3, || {
        temp.oj()
            .args(&["pipeline", "list"])
            .passes()
            .stdout()
            .contains("Waiting")
    });
    assert!(waiting, "pipeline should escalate");

    // Resume the pipeline
    temp.oj().args(&["pipeline", "resume", "test"]).passes();

    // Pipeline should now be Running again
    temp.oj()
        .args(&["pipeline", "list"])
        .passes()
        .stdout_has("Running");
}

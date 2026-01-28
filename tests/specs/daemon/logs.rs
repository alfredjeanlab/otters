//! Daemon logs specs
//!
//! Verify daemon logs command behavior.

use crate::prelude::*;

#[test]
fn daemon_logs_shows_output() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/runbooks/build.toml", MINIMAL_RUNBOOK);
    temp.oj().args(&["daemon", "start"]).passes();

    temp.oj()
        .args(&["daemon", "logs", "--lines", "10"])
        .passes()
        .stdout_has("ojd: starting");
}

#[test]
fn daemon_logs_shows_startup_info() {
    let temp = Project::empty();
    temp.git_init();
    temp.file(".oj/runbooks/build.toml", MINIMAL_RUNBOOK);
    temp.oj().args(&["daemon", "start"]).passes();

    temp.oj()
        .args(&["daemon", "logs"])
        .passes()
        .stdout_has("Daemon ready");
}

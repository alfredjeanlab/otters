//! Behavioral specifications for oj CLI.
//!
//! These tests are black-box: they invoke the CLI binary and verify
//! stdout, stderr, and exit codes. See tests/specs/CLAUDE.md for conventions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "specs/prelude.rs"]
mod prelude;

// cli/
#[path = "specs/cli/errors.rs"]
mod cli_errors;
#[path = "specs/cli/help.rs"]
mod cli_help;

// project/
#[path = "specs/project/setup.rs"]
mod project_setup;

// daemon/
#[path = "specs/daemon/lifecycle.rs"]
mod daemon_lifecycle;
#[path = "specs/daemon/logs.rs"]
mod daemon_logs;

// pipeline/
#[path = "specs/pipeline/execution.rs"]
mod pipeline_execution;
#[path = "specs/pipeline/show.rs"]
mod pipeline_show;

// agent/
#[path = "specs/agent/actions.rs"]
mod agent_actions;
#[path = "specs/agent/config.rs"]
mod agent_config;
#[path = "specs/agent/error.rs"]
mod agent_error;
#[path = "specs/agent/monitoring.rs"]
mod agent_monitoring;
#[path = "specs/agent/run.rs"]
mod agent_run;
#[path = "specs/agent/spawn.rs"]
mod agent_spawn;

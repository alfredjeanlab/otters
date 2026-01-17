//! Bugfix pipeline configuration
//!
//! Phases: init → fix → verify → merge → cleanup → done

use super::{PhaseConfig, TaskConfig};
use crate::pipeline::Phase;
use std::time::Duration;

/// Bugfix pipeline configuration
pub struct BugfixPipeline;

impl BugfixPipeline {
    /// Get the configuration for a given phase
    pub fn phase_config(phase: &Phase) -> Option<PhaseConfig> {
        match phase {
            Phase::Init => Some(PhaseConfig {
                run: Some(vec![
                    "git worktree add {workspace} -b {branch}".to_string(),
                    "wk start {issue_id}".to_string(),
                ]),
                task: None,
                next: Phase::Fix,
            }),
            Phase::Fix => Some(PhaseConfig {
                run: None,
                task: Some(TaskConfig {
                    command: "claude".to_string(),
                    prompt_file: Some("templates/bugfix.md".to_string()),
                    timeout: Duration::from_secs(60 * 60),
                    idle_timeout: Duration::from_secs(5 * 60),
                }),
                next: Phase::Verify,
            }),
            Phase::Verify => Some(PhaseConfig {
                run: Some(vec![
                    "cargo test".to_string(),
                    "cargo clippy".to_string(),
                ]),
                task: None,
                next: Phase::Merge,
            }),
            Phase::Merge => Some(PhaseConfig {
                run: Some(vec![
                    "git add .".to_string(),
                    "git commit -m \"Fix {issue_id}\"".to_string(),
                    "oj queue add merges branch={branch}".to_string(),
                ]),
                task: None,
                next: Phase::Cleanup,
            }),
            Phase::Cleanup => Some(PhaseConfig {
                run: Some(vec![
                    "wk done {issue_id}".to_string(),
                    "git worktree remove {workspace}".to_string(),
                ]),
                task: None,
                next: Phase::Done,
            }),
            Phase::Done => None,
            Phase::Failed { .. } => None,
            Phase::Blocked { .. } => None,
            // Build phases not used in bugfix pipeline
            Phase::Plan | Phase::Decompose | Phase::Execute => None,
        }
    }

    /// Get all phases in order
    pub fn phases() -> Vec<Phase> {
        vec![
            Phase::Init,
            Phase::Fix,
            Phase::Verify,
            Phase::Merge,
            Phase::Cleanup,
            Phase::Done,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_phase_has_run_commands() {
        let config = BugfixPipeline::phase_config(&Phase::Init).unwrap();
        assert!(config.run.is_some());
        assert!(config.task.is_none());
        assert_eq!(config.next, Phase::Fix);
    }

    #[test]
    fn fix_phase_has_task() {
        let config = BugfixPipeline::phase_config(&Phase::Fix).unwrap();
        assert!(config.run.is_none());
        assert!(config.task.is_some());
        assert_eq!(config.next, Phase::Verify);
    }

    #[test]
    fn verify_phase_runs_tests() {
        let config = BugfixPipeline::phase_config(&Phase::Verify).unwrap();
        assert!(config.run.is_some());
        let commands = config.run.unwrap();
        assert!(commands.iter().any(|c| c.contains("cargo test")));
    }

    #[test]
    fn cleanup_phase_removes_worktree() {
        let config = BugfixPipeline::phase_config(&Phase::Cleanup).unwrap();
        assert!(config.run.is_some());
        let commands = config.run.unwrap();
        assert!(commands.iter().any(|c| c.contains("worktree remove")));
    }

    #[test]
    fn phases_are_in_correct_order() {
        let phases = BugfixPipeline::phases();
        assert_eq!(phases[0], Phase::Init);
        assert_eq!(phases[1], Phase::Fix);
        assert_eq!(phases[2], Phase::Verify);
        assert_eq!(phases[3], Phase::Merge);
        assert_eq!(phases[4], Phase::Cleanup);
        assert_eq!(phases[5], Phase::Done);
    }
}

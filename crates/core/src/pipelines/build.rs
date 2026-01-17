//! Build pipeline configuration
//!
//! Phases: init → plan → decompose → execute → merge → done

use super::{PhaseConfig, TaskConfig};
use crate::pipeline::Phase;
use std::time::Duration;

/// Build pipeline configuration
pub struct BuildPipeline;

impl BuildPipeline {
    /// Get the configuration for a given phase
    pub fn phase_config(phase: &Phase) -> Option<PhaseConfig> {
        match phase {
            Phase::Init => Some(PhaseConfig {
                run: Some(vec![
                    "git worktree add {workspace} -b {branch}".to_string(),
                    "wk new feature \"{prompt}\" -l plan:{name}".to_string(),
                ]),
                task: None,
                next: Phase::Plan,
            }),
            Phase::Plan => Some(PhaseConfig {
                run: None,
                task: Some(TaskConfig {
                    command: "claude --print".to_string(),
                    prompt_file: Some("templates/plan.md".to_string()),
                    timeout: Duration::from_secs(30 * 60),
                    idle_timeout: Duration::from_secs(2 * 60),
                }),
                next: Phase::Decompose,
            }),
            Phase::Decompose => Some(PhaseConfig {
                run: None,
                task: Some(TaskConfig {
                    command: "claude --print".to_string(),
                    prompt_file: Some("templates/decompose.md".to_string()),
                    timeout: Duration::from_secs(30 * 60),
                    idle_timeout: Duration::from_secs(2 * 60),
                }),
                next: Phase::Execute,
            }),
            Phase::Execute => Some(PhaseConfig {
                run: None,
                task: Some(TaskConfig {
                    command: "claude".to_string(),
                    prompt_file: Some("templates/execute.md".to_string()),
                    timeout: Duration::from_secs(60 * 60),
                    idle_timeout: Duration::from_secs(5 * 60),
                }),
                next: Phase::Merge,
            }),
            Phase::Merge => Some(PhaseConfig {
                run: Some(vec![
                    "git add .".to_string(),
                    "git commit -m \"Complete {name}\"".to_string(),
                    "oj queue add merges branch={branch}".to_string(),
                ]),
                task: None,
                next: Phase::Done,
            }),
            Phase::Done => None,
            Phase::Failed { .. } => None,
            _ => None,
        }
    }

    /// Get all phases in order
    pub fn phases() -> Vec<Phase> {
        vec![
            Phase::Init,
            Phase::Plan,
            Phase::Decompose,
            Phase::Execute,
            Phase::Merge,
            Phase::Done,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_phase_has_run_commands() {
        let config = BuildPipeline::phase_config(&Phase::Init).unwrap();
        assert!(config.run.is_some());
        assert!(config.task.is_none());
        assert_eq!(config.next, Phase::Plan);
    }

    #[test]
    fn plan_phase_has_task() {
        let config = BuildPipeline::phase_config(&Phase::Plan).unwrap();
        assert!(config.run.is_none());
        assert!(config.task.is_some());
        assert_eq!(config.next, Phase::Decompose);
    }

    #[test]
    fn done_phase_has_no_config() {
        let config = BuildPipeline::phase_config(&Phase::Done);
        assert!(config.is_none());
    }

    #[test]
    fn phases_are_in_correct_order() {
        let phases = BuildPipeline::phases();
        assert_eq!(phases[0], Phase::Init);
        assert_eq!(phases[1], Phase::Plan);
        assert_eq!(phases[2], Phase::Decompose);
        assert_eq!(phases[3], Phase::Execute);
        assert_eq!(phases[4], Phase::Merge);
        assert_eq!(phases[5], Phase::Done);
    }
}

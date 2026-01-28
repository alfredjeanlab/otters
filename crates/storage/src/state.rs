// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Materialized state from WAL replay

use oj_core::{Operation, Pipeline, Worker};
use std::collections::HashMap;
use std::path::PathBuf;

/// Session record
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub pipeline_id: String,
}

/// Workspace record
#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub path: PathBuf,
    pub branch: String,
}

/// Materialized state built from WAL operations
#[derive(Debug, Default)]
pub struct MaterializedState {
    pub pipelines: HashMap<String, Pipeline>,
    pub sessions: HashMap<String, Session>,
    pub workspaces: HashMap<String, Workspace>,
    pub workers: HashMap<String, Worker>,
}

impl MaterializedState {
    /// Get a pipeline by ID or unique prefix (like git commit hashes)
    pub fn get_pipeline(&self, id: &str) -> Option<&Pipeline> {
        // Try exact match first
        if let Some(pipeline) = self.pipelines.get(id) {
            return Some(pipeline);
        }

        // Try prefix match
        let matches: Vec<_> = self
            .pipelines
            .iter()
            .filter(|(k, _)| k.starts_with(id))
            .collect();

        // Only return if exactly one match (unambiguous)
        if matches.len() == 1 {
            Some(matches[0].1)
        } else {
            None
        }
    }

    /// Apply an operation to update the state
    pub fn apply(&mut self, op: &Operation) {
        match op {
            Operation::PipelineCreate {
                id,
                kind,
                name,
                inputs,
                initial_phase,
            } => {
                let pipeline = Pipeline::new(
                    id.clone(),
                    name.clone(),
                    kind.clone(),
                    inputs.clone(),
                    initial_phase.clone(),
                    &oj_core::SystemClock,
                );
                self.pipelines.insert(id.clone(), pipeline);
            }

            Operation::PipelineTransition { id, phase } => {
                if let Some(pipeline) = self.pipelines.get_mut(id) {
                    pipeline.phase = phase.clone();
                    pipeline.phase_status = oj_core::PhaseStatus::Pending;
                }
            }

            Operation::PhaseStatusUpdate {
                pipeline_id,
                status,
            } => {
                if let Some(pipeline) = self.pipelines.get_mut(pipeline_id) {
                    pipeline.phase_status = *status;
                }
            }

            Operation::PipelineDelete { id } => {
                self.pipelines.remove(id);
            }

            Operation::SessionCreate { id, pipeline_id } => {
                self.sessions.insert(
                    id.clone(),
                    Session {
                        id: id.clone(),
                        pipeline_id: pipeline_id.clone(),
                    },
                );
            }

            Operation::SessionDelete { id } => {
                self.sessions.remove(id);
            }

            Operation::WorkspaceCreate { id, path, branch } => {
                self.workspaces.insert(
                    id.clone(),
                    Workspace {
                        id: id.clone(),
                        path: path.clone(),
                        branch: branch.clone(),
                    },
                );
                // Link workspace to pipeline if it exists with the same ID
                if let Some(pipeline) = self.pipelines.get_mut(id) {
                    pipeline.workspace_path = Some(path.clone());
                }
            }

            Operation::WorkspaceDelete { id } => {
                self.workspaces.remove(id);
            }
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

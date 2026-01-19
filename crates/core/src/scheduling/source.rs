// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Production source fetcher for watcher condition evaluation.

use super::{FetchContext, FetchError, SourceFetcher, SourceValue, WatcherSource};
use crate::session::SessionId;
use crate::storage::wal::MaterializedState;
use crate::task::TaskId;
use std::time::Instant;

/// Production source fetcher that queries real system state
pub struct DefaultSourceFetcher<'a> {
    state: &'a MaterializedState,
}

impl<'a> DefaultSourceFetcher<'a> {
    pub fn new(state: &'a MaterializedState) -> Self {
        Self { state }
    }

    fn interpolate(&self, template: &str, context: &FetchContext) -> String {
        let mut result = template.to_string();
        for (key, value) in &context.variables {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    fn parse_command_output(
        &self,
        output: &std::process::Output,
    ) -> Result<SourceValue, FetchError> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FetchError::CommandFailed {
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();

        // Try parsing as JSON first
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return self.json_to_source_value(&json);
        }

        // Try parsing as number
        if let Ok(n) = trimmed.parse::<i64>() {
            return Ok(SourceValue::Numeric { value: n });
        }

        // Try parsing as duration (e.g., "5m", "300s")
        if let Ok(d) = humantime::parse_duration(trimmed) {
            return Ok(SourceValue::Idle { duration: d });
        }

        // Fall back to text
        Ok(SourceValue::Text {
            value: trimmed.to_string(),
        })
    }

    fn json_to_source_value(&self, json: &serde_json::Value) -> Result<SourceValue, FetchError> {
        match json {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(SourceValue::Numeric { value: i })
                } else if let Some(f) = n.as_f64() {
                    Ok(SourceValue::Numeric { value: f as i64 })
                } else {
                    Err(FetchError::ParseError {
                        message: "invalid number".into(),
                    })
                }
            }
            serde_json::Value::Bool(b) => Ok(SourceValue::Boolean { value: *b }),
            serde_json::Value::String(s) => Ok(SourceValue::Text { value: s.clone() }),
            serde_json::Value::Object(obj) => {
                // Check for known patterns
                if let Some(duration) = obj.get("idle_seconds") {
                    if let Some(secs) = duration.as_u64() {
                        return Ok(SourceValue::Idle {
                            duration: std::time::Duration::from_secs(secs),
                        });
                    }
                }
                if let Some(count) = obj.get("count") {
                    if let Some(n) = count.as_u64() {
                        return Ok(SourceValue::EventCount { count: n as usize });
                    }
                }
                Ok(SourceValue::Text {
                    value: json.to_string(),
                })
            }
            _ => Ok(SourceValue::Text {
                value: json.to_string(),
            }),
        }
    }
}

impl SourceFetcher for DefaultSourceFetcher<'_> {
    fn fetch(
        &self,
        source: &WatcherSource,
        context: &FetchContext,
    ) -> Result<SourceValue, FetchError> {
        match source {
            WatcherSource::Session { name } => {
                // Query session from state (sessions are keyed by SessionId)
                let session_id = SessionId(name.clone());
                if let Some(session) = self.state.session(&session_id) {
                    let now = Instant::now();
                    let idle_time = session.idle_time(now).unwrap_or(std::time::Duration::ZERO);
                    Ok(SourceValue::Idle {
                        duration: idle_time,
                    })
                } else {
                    Err(FetchError::SessionNotFound { name: name.clone() })
                }
            }

            WatcherSource::Task { id } => {
                // Query task from state
                let task_id = TaskId(id.clone());
                if let Some(task) = self.state.task(&task_id) {
                    Ok(SourceValue::TaskState {
                        state: format!("{:?}", task.state),
                        phase: Some(task.phase.clone()),
                    })
                } else {
                    Err(FetchError::TaskNotFound { id: id.clone() })
                }
            }

            WatcherSource::Pipeline { id } => {
                // Query pipeline from state
                let pipeline_id = crate::pipeline::PipelineId(id.clone());
                if let Some(pipeline) = self.state.pipeline(&pipeline_id) {
                    Ok(SourceValue::State {
                        state: pipeline.phase.name().to_string(),
                        // Duration tracking would require additional state
                        duration: std::time::Duration::ZERO,
                    })
                } else {
                    Err(FetchError::Other {
                        message: format!("pipeline not found: {}", id),
                    })
                }
            }

            WatcherSource::Queue { name } => {
                // Query queue depth from state (items is the pending items vector)
                let depth = self.state.queue(name).map(|q| q.items.len()).unwrap_or(0);
                Ok(SourceValue::Numeric {
                    value: depth as i64,
                })
            }

            WatcherSource::Events { pattern } => {
                // Query recent events matching pattern
                let events = self.state.recent_events();
                let count = events
                    .iter()
                    .filter(|e| event_matches_pattern(&e.event_type, pattern))
                    .count();
                Ok(SourceValue::EventCount { count })
            }

            WatcherSource::Command { command } => {
                // Execute command and parse output
                let interpolated = self.interpolate(command, context);

                let output = std::process::Command::new("sh")
                    .args(["-c", &interpolated])
                    .output()
                    .map_err(|e| FetchError::CommandFailed {
                        message: e.to_string(),
                    })?;

                self.parse_command_output(&output)
            }

            WatcherSource::File { path } => {
                // Read file and parse content
                let content = std::fs::read_to_string(path).map_err(|e| FetchError::Other {
                    message: format!("failed to read {}: {}", path, e),
                })?;

                // Try JSON first
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    return self.json_to_source_value(&json);
                }

                Ok(SourceValue::Text { value: content })
            }

            WatcherSource::Http { url } => {
                // Simple HTTP GET (blocking)
                let mut response = ureq::get(url).call().map_err(|e| FetchError::Other {
                    message: format!("HTTP request failed: {}", e),
                })?;

                let body = response.body_mut().read_to_string().map_err(|e| FetchError::Other {
                    message: format!("failed to read response: {}", e),
                })?;

                // Try JSON first
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    return self.json_to_source_value(&json);
                }

                Ok(SourceValue::Text { value: body })
            }
        }
    }
}

/// Check if an event type matches a pattern
fn event_matches_pattern(event_type: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        event_type.starts_with(prefix)
    } else {
        event_type == pattern
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;

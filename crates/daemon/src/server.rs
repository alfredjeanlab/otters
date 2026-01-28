// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Socket server and connection handling.

use tokio::net::UnixStream;
use tracing::{debug, error};

use crate::lifecycle::DaemonState;
use crate::protocol::{
    self, PipelineDetail, PipelineSummary, Query, Request, Response, SessionSummary,
    DEFAULT_TIMEOUT, PROTOCOL_VERSION,
};

/// Handle a single client connection
pub async fn handle_connection(
    daemon: &mut DaemonState,
    stream: UnixStream,
) -> Result<(), ServerError> {
    // Split stream for reading/writing
    let (mut reader, mut writer) = stream.into_split();

    // Read request with timeout
    let request = match protocol::read_request(&mut reader, DEFAULT_TIMEOUT).await {
        Ok(req) => req,
        Err(protocol::ProtocolError::Timeout) => {
            error!("Request read timeout");
            return Err(ServerError::Timeout);
        }
        Err(protocol::ProtocolError::ConnectionClosed) => {
            debug!("Client disconnected before sending request");
            return Ok(());
        }
        Err(e) => {
            error!("Failed to read request: {}", e);
            return Err(ServerError::Protocol(e));
        }
    };

    debug!("Received request: {:?}", request);

    // Handle request
    let response = handle_request(daemon, request).await;

    debug!("Sending response: {:?}", response);

    // Write response with timeout
    protocol::write_response(&mut writer, &response, DEFAULT_TIMEOUT)
        .await
        .map_err(ServerError::Protocol)?;

    Ok(())
}

/// Handle a single request and return a response
async fn handle_request(daemon: &mut DaemonState, request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong,

        Request::Hello { version: _ } => Response::Hello {
            version: PROTOCOL_VERSION.to_string(),
        },

        Request::Event { event } => match daemon.process_event(event).await {
            Ok(()) => Response::Event { accepted: true },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::Query { query } => handle_query(daemon, query),

        Request::Shutdown => {
            daemon.shutdown_requested = true;
            Response::ShuttingDown
        }

        Request::Status => {
            let uptime_secs = daemon.start_time.elapsed().as_secs();
            let (pipelines_active, sessions_active) = {
                let state = daemon.state.lock().unwrap_or_else(|e| e.into_inner());
                let active = state
                    .pipelines
                    .values()
                    .filter(|p| !p.is_terminal())
                    .count();
                let sessions = state.sessions.len();
                (active, sessions)
            };

            Response::Status {
                uptime_secs,
                pipelines_active,
                sessions_active,
            }
        }

        Request::SessionSend { id, input } => {
            // Find the session and send input
            let session_id = {
                let state = daemon.state.lock().unwrap_or_else(|e| e.into_inner());
                // Look up by session ID directly or by pipeline ID
                if state.sessions.contains_key(&id) {
                    Some(id.clone())
                } else {
                    // Maybe it's a pipeline ID - find associated session
                    state.pipelines.get(&id).and_then(|p| p.session_id.clone())
                }
            };

            match session_id {
                Some(sid) => {
                    // Create send event and process it
                    match daemon
                        .process_event(oj_core::Event::Custom {
                            name: "session:send".to_string(),
                            data: serde_json::json!({
                                "session_id": sid,
                                "input": input,
                            }),
                        })
                        .await
                    {
                        Ok(()) => Response::Ok,
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                None => Response::Error {
                    message: format!("Session not found: {}", id),
                },
            }
        }

        Request::PipelineResume { id } => {
            // Resume monitoring for an escalated pipeline
            match daemon
                .process_event(oj_core::Event::Custom {
                    name: "pipeline:resume".to_string(),
                    data: serde_json::json!({
                        "pipeline_id": id,
                    }),
                })
                .await
            {
                Ok(()) => Response::Ok,
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::PipelineFail { id, error } => {
            // Mark pipeline as failed
            match daemon
                .process_event(oj_core::Event::AgentError {
                    pipeline_id: id,
                    error,
                })
                .await
            {
                Ok(()) => Response::Ok,
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

/// Handle query requests
fn handle_query(daemon: &DaemonState, query: Query) -> Response {
    let state = daemon.state.lock().unwrap_or_else(|e| e.into_inner());

    match query {
        Query::ListPipelines => {
            let pipelines = state
                .pipelines
                .values()
                .map(|p| PipelineSummary {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    phase: p.phase.clone(),
                    phase_status: format!("{:?}", p.phase_status),
                })
                .collect();
            Response::Pipelines { pipelines }
        }

        Query::GetPipeline { id } => {
            let pipeline = state.get_pipeline(&id).map(|p| {
                Box::new(PipelineDetail {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    phase: p.phase.clone(),
                    phase_status: format!("{:?}", p.phase_status),
                    inputs: p.inputs.clone(),
                    workspace_path: p.workspace_path.clone(),
                    session_id: p.session_id.clone(),
                    error: p.error.clone(),
                })
            });
            Response::Pipeline { pipeline }
        }

        Query::ListSessions => {
            let sessions = state
                .sessions
                .values()
                .map(|s| SessionSummary {
                    id: s.id.clone(),
                    pipeline_id: Some(s.pipeline_id.clone()),
                })
                .collect();
            Response::Sessions { sessions }
        }
    }
}

/// Server errors
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] protocol::ProtocolError),

    #[error("Request timeout")]
    Timeout,
}

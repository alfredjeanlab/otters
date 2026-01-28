// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Fake session adapter for testing
#![cfg_attr(coverage_nightly, coverage(off))]

use super::{SessionAdapter, SessionError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Recorded session call
#[derive(Debug, Clone)]
pub enum SessionCall {
    Spawn {
        name: String,
        cwd: PathBuf,
        cmd: String,
        env: Vec<(String, String)>,
    },
    Send {
        id: String,
        input: String,
    },
    Kill {
        id: String,
    },
    IsAlive {
        id: String,
    },
    CaptureOutput {
        id: String,
        lines: u32,
    },
    IsProcessRunning {
        id: String,
        pattern: String,
    },
}

/// Fake session state
#[derive(Debug, Clone)]
pub struct FakeSession {
    pub name: String,
    pub cwd: PathBuf,
    pub cmd: String,
    pub env: Vec<(String, String)>,
    pub output: Vec<String>,
    pub alive: bool,
    pub exit_code: Option<i32>,
    pub process_running: bool,
}

/// Fake session adapter for testing
#[derive(Clone, Default)]
pub struct FakeSessionAdapter {
    sessions: Arc<Mutex<HashMap<String, FakeSession>>>,
    calls: Arc<Mutex<Vec<SessionCall>>>,
    next_id: Arc<Mutex<u64>>,
}

impl FakeSessionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all recorded calls
    pub fn calls(&self) -> Vec<SessionCall> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Option<FakeSession> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// Set session output
    pub fn set_output(&self, id: &str, output: Vec<String>) {
        if let Some(session) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(id)
        {
            session.output = output;
        }
    }

    /// Mark session as exited
    pub fn set_exited(&self, id: &str, exit_code: i32) {
        if let Some(session) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(id)
        {
            session.alive = false;
            session.exit_code = Some(exit_code);
        }
    }

    /// Set whether a process is running in the session
    pub fn set_process_running(&self, id: &str, running: bool) {
        if let Some(session) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(id)
        {
            session.process_running = running;
        }
    }
}

#[async_trait]
impl SessionAdapter for FakeSessionAdapter {
    async fn spawn(
        &self,
        name: &str,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<String, SessionError> {
        let id = {
            let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            format!("fake-{}", *next)
        };

        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::Spawn {
                name: name.to_string(),
                cwd: cwd.to_path_buf(),
                cmd: cmd.to_string(),
                env: env.to_vec(),
            });

        let session = FakeSession {
            name: name.to_string(),
            cwd: cwd.to_path_buf(),
            cmd: cmd.to_string(),
            env: env.to_vec(),
            output: Vec::new(),
            alive: true,
            exit_code: None,
            process_running: true,
        };

        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), session);

        Ok(id)
    }

    async fn send(&self, id: &str, input: &str) -> Result<(), SessionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::Send {
                id: id.to_string(),
                input: input.to_string(),
            });

        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if !sessions.contains_key(id) {
            return Err(SessionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    async fn kill(&self, id: &str) -> Result<(), SessionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::Kill { id: id.to_string() });

        if let Some(session) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(id)
        {
            session.alive = false;
        }

        Ok(())
    }

    async fn is_alive(&self, id: &str) -> Result<bool, SessionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::IsAlive { id: id.to_string() });

        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match sessions.get(id) {
            Some(session) => Ok(session.alive),
            None => Ok(false),
        }
    }

    async fn capture_output(&self, id: &str, lines: u32) -> Result<String, SessionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::CaptureOutput {
                id: id.to_string(),
                lines,
            });

        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match sessions.get(id) {
            Some(session) => {
                let start = session.output.len().saturating_sub(lines as usize);
                Ok(session.output[start..].join("\n"))
            }
            None => Err(SessionError::NotFound(id.to_string())),
        }
    }

    async fn is_process_running(&self, id: &str, pattern: &str) -> Result<bool, SessionError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(SessionCall::IsProcessRunning {
                id: id.to_string(),
                pattern: pattern.to_string(),
            });

        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match sessions.get(id) {
            Some(session) => Ok(session.process_running),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
#[path = "fake_tests.rs"]
mod tests;

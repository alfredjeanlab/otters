// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Daemon client for CLI commands

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use oj_core::Event;
use oj_daemon::protocol::{self, ProtocolError};
use oj_daemon::{Query, Request, Response};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::net::UnixStream;

// Timeout configuration (env vars in milliseconds)
fn parse_duration_ms(var: &str) -> Option<Duration> {
    std::env::var(var)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_millis)
}

/// Timeout for IPC requests (hello, status, event, query, shutdown)
pub fn timeout_ipc() -> Duration {
    parse_duration_ms("OJ_TIMEOUT_IPC_MS").unwrap_or(Duration::from_secs(5))
}

/// Timeout for waiting for daemon to start
pub fn timeout_connect() -> Duration {
    parse_duration_ms("OJ_TIMEOUT_CONNECT_MS").unwrap_or(Duration::from_secs(5))
}

/// Timeout for waiting for process to exit
pub fn timeout_exit() -> Duration {
    parse_duration_ms("OJ_TIMEOUT_EXIT_MS").unwrap_or(Duration::from_secs(2))
}

/// Polling interval for retries
pub fn poll_interval() -> Duration {
    parse_duration_ms("OJ_POLL_INTERVAL_MS").unwrap_or(Duration::from_millis(50))
}

/// Client errors
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Daemon not running")]
    DaemonNotRunning,

    #[error("Failed to start daemon: {0}")]
    DaemonStartFailed(String),

    #[error("Connection timeout waiting for daemon to start")]
    DaemonStartTimeout,

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Event rejected: {0}")]
    Rejected(String),

    #[error("Unexpected response from daemon")]
    UnexpectedResponse,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Could not determine project root")]
    NoProjectRoot,

    #[error("Could not determine state directory")]
    NoStateDir,
}

/// Daemon client
pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    /// Connect to daemon, auto-starting if not running
    pub fn connect_or_start(project_root: PathBuf) -> Result<Self, ClientError> {
        // Check version file before connecting - restart daemon if version mismatch
        if let Ok(daemon_dir) = get_daemon_dir(&project_root) {
            let version_path = daemon_dir.join("daemon.version");
            if let Ok(daemon_version) = std::fs::read_to_string(&version_path) {
                let cli_version = env!("CARGO_PKG_VERSION");
                if daemon_version.trim() != cli_version {
                    // Version mismatch - stop old daemon first
                    let _ = tokio::runtime::Handle::current().block_on(daemon_stop(&project_root));
                }
            }
        }

        match Self::connect(project_root.clone()) {
            Ok(client) => Ok(client),
            Err(ClientError::DaemonNotRunning) => {
                // Start daemon in background
                let child = start_daemon_background(&project_root)?;
                // Wait for socket with retry, watching for early exit
                Self::connect_with_retry(project_root, timeout_connect(), child)
            }
            Err(e) => Err(wrap_with_startup_error(e, &project_root)),
        }
    }

    /// Connect to existing daemon (no auto-start)
    pub fn connect(project_root: PathBuf) -> Result<Self, ClientError> {
        let socket_path = get_socket_path(&project_root)?;

        if !socket_path.exists() {
            return Err(ClientError::DaemonNotRunning);
        }

        Ok(Self { socket_path })
    }

    fn connect_with_retry(
        project_root: PathBuf,
        timeout: Duration,
        mut child: std::process::Child,
    ) -> Result<Self, ClientError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            // Check if daemon process exited early (startup failure)
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited - startup failed
                    // Poll for startup error in log (filesystem may need to sync)
                    let poll_start = Instant::now();
                    while poll_start.elapsed() < timeout_exit() {
                        if let Some(err) = read_startup_error(&project_root) {
                            return Err(ClientError::DaemonStartFailed(err));
                        }
                        std::thread::sleep(poll_interval());
                    }
                    // No error found in log, return generic failure
                    return Err(ClientError::DaemonStartFailed(format!(
                        "exited with {}",
                        status
                    )));
                }
                Ok(None) => {
                    // Still running, try to connect
                }
                Err(_) => {
                    // Error checking status, assume still running
                }
            }

            match Self::connect(project_root.clone()) {
                Ok(client) => return Ok(client),
                Err(ClientError::DaemonNotRunning) => {
                    std::thread::sleep(poll_interval());
                }
                Err(e) => return Err(wrap_with_startup_error(e, &project_root)),
            }
        }

        // Timeout - check log for startup errors
        Err(wrap_with_startup_error(
            ClientError::DaemonStartTimeout,
            &project_root,
        ))
    }

    /// Send a request and receive a response with specific timeouts
    async fn send_with_timeout(
        &self,
        request: Request,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Result<Response, ClientError> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (mut reader, mut writer) = stream.into_split();

        // Encode and send request with write timeout
        let data = protocol::encode(&request)?;
        tokio::time::timeout(write_timeout, protocol::write_message(&mut writer, &data))
            .await
            .map_err(|_| ProtocolError::Timeout)??;

        // Read response with read timeout
        let response_bytes =
            tokio::time::timeout(read_timeout, protocol::read_message(&mut reader))
                .await
                .map_err(|_| ProtocolError::Timeout)??;

        let response: Response = protocol::decode(&response_bytes)?;
        Ok(response)
    }

    /// Send a request and receive a response
    pub async fn send(&self, request: Request) -> Result<Response, ClientError> {
        self.send_with_timeout(request, timeout_ipc(), timeout_ipc())
            .await
    }

    /// Send an event to the daemon
    pub async fn send_event(&self, event: Event) -> Result<(), ClientError> {
        match self.send(Request::Event { event }).await? {
            Response::Event { accepted: true } => Ok(()),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Query for pipelines
    pub async fn list_pipelines(&self) -> Result<Vec<oj_daemon::PipelineSummary>, ClientError> {
        match self
            .send(Request::Query {
                query: Query::ListPipelines,
            })
            .await?
        {
            Response::Pipelines { pipelines } => Ok(pipelines),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Query for a specific pipeline
    pub async fn get_pipeline(
        &self,
        id: &str,
    ) -> Result<Option<oj_daemon::PipelineDetail>, ClientError> {
        match self
            .send(Request::Query {
                query: Query::GetPipeline { id: id.to_string() },
            })
            .await?
        {
            Response::Pipeline { pipeline } => Ok(pipeline.map(|b| *b)),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Get daemon status
    pub async fn status(&self) -> Result<(u64, usize, usize), ClientError> {
        match self.send(Request::Status).await? {
            Response::Status {
                uptime_secs,
                pipelines_active,
                sessions_active,
            } => Ok((uptime_secs, pipelines_active, sessions_active)),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Request daemon shutdown
    pub async fn shutdown(&self) -> Result<(), ClientError> {
        match self.send(Request::Shutdown).await? {
            Response::Ok | Response::ShuttingDown => Ok(()),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Get daemon version via Hello handshake
    pub async fn hello(&self) -> Result<String, ClientError> {
        match self
            .send(Request::Hello {
                version: env!("CARGO_PKG_VERSION").to_string(),
            })
            .await?
        {
            Response::Hello { version } => Ok(version),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Query for sessions
    pub async fn list_sessions(&self) -> Result<Vec<oj_daemon::SessionSummary>, ClientError> {
        match self
            .send(Request::Query {
                query: Query::ListSessions,
            })
            .await?
        {
            Response::Sessions { sessions } => Ok(sessions),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Send input to a session
    pub async fn session_send(&self, id: &str, input: &str) -> Result<(), ClientError> {
        match self
            .send(Request::SessionSend {
                id: id.to_string(),
                input: input.to_string(),
            })
            .await?
        {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Resume monitoring for an escalated pipeline
    pub async fn pipeline_resume(&self, id: &str) -> Result<(), ClientError> {
        match self
            .send(Request::PipelineResume { id: id.to_string() })
            .await?
        {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Mark a pipeline as failed
    pub async fn pipeline_fail(&self, id: &str, error: &str) -> Result<(), ClientError> {
        match self
            .send(Request::PipelineFail {
                id: id.to_string(),
                error: error.to_string(),
            })
            .await?
        {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(ClientError::Rejected(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }
}

/// Start the daemon in the background, returning the child process handle
fn start_daemon_background(project_root: &Path) -> Result<std::process::Child, ClientError> {
    // Find the ojd binary - look in cargo target dir or PATH
    let ojd_path = find_ojd_binary()?;

    Command::new(&ojd_path)
        .arg(project_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| ClientError::DaemonStartFailed(e.to_string()))
}

/// Stop the daemon (graceful first, then forceful)
/// Returns true if daemon was stopped, false if it wasn't running
pub async fn daemon_stop(project_root: &Path) -> Result<bool, ClientError> {
    let client = match DaemonClient::connect(project_root.to_path_buf()) {
        Ok(c) => c,
        Err(ClientError::DaemonNotRunning) => {
            // Clean up any stale files
            if let Ok(daemon_dir) = get_daemon_dir(project_root) {
                cleanup_stale_pid(&daemon_dir);
            }
            return Ok(false);
        }
        Err(e) => return Err(e),
    };

    // Try graceful shutdown (timeout handled by send())
    let shutdown_result = client.shutdown().await;

    if let Some(pid) = read_daemon_pid(project_root)? {
        if shutdown_result.is_ok() {
            // Graceful shutdown succeeded, wait for process to exit
            wait_for_exit(pid, timeout_exit()).await;
        }

        // Force kill if still running
        if process_exists(pid) {
            force_kill_daemon(pid);
            wait_for_exit(pid, timeout_exit()).await;
        }
    }

    // Clean up stale files
    let daemon_dir = get_daemon_dir(project_root)?;
    cleanup_stale_pid(&daemon_dir);

    Ok(true)
}

/// Wait for a process to exit
async fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !process_exists(pid) {
            return true;
        }
        tokio::time::sleep(poll_interval()).await;
    }
    false
}

/// Find the ojd binary
fn find_ojd_binary() -> Result<PathBuf, ClientError> {
    // Explicit override (used by tests to ensure correct binary)
    if let Ok(path) = std::env::var("OJ_DAEMON_BINARY") {
        return Ok(PathBuf::from(path));
    }

    // First check if we're running from cargo (development)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = PathBuf::from(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("target/debug/ojd"));
        if let Some(path) = dev_path {
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // Check current executable's directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("ojd");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // Fall back to PATH lookup
    Ok(PathBuf::from("ojd"))
}

/// Get the socket path for a project
///
/// Uses a short path under /tmp to avoid SUN_LEN limit (104 bytes on macOS).
/// The socket is separate from state_dir which can be longer.
fn get_socket_path(project_root: &Path) -> Result<PathBuf, ClientError> {
    let canonical = project_root
        .canonicalize()
        .map_err(|_| ClientError::NoProjectRoot)?;

    let hash = project_hash(&canonical);
    let socket_dir = socket_dir()?;

    Ok(socket_dir.join(format!("{}.sock", hash)))
}

/// Get the socket directory for oj
///
/// Uses /tmp/oj by default to keep paths short (macOS SUN_LEN = 104).
/// Can be overridden with OJ_SOCKET_DIR for testing.
fn socket_dir() -> Result<PathBuf, ClientError> {
    if let Ok(dir) = std::env::var("OJ_SOCKET_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from("/tmp/oj"))
}

/// Get the state directory for oj
fn state_dir() -> Result<PathBuf, ClientError> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(xdg).join("oj"));
    }

    let home = std::env::var("HOME").map_err(|_| ClientError::NoStateDir)?;
    Ok(PathBuf::from(home).join(".local/state/oj"))
}

/// Compute project hash for unique daemon directory
fn project_hash(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    // Take first 16 chars of hex digest
    result[..8].iter().map(|b| format!("{:02x}", b)).collect()
}

/// Find the project root by walking up from current directory
///
/// Checks OJ_PROJECT_ROOT env var first (for agents running in workspaces),
/// then walks up looking for .oj directory.
pub fn find_project_root() -> Result<PathBuf, ClientError> {
    // Check env var first (set for agents running in workspaces)
    if let Ok(root) = std::env::var("OJ_PROJECT_ROOT") {
        return Ok(PathBuf::from(root));
    }

    let mut current = std::env::current_dir().map_err(|_| ClientError::NoProjectRoot)?;

    loop {
        if current.join(".oj").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            // No .oj directory found, use current directory as project root
            return std::env::current_dir().map_err(|_| ClientError::NoProjectRoot);
        }
    }
}

/// Clean up orphaned PID file during shutdown.
///
/// Called by daemon_stop when the daemon is not running or after stopping it.
fn cleanup_stale_pid(daemon_dir: &Path) {
    let pid_path = daemon_dir.join("daemon.pid");
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }
}

/// Get the PID from the daemon PID file, if it exists
pub fn read_daemon_pid(project_root: &Path) -> Result<Option<u32>, ClientError> {
    let daemon_dir = get_daemon_dir(project_root)?;
    let pid_path = daemon_dir.join("daemon.pid");

    if !pid_path.exists() {
        return Ok(None);
    }

    match std::fs::read_to_string(&pid_path) {
        Ok(content) => {
            let pid = content.trim().parse::<u32>().ok();
            Ok(pid)
        }
        Err(_) => Ok(None),
    }
}

/// Check if a process with the given PID exists
pub fn process_exists(pid: u32) -> bool {
    // Use kill -0 to check if process exists without sending a signal
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Force kill a daemon process
pub fn force_kill_daemon(pid: u32) -> bool {
    Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get the daemon state directory for a project (where logs, pid, version files live)
pub fn get_daemon_dir(project_root: &Path) -> Result<PathBuf, ClientError> {
    let canonical = project_root
        .canonicalize()
        .map_err(|_| ClientError::NoProjectRoot)?;

    let hash = project_hash(&canonical);
    let state_dir = state_dir()?;

    Ok(state_dir.join("projects").join(&hash))
}

/// Startup marker prefix that daemon writes to log before anything else.
/// Full format: "--- ojd: starting (pid: 12345) ---"
const STARTUP_MARKER_PREFIX: &str = "--- ojd: starting (pid: ";

/// Read daemon log from startup marker, looking for errors.
/// Returns the error message if found, None otherwise.
pub fn read_startup_error(project_root: &Path) -> Option<String> {
    let daemon_dir = get_daemon_dir(project_root).ok()?;
    let log_path = daemon_dir.join("daemon.log");

    let content = std::fs::read_to_string(&log_path).ok()?;

    // Find the last startup marker
    let start_pos = content.rfind(STARTUP_MARKER_PREFIX)?;
    let startup_log = &content[start_pos..];

    // Look for ERROR lines
    let errors: Vec<&str> = startup_log
        .lines()
        .filter(|line| line.contains(" ERROR ") || line.contains("Failed to start"))
        .collect();

    if errors.is_empty() {
        return None;
    }

    // Extract just the error messages (strip timestamp/level prefix)
    let error_messages: Vec<String> = errors
        .iter()
        .filter_map(|line| {
            // Format: "timestamp LEVEL target: message"
            // Find the message part after the last colon-space
            line.split_once(": ").map(|(_, msg)| msg.to_string())
        })
        .collect();

    if error_messages.is_empty() {
        Some(errors.join("\n"))
    } else {
        Some(error_messages.join("\n"))
    }
}

/// Wrap an error with startup log info if available.
/// If the daemon log contains errors, return DaemonStartFailed with that info.
/// Otherwise, return the original error.
fn wrap_with_startup_error(err: ClientError, project_root: &Path) -> ClientError {
    // Don't double-wrap
    if matches!(err, ClientError::DaemonStartFailed(_)) {
        return err;
    }

    if let Some(startup_error) = read_startup_error(project_root) {
        ClientError::DaemonStartFailed(startup_error)
    } else {
        err
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

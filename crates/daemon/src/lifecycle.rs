// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Daemon lifecycle management: startup, shutdown, recovery.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fs2::FileExt;
use oj_adapters::session::SessionAdapter;
use oj_adapters::{
    GitAdapter, NoOpNotifyAdapter, TmuxAdapter, TracedRepoAdapter, TracedSessionAdapter,
};
use oj_core::{Event, SystemClock, UuidIdGen};
use oj_engine::{Runtime, RuntimeConfig, RuntimeDeps, Scheduler};
use oj_runbook::{parse_runbook, Runbook};
use oj_storage::{MaterializedState, Wal};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Daemon runtime with concrete adapter types (wrapped with tracing)
pub type DaemonRuntime = Runtime<
    TracedSessionAdapter<TmuxAdapter>,
    TracedRepoAdapter<GitAdapter>,
    NoOpNotifyAdapter,
    SystemClock,
    UuidIdGen,
>;

/// Daemon configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Project root directory
    pub project_root: PathBuf,
    /// Path to Unix socket
    pub socket_path: PathBuf,
    /// Path to lock/PID file
    pub lock_path: PathBuf,
    /// Path to version file
    pub version_path: PathBuf,
    /// Path to daemon log file
    pub log_path: PathBuf,
    /// Path to WAL directory
    pub wal_path: PathBuf,
    /// Path to workspaces directory
    pub workspaces_path: PathBuf,
}

impl Config {
    /// Create config for a project
    pub fn for_project(project_root: &Path) -> Result<Self, LifecycleError> {
        let canonical = project_root
            .canonicalize()
            .map_err(|e| LifecycleError::ProjectNotFound(project_root.to_path_buf(), e))?;

        let hash = project_hash(&canonical);
        let state_dir = state_dir()?.join("projects").join(&hash);
        let socket_dir = socket_dir()?;

        Ok(Self {
            project_root: canonical,
            socket_path: socket_dir.join(format!("{}.sock", hash)),
            lock_path: state_dir.join("daemon.pid"),
            version_path: state_dir.join("daemon.version"),
            log_path: state_dir.join("daemon.log"),
            wal_path: state_dir.join("wal").join("events.wal"),
            workspaces_path: state_dir.join("workspaces"),
        })
    }
}

/// Daemon state during operation
pub struct DaemonState {
    /// Configuration
    pub config: Config,
    // NOTE(lifetime): Held to maintain exclusive file lock; released on drop
    #[allow(dead_code)]
    lock_file: File,
    /// Unix socket listener
    pub listener: UnixListener,
    /// Materialized state (shared with runtime)
    pub state: Arc<Mutex<MaterializedState>>,
    /// Runtime for event processing
    pub runtime: DaemonRuntime,
    /// Scheduler for timers (shared with runtime)
    pub scheduler: Arc<Mutex<Scheduler>>,
    /// Channel for internal events
    pub internal_events: mpsc::Receiver<Event>,
    // KEEP UNTIL: adapter event emission is implemented
    #[allow(dead_code)]
    internal_tx: mpsc::Sender<Event>,
    /// When daemon started
    pub start_time: Instant,
    /// Shutdown requested flag
    pub shutdown_requested: bool,
}

impl DaemonState {
    /// Process an event through the runtime
    ///
    /// Any events produced by the runtime (e.g., ShellCompleted) are fed back
    /// into the event loop iteratively.
    pub async fn process_event(&mut self, event: Event) -> Result<(), LifecycleError> {
        let mut pending_events = vec![event];

        while let Some(event) = pending_events.pop() {
            let result_events = self
                .runtime
                .handle_event(event)
                .await
                .map_err(|e| LifecycleError::Runtime(e.to_string()))?;

            // Queue any produced events to be processed next
            pending_events.extend(result_events);
        }

        Ok(())
    }

    /// Check heartbeats and fire timers
    pub async fn check_heartbeats(&mut self) -> Result<(), LifecycleError> {
        let now = std::time::Instant::now();
        let timer_events = {
            let mut scheduler = self.scheduler.lock().unwrap_or_else(|e| e.into_inner());
            scheduler.fired_timers(now)
        };
        for event in timer_events {
            self.process_event(event).await?;
        }
        Ok(())
    }

    /// Shutdown the daemon gracefully
    pub async fn shutdown(&mut self) -> Result<(), LifecycleError> {
        info!("Shutting down daemon...");

        // 1. Stop accepting connections (listener dropped when DaemonState dropped)
        // Note: we don't drop the listener here to keep accepting until the very end

        // 2. Remove socket file
        if self.config.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.config.socket_path) {
                warn!("Failed to remove socket file: {}", e);
            }
        }

        // 3. Remove PID file
        if self.config.lock_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.config.lock_path) {
                warn!("Failed to remove PID file: {}", e);
            }
        }

        // 4. Remove version file
        if self.config.version_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.config.version_path) {
                warn!("Failed to remove version file: {}", e);
            }
        }

        // 5. Lock file is released automatically when self.lock_file is dropped

        info!("Daemon shutdown complete");
        Ok(())
    }
}

/// Lifecycle errors
#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Project not found at {0}: {1}")]
    ProjectNotFound(PathBuf, std::io::Error),

    #[error("Could not determine state directory")]
    NoStateDir,

    #[error("Failed to acquire lock: daemon already running?")]
    LockFailed(#[source] std::io::Error),

    #[error("Failed to bind socket at {0}: {1}")]
    BindFailed(PathBuf, std::io::Error),

    #[error("WAL error: {0}")]
    Wal(#[from] oj_storage::WalError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Runbook parse error: {0}")]
    Runbook(#[from] oj_runbook::ParseError),

    #[error("Runtime error: {0}")]
    Runtime(String),
}

/// Start the daemon
pub async fn startup(config: &Config) -> Result<DaemonState, LifecycleError> {
    match startup_inner(config).await {
        Ok(state) => Ok(state),
        Err(e) => {
            // Clean up any resources created before failure
            cleanup_on_failure(config);
            Err(e)
        }
    }
}

/// Inner startup logic - cleanup_on_failure called if this fails
async fn startup_inner(config: &Config) -> Result<DaemonState, LifecycleError> {
    // 1. Create state directory (needed for socket, lock, etc.)
    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 2. Acquire lock file FIRST - prevents races
    let lock_file = File::create(&config.lock_path)?;
    lock_file
        .try_lock_exclusive()
        .map_err(LifecycleError::LockFailed)?;

    // Write PID to lock file
    use std::io::Write;
    let mut lock_file = lock_file;
    writeln!(lock_file, "{}", std::process::id())?;
    let lock_file = lock_file; // Reborrow as immutable

    // 3. Create directories
    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = config.wal_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&config.workspaces_path)?;

    // Write version file
    std::fs::write(&config.version_path, env!("CARGO_PKG_VERSION"))?;

    // 4. Load runbook BEFORE binding socket (fail fast, don't accept connections if invalid)
    let runbook = load_runbook(&config.project_root)?;

    // 5. Load state from WAL
    let wal = Wal::open(&config.wal_path)?;
    let mut state = MaterializedState::default();
    for op in Wal::replay(&config.wal_path)? {
        state.apply(&op);
    }

    info!(
        "Loaded state: {} pipelines, {} sessions, {} workspaces",
        state.pipelines.len(),
        state.sessions.len(),
        state.workspaces.len()
    );

    // 6. Reconcile with reality (MVP: log warnings only)
    reconcile_state(&state, &config.project_root).await;

    // 7. Set up adapters (wrapped with tracing for observability)
    let session_adapter = TracedSessionAdapter::new(TmuxAdapter::new());
    let repo_adapter = TracedRepoAdapter::new(GitAdapter::new(config.project_root.clone()));
    let notify_adapter = NoOpNotifyAdapter::new();

    // 8. Set up internal event channel
    let (internal_tx, internal_events) = mpsc::channel(100);

    // 9. Remove stale socket and bind (LAST - only after all validation passes)
    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path)?;
    }
    let listener = UnixListener::bind(&config.socket_path)
        .map_err(|e| LifecycleError::BindFailed(config.socket_path.clone(), e))?;

    // 11. Wrap state and WAL in Arc<Mutex>
    let state = Arc::new(Mutex::new(state));
    let wal = Arc::new(Mutex::new(wal));
    let scheduler = Arc::new(Mutex::new(Scheduler::new()));

    // 12. Create runtime
    let runtime = Runtime::new(
        RuntimeDeps {
            sessions: session_adapter,
            repos: repo_adapter,
            notify: notify_adapter,
            wal,
            state: Arc::clone(&state),
        },
        runbook,
        SystemClock,
        UuidIdGen,
        RuntimeConfig {
            project_root: config.project_root.clone(),
            worktree_root: config.workspaces_path.clone(),
        },
    );

    info!(
        "Daemon started for project: {}",
        config.project_root.display()
    );

    Ok(DaemonState {
        config: config.clone(),
        lock_file,
        listener,
        state,
        runtime,
        scheduler,
        internal_events,
        internal_tx,
        start_time: Instant::now(),
        shutdown_requested: false,
    })
}

/// Clean up resources on startup failure
fn cleanup_on_failure(config: &Config) {
    // Remove socket if we created it
    if config.socket_path.exists() {
        let _ = std::fs::remove_file(&config.socket_path);
    }

    // Remove version file
    if config.version_path.exists() {
        let _ = std::fs::remove_file(&config.version_path);
    }

    // Remove PID/lock file
    if config.lock_path.exists() {
        let _ = std::fs::remove_file(&config.lock_path);
    }
}

/// Load runbook from project directory
fn load_runbook(project_root: &Path) -> Result<Runbook, LifecycleError> {
    let runbook_dir = project_root.join(".oj/runbooks");

    if !runbook_dir.exists() {
        return Ok(Runbook::default());
    }

    let mut combined_content = String::new();

    // Read all .toml files in the runbooks directory
    let entries = std::fs::read_dir(&runbook_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            let content = std::fs::read_to_string(&path)?;
            combined_content.push_str(&content);
            combined_content.push('\n');
        }
    }

    if combined_content.is_empty() {
        return Ok(Runbook::default());
    }

    Ok(parse_runbook(&combined_content)?)
}

/// Reconcile persisted state with actual world state
async fn reconcile_state(state: &MaterializedState, _project_root: &Path) {
    // MVP: Just log warnings about state that may need attention

    // Check for in-progress pipelines
    let in_progress: Vec<_> = state
        .pipelines
        .values()
        .filter(|p| !p.is_terminal())
        .collect();

    if !in_progress.is_empty() {
        warn!(
            "Found {} in-progress pipelines from previous session (manual intervention may be needed)",
            in_progress.len()
        );
        for p in &in_progress {
            warn!("  - {} ({}): {}", p.id, p.name, p.phase);
        }
    }

    // Check for orphaned sessions
    if !state.sessions.is_empty() {
        warn!(
            "Found {} sessions from previous session (may be stale)",
            state.sessions.len()
        );
    }
}

/// Get the state directory for oj
fn state_dir() -> Result<PathBuf, LifecycleError> {
    // Use XDG_STATE_HOME or default to ~/.local/state
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(xdg).join("oj"));
    }

    let home = std::env::var("HOME").map_err(|_| LifecycleError::NoStateDir)?;
    Ok(PathBuf::from(home).join(".local/state/oj"))
}

/// Get the socket directory for oj
///
/// Uses /tmp/oj by default to keep paths short (macOS SUN_LEN = 104).
/// Can be overridden with OJ_SOCKET_DIR for testing.
fn socket_dir() -> Result<PathBuf, LifecycleError> {
    if let Ok(dir) = std::env::var("OJ_SOCKET_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from("/tmp/oj"))
}

/// Compute project hash for unique daemon directory
fn project_hash(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    // Take first 16 chars of hex digest
    hex_encode(&result[..8])
}

// Hex encoding helper
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Events detected from session checks
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// tmux session exited (session is gone, everything lost)
    TmuxExited { pipeline_id: String },
    /// Claude process exited inside tmux (triggers on_exit)
    ClaudeExited { pipeline_id: String },
}

/// Check for sessions that have exited
///
/// Iterates over all pipelines with active sessions and checks:
/// 1. Whether the tmux session is still alive
/// 2. Whether the claude process is still running inside tmux
pub async fn check_sessions<S: SessionAdapter>(
    sessions: &S,
    state: &MaterializedState,
) -> Vec<SessionEvent> {
    let mut events = Vec::new();

    for pipeline in state.pipelines.values() {
        // Skip terminal pipelines
        if pipeline.is_terminal() {
            continue;
        }

        let Some(session_id) = &pipeline.session_id else {
            continue;
        };

        // Check if tmux session is alive
        match sessions.is_alive(session_id).await {
            Ok(false) => {
                events.push(SessionEvent::TmuxExited {
                    pipeline_id: pipeline.id.clone(),
                });
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    pipeline_id = %pipeline.id,
                    error = %e,
                    "failed to check tmux session"
                );
                continue;
            }
            Ok(true) => {}
        }

        // Check if claude process is running inside tmux
        match sessions.is_process_running(session_id, "claude").await {
            Ok(false) => {
                events.push(SessionEvent::ClaudeExited {
                    pipeline_id: pipeline.id.clone(),
                });
            }
            Ok(true) => {} // Still running
            Err(e) => {
                tracing::warn!(
                    pipeline_id = %pipeline.id,
                    error = %e,
                    "failed to check claude process"
                );
            }
        }
    }

    events
}

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Otter Jobs Daemon (ojd)
//!
//! Background process that owns the event loop and dispatches work.

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

mod lifecycle;
mod protocol;
mod server;

use std::path::PathBuf;
use std::time::Duration;

use tokio::signal::unix::{signal, SignalKind};
use tokio::time::Instant;
use tracing::{error, info};

use crate::lifecycle::{check_sessions, Config, LifecycleError, SessionEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let project_root = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        std::env::current_dir()?
    };

    // Load configuration
    let config = Config::for_project(&project_root)?;

    // Write startup marker to log (before tracing setup, so CLI can find it)
    write_startup_marker(&config)?;

    // Set up logging
    let log_guard = setup_logging(&config)?;

    info!("Starting ojd for project: {}", project_root.display());

    // Start daemon
    let mut daemon = match lifecycle::startup(&config).await {
        Ok(d) => d,
        Err(e) => {
            // Write error synchronously (tracing is non-blocking and may not flush in time)
            write_startup_error(&config, &e);
            error!("Failed to start daemon: {}", e);
            drop(log_guard);
            return Err(e.into());
        }
    };

    // Set up signal handlers
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    info!(
        "Daemon ready, listening on {}",
        config.socket_path.display()
    );

    // Signal ready for parent process (e.g., systemd, CLI waiting for startup)
    println!("READY");

    // Session check interval - runs every 10 seconds
    let session_check_interval = Duration::from_secs(10);
    let mut last_session_check = Instant::now();

    // Main event loop
    loop {
        tokio::select! {
            // Accept client connections
            result = daemon.listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        if let Err(e) = server::handle_connection(&mut daemon, stream).await {
                            error!("Error handling connection: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error accepting connection: {}", e);
                    }
                }
            }

            // Process internal events (from effect execution)
            Some(event) = daemon.internal_events.recv() => {
                if let Err(e) = daemon.process_event(event).await {
                    error!("Error processing internal event: {}", e);
                }
            }

            // Heartbeat check interval (1 second) - also check sessions periodically
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if let Err(e) = daemon.check_heartbeats().await {
                    error!("Error checking heartbeats: {}", e);
                }

                // Periodic session checks (every 10 seconds)
                if last_session_check.elapsed() >= session_check_interval {
                    last_session_check = Instant::now();

                    let session_events = {
                        let state = daemon.state.lock().unwrap_or_else(|e| e.into_inner());
                        let sessions = oj_adapters::TmuxAdapter::new();
                        check_sessions(&sessions, &state).await
                    };

                    for event in session_events {
                        match event {
                            SessionEvent::TmuxExited { pipeline_id } => {
                                // tmux died - session is gone
                                if let Err(e) = daemon.runtime.handle_tmux_exited(&pipeline_id).await {
                                    error!("Error handling tmux exit for {}: {}", pipeline_id, e);
                                }
                            }
                            SessionEvent::ClaudeExited { pipeline_id } => {
                                // claude exited - trigger on_exit
                                if let Err(e) = daemon.runtime.handle_claude_exited(&pipeline_id).await {
                                    error!("Error handling claude exit for {}: {}", pipeline_id, e);
                                }
                            }
                        }
                    }
                }
            }

            // Graceful shutdown on SIGTERM
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
                daemon.shutdown().await?;
                break;
            }

            // Graceful shutdown on SIGINT
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down...");
                daemon.shutdown().await?;
                break;
            }
        }

        // Check if shutdown was requested via IPC
        if daemon.shutdown_requested {
            info!("Shutdown requested via IPC, shutting down...");
            daemon.shutdown().await?;
            break;
        }
    }

    info!("Daemon stopped");
    Ok(())
}

/// Startup marker prefix written to log before anything else.
/// CLI uses this to find where the current startup attempt begins.
/// Full format: "--- ojd: starting (pid: 12345) ---"
pub const STARTUP_MARKER_PREFIX: &str = "--- ojd: starting (pid: ";

/// Write startup marker to log file (appends to existing log)
fn write_startup_marker(config: &Config) -> Result<(), LifecycleError> {
    use std::io::Write;

    // Create log directory if needed
    if let Some(parent) = config.log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Append marker to log file with PID
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_path)?;
    writeln!(file, "{}{})", STARTUP_MARKER_PREFIX, std::process::id())?;

    Ok(())
}

/// Write startup error synchronously to log file.
/// This ensures the error is visible to the CLI even if the process exits quickly.
fn write_startup_error(config: &Config, error: &LifecycleError) {
    use std::io::Write;

    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_path)
    else {
        return;
    };
    let _ = writeln!(file, "ERROR Failed to start daemon: {}", error);
}

fn setup_logging(
    config: &Config,
) -> Result<tracing_appender::non_blocking::WorkerGuard, LifecycleError> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    // Create log directory if needed
    if let Some(parent) = config.log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Set up file appender
    let file_appender = tracing_appender::rolling::never(
        config.log_path.parent().ok_or(LifecycleError::NoStateDir)?,
        config
            .log_path
            .file_name()
            .ok_or(LifecycleError::NoStateDir)?,
    );
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Set up subscriber with env filter
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking))
        .init();

    Ok(guard)
}

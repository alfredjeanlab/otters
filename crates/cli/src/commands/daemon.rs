// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! `oj daemon` - Daemon management commands

use crate::client::{daemon_stop, find_project_root, DaemonClient};
use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon (foreground or background)
    Start {
        /// Run in foreground (useful for debugging)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
    /// View daemon logs
    Logs {
        /// Number of lines to show
        #[arg(long, default_value = "100")]
        lines: usize,
        /// Follow log output
        #[arg(long, short)]
        follow: bool,
    },
}

pub async fn daemon(args: DaemonArgs, project_root: Option<PathBuf>) -> Result<()> {
    let project_root = project_root.map_or_else(find_project_root, Ok)?;

    match args.command {
        DaemonCommand::Start { foreground } => start(&project_root, foreground).await,
        DaemonCommand::Stop => stop(&project_root).await,
        DaemonCommand::Status => status(&project_root).await,
        DaemonCommand::Logs { lines, follow } => logs(&project_root, lines, follow).await,
    }
}

async fn start(project_root: &Path, foreground: bool) -> Result<()> {
    if foreground {
        // Run daemon in foreground - spawn and wait
        let ojd_path = find_ojd_binary()?;
        let status = Command::new(&ojd_path).arg(project_root).status()?;
        if !status.success() {
            return Err(anyhow!("Daemon exited with status: {}", status));
        }
        return Ok(());
    }

    // Check if already running
    if let Ok(client) = DaemonClient::connect(project_root.to_path_buf()) {
        if let Ok((uptime, _, _)) = client.status().await {
            println!("Daemon already running (uptime: {}s)", uptime);
            return Ok(());
        }
    }

    // Start in background and verify it started
    match DaemonClient::connect_or_start(project_root.to_path_buf()) {
        Ok(_client) => {
            println!("Daemon started for {}", project_root.display());
            Ok(())
        }
        Err(e) => Err(anyhow!("{}", e)),
    }
}

async fn stop(project_root: &Path) -> Result<()> {
    match daemon_stop(project_root).await {
        Ok(true) => {
            println!("Daemon stopped");
            Ok(())
        }
        Ok(false) => {
            println!("Daemon not running");
            Ok(())
        }
        Err(e) => Err(anyhow!("Failed to stop daemon: {}", e)),
    }
}

async fn status(project_root: &Path) -> Result<()> {
    let client = match DaemonClient::connect(project_root.to_path_buf()) {
        Ok(c) => c,
        Err(_) => {
            println!("Daemon not running");
            return Ok(());
        }
    };

    let (uptime, pipelines, sessions) = client.status().await?;
    let version = client
        .hello()
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    let uptime_str = format_uptime(uptime);
    println!("Status: running");
    println!("Version: {}", version);
    println!("Uptime: {}", uptime_str);
    println!("Pipelines: {} active", pipelines);
    println!("Sessions: {} active", sessions);

    Ok(())
}

async fn logs(project_root: &Path, lines: usize, follow: bool) -> Result<()> {
    let log_path = get_log_path(project_root)?;

    if !log_path.exists() {
        println!("No log file found at {}", log_path.display());
        return Ok(());
    }

    let mut cmd = if follow {
        let mut c = Command::new("tail");
        c.args(["-f", "-n", &lines.to_string()]);
        c.arg(&log_path);
        c
    } else {
        let mut c = Command::new("tail");
        c.args(["-n", &lines.to_string()]);
        c.arg(&log_path);
        c
    };

    cmd.status()?;
    Ok(())
}

fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

fn find_ojd_binary() -> Result<PathBuf> {
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

fn get_log_path(project_root: &Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};

    let canonical = project_root.canonicalize()?;

    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    let hash: String = result[..8].iter().map(|b| format!("{:02x}", b)).collect();

    let state_dir = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".local/state"))
                .unwrap_or_else(|_| PathBuf::from("."))
        })
        .join("oj");

    Ok(state_dir.join("projects").join(&hash).join("daemon.log"))
}

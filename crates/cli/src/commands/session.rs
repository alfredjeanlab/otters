//! Session commands

use anyhow::bail;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List all sessions
    List,
    /// Show details of a session
    Show {
        /// Session name
        name: String,
    },
    /// Nudge an idle session
    Nudge {
        /// Session name
        name: String,
        /// Message to send
        #[arg(default_value = "Are you still working?")]
        message: String,
    },
    /// Kill a session
    Kill {
        /// Session name
        name: String,
    },
}

pub async fn handle(command: SessionCommand) -> anyhow::Result<()> {
    match command {
        SessionCommand::List => list_sessions().await,
        SessionCommand::Show { name } => show_session(name).await,
        SessionCommand::Nudge { name, message } => nudge_session(name, message).await,
        SessionCommand::Kill { name } => kill_session(name).await,
    }
}

async fn list_sessions() -> anyhow::Result<()> {
    // For now, just list tmux sessions with our prefix
    let output = tokio::process::Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let sessions = String::from_utf8_lossy(&output.stdout);
            let oj_sessions: Vec<_> = sessions
                .lines()
                .filter(|s| s.starts_with("oj-"))
                .collect();

            if oj_sessions.is_empty() {
                println!("No oj sessions found.");
            } else {
                println!("{:<30} {}", "SESSION", "STATUS");
                println!("{}", "-".repeat(40));
                for session in oj_sessions {
                    println!("{:<30} running", session);
                }
            }
        }
        Ok(_) | Err(_) => {
            println!("No tmux sessions found (tmux may not be running).");
        }
    }

    Ok(())
}

async fn show_session(name: String) -> anyhow::Result<()> {
    let session_name = if name.starts_with("oj-") {
        name
    } else {
        format!("oj-{}", name)
    };

    // Check if session exists
    let exists = tokio::process::Command::new("tmux")
        .args(["has-session", "-t", &session_name])
        .status()
        .await?
        .success();

    if !exists {
        bail!("Session '{}' not found", session_name);
    }

    println!("Session: {}", session_name);

    // Get pane content
    let output = tokio::process::Command::new("tmux")
        .args(["capture-pane", "-t", &session_name, "-p", "-S", "-20"])
        .output()
        .await?;

    if output.status.success() {
        println!();
        println!("Recent output:");
        println!("{}", "-".repeat(40));
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

async fn nudge_session(name: String, message: String) -> anyhow::Result<()> {
    let session_name = if name.starts_with("oj-") {
        name
    } else {
        format!("oj-{}", name)
    };

    // Send message to session
    tokio::process::Command::new("tmux")
        .args(["send-keys", "-t", &session_name, &format!("# {}\n", message)])
        .status()
        .await?;

    println!("Sent nudge to session '{}'", session_name);

    Ok(())
}

async fn kill_session(name: String) -> anyhow::Result<()> {
    let session_name = if name.starts_with("oj-") {
        name
    } else {
        format!("oj-{}", name)
    };

    tokio::process::Command::new("tmux")
        .args(["kill-session", "-t", &session_name])
        .status()
        .await?;

    println!("Killed session '{}'", session_name);

    Ok(())
}

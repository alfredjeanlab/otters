// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Daemon command for background polling and tick loops

use crate::adapters::make_engine;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

#[derive(clap::Args)]
pub struct DaemonArgs {
    /// Interval for polling sessions (seconds)
    #[arg(long, default_value = "5")]
    poll_interval: u64,

    /// Interval for ticking tasks (seconds)
    #[arg(long, default_value = "30")]
    tick_interval: u64,

    /// Interval for ticking queue (seconds)
    #[arg(long, default_value = "10")]
    queue_interval: u64,

    /// Run once and exit (for testing)
    #[arg(long)]
    once: bool,
}

pub async fn handle(args: DaemonArgs) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        eprintln!("\nShutting down daemon...");
        r.store(false, Ordering::SeqCst);
    })?;

    println!("Starting oj daemon");
    println!("  Poll interval: {}s", args.poll_interval);
    println!("  Tick interval: {}s", args.tick_interval);
    println!("  Queue interval: {}s", args.queue_interval);
    println!();

    let mut engine = make_engine()?;
    engine.load()?;

    let mut poll_timer = interval(Duration::from_secs(args.poll_interval));
    let mut tick_timer = interval(Duration::from_secs(args.tick_interval));
    let mut queue_timer = interval(Duration::from_secs(args.queue_interval));

    // Skip initial immediate tick
    poll_timer.tick().await;
    tick_timer.tick().await;
    queue_timer.tick().await;

    if args.once {
        // Single iteration for testing
        run_once(&mut engine).await?;
        return Ok(());
    }

    loop {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        tokio::select! {
            _ = poll_timer.tick() => {
                if let Err(e) = engine.poll_sessions().await {
                    eprintln!("Poll sessions error: {}", e);
                }
            }
            _ = tick_timer.tick() => {
                if let Err(e) = engine.tick_all_tasks().await {
                    eprintln!("Tick tasks error: {}", e);
                }
            }
            _ = queue_timer.tick() => {
                if let Err(e) = engine.tick_queue("merge") {
                    eprintln!("Tick queue error: {}", e);
                }
            }
        }
    }

    println!("Daemon stopped");
    Ok(())
}

async fn run_once(
    engine: &mut oj_core::engine::Engine<oj_core::RealAdapters, oj_core::clock::SystemClock>,
) -> Result<()> {
    println!("Running single daemon iteration...");

    if let Err(e) = engine.poll_sessions().await {
        eprintln!("Poll sessions error: {}", e);
    }

    if let Err(e) = engine.tick_all_tasks().await {
        eprintln!("Tick tasks error: {}", e);
    }

    if let Err(e) = engine.tick_queue("merge") {
        eprintln!("Tick queue error: {}", e);
    }

    println!("Done");
    Ok(())
}

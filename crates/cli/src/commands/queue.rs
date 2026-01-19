// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Queue commands

use clap::Subcommand;
use oj_core::clock::SystemClock;
use oj_core::storage::WalStore;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;

#[derive(Subcommand)]
pub enum QueueCommand {
    /// List items in a queue
    List {
        /// Queue name
        #[arg(default_value = "merges")]
        name: String,
    },
    /// Add an item to a queue
    Add {
        /// Queue name
        name: String,
        /// Key=value pairs for the item data
        #[arg(num_args = 1.., value_parser = parse_key_value)]
        data: Vec<(String, String)>,
        /// Priority (higher = processed first)
        #[arg(long, default_value = "0")]
        priority: i32,
    },
    /// Take the next item from a queue (claims it for processing)
    Take {
        /// Queue name
        #[arg(default_value = "merges")]
        name: String,
        /// Visibility timeout in seconds (default: 300)
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
    /// Complete a claimed item
    Complete {
        /// Queue name
        #[arg(default_value = "merges")]
        name: String,
        /// Claim ID to complete (returned by take command)
        #[arg(long)]
        claim_id: String,
    },
}

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid key=value: no '=' found in '{}'", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[derive(Serialize)]
struct QueueItemInfo {
    id: String,
    priority: i32,
    attempts: u32,
    data: BTreeMap<String, String>,
}

impl fmt::Display for QueueItemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data_str: Vec<_> = self
            .data
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        write!(
            f,
            "{:<20} pri={:<3} attempts={:<2} {}",
            self.id,
            self.priority,
            self.attempts,
            data_str.join(" ")
        )
    }
}

pub async fn handle(command: QueueCommand) -> anyhow::Result<()> {
    match command {
        QueueCommand::List { name } => list_queue(name).await,
        QueueCommand::Add {
            name,
            data,
            priority,
        } => add_to_queue(name, data, priority).await,
        QueueCommand::Take { name, timeout } => take_from_queue(name, timeout).await,
        QueueCommand::Complete { name, claim_id } => complete_item(name, claim_id).await,
    }
}

async fn list_queue(name: String) -> anyhow::Result<()> {
    let store = WalStore::open_default(Path::new(".build/operations"))?;

    let queue = match store.load_queue(&name) {
        Ok(q) => q,
        Err(_) => {
            println!("Queue '{}' is empty or does not exist.", name);
            return Ok(());
        }
    };

    println!("Queue: {}", name);
    println!();

    if !queue.claimed.is_empty() {
        println!("Currently claimed ({}):", queue.claimed.len());
        for claimed in &queue.claimed {
            let info = QueueItemInfo {
                id: claimed.item.id.clone(),
                priority: claimed.item.priority,
                attempts: claimed.item.attempts,
                data: claimed.item.data.clone(),
            };
            println!("  {} (claim: {})", info, claimed.claim_id);
        }
        println!();
    }

    if queue.items.is_empty() {
        println!("No items waiting.");
    } else {
        println!("Waiting ({} items):", queue.items.len());
        for item in &queue.items {
            let info = QueueItemInfo {
                id: item.id.clone(),
                priority: item.priority,
                attempts: item.attempts,
                data: item.data.clone(),
            };
            println!("  {}", info);
        }
    }

    if !queue.dead_letters.is_empty() {
        println!();
        println!("Dead letters ({}):", queue.dead_letters.len());
        for dl in &queue.dead_letters {
            println!("  {} - {}", dl.item.id, dl.reason);
        }
    }

    Ok(())
}

async fn add_to_queue(
    name: String,
    data: Vec<(String, String)>,
    priority: i32,
) -> anyhow::Result<()> {
    let mut store = WalStore::open_default(Path::new(".build/operations"))?;

    let data_map: BTreeMap<String, String> = data.into_iter().collect();
    let item_id = format!(
        "{}-{}",
        name,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );

    // Use granular queue_push operation (auto-creates queue if needed)
    store.queue_push(&name, &item_id, data_map.clone(), priority, 3)?;

    println!("Added item '{}' to queue '{}'", item_id, name);
    for (k, v) in data_map {
        println!("  {}={}", k, v);
    }

    Ok(())
}

async fn take_from_queue(name: String, timeout_secs: u64) -> anyhow::Result<()> {
    let mut store = WalStore::open_default(Path::new(".build/operations"))?;
    let clock = SystemClock;

    let queue = store.load_queue(&name)?;

    if queue.items.is_empty() {
        if !queue.claimed.is_empty() {
            println!(
                "Queue '{}' has {} item(s) currently claimed.",
                name,
                queue.claimed.len()
            );
        } else {
            println!("Queue '{}' is empty.", name);
        }
        return Ok(());
    }

    // Get the first item's ID for the claim
    let item_id = queue.items[0].id.clone();
    let claim_id = format!("cli-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // Claim the item using the granular operation
    let (new_queue, _effects) = queue.transition(
        oj_core::queue::QueueEvent::Claim {
            claim_id: claim_id.clone(),
            visibility_timeout: Some(Duration::from_secs(timeout_secs)),
        },
        &clock,
    );

    // Persist the claim
    store.queue_claim(&name, &item_id, &claim_id, timeout_secs)?;

    // Find the claimed item to display
    if let Some(claimed) = new_queue.claimed.iter().find(|c| c.claim_id == claim_id) {
        println!("Claimed item '{}' from queue '{}'", claimed.item.id, name);
        println!("Claim ID: {}", claim_id);
        println!("Visibility timeout: {}s", timeout_secs);
        for (k, v) in &claimed.item.data {
            println!("  {}={}", k, v);
        }
    }

    Ok(())
}

async fn complete_item(name: String, claim_id: String) -> anyhow::Result<()> {
    let mut store = WalStore::open_default(Path::new(".build/operations"))?;

    // Use granular queue_complete operation
    store.queue_complete(&name, &claim_id)?;

    println!("Completed claim '{}' in queue '{}'", claim_id, name);

    Ok(())
}

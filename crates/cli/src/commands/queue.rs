// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Queue commands

use clap::Subcommand;
use oj_core::queue::{Queue, QueueItem};
use oj_core::storage::JsonStore;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;

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
    /// Take the next item from a queue
    Take {
        /// Queue name
        #[arg(default_value = "merges")]
        name: String,
    },
    /// Complete the current item
    Complete {
        /// Queue name
        #[arg(default_value = "merges")]
        name: String,
        /// Item ID to complete
        id: String,
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
    data: HashMap<String, String>,
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
        QueueCommand::Take { name } => take_from_queue(name).await,
        QueueCommand::Complete { name, id } => complete_item(name, id).await,
    }
}

async fn list_queue(name: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    let queue = match store.load_queue(&name) {
        Ok(q) => q,
        Err(_) => {
            println!("Queue '{}' is empty or does not exist.", name);
            return Ok(());
        }
    };

    println!("Queue: {}", name);
    println!();

    if let Some(ref processing) = queue.processing {
        println!("Currently processing:");
        let info = QueueItemInfo {
            id: processing.id.clone(),
            priority: processing.priority,
            attempts: processing.attempts,
            data: processing.data.clone(),
        };
        println!("  {}", info);
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
    let store = JsonStore::open(".build/operations")?;

    let queue = store
        .load_queue(&name)
        .unwrap_or_else(|_| Queue::new(&name));

    let data_map: HashMap<String, String> = data.into_iter().collect();
    let item_id = format!(
        "{}-{}",
        name,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let item = QueueItem::with_priority(&item_id, data_map.clone(), priority);

    let queue = queue.push(item);
    store.save_queue(&name, &queue)?;

    println!("Added item '{}' to queue '{}'", item_id, name);
    for (k, v) in data_map {
        println!("  {}={}", k, v);
    }

    Ok(())
}

async fn take_from_queue(name: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    let queue = store.load_queue(&name)?;
    let (queue, item) = queue.take();

    match item {
        Some(item) => {
            store.save_queue(&name, &queue)?;
            println!("Took item '{}' from queue '{}'", item.id, name);
            for (k, v) in &item.data {
                println!("  {}={}", k, v);
            }
        }
        None => {
            if queue.is_processing() {
                println!("Queue '{}' is currently processing an item.", name);
            } else {
                println!("Queue '{}' is empty.", name);
            }
        }
    }

    Ok(())
}

async fn complete_item(name: String, id: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    let queue = store.load_queue(&name)?;
    let queue = queue.complete(&id);
    store.save_queue(&name, &queue)?;

    println!("Completed item '{}' in queue '{}'", id, name);

    Ok(())
}

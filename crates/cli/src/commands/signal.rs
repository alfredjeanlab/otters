//! Signal commands (done, checkpoint)

use anyhow::bail;
use oj_core::clock::SystemClock;
use oj_core::pipeline::PipelineEvent;
use oj_core::storage::JsonStore;

pub async fn handle_done(error: Option<String>) -> anyhow::Result<()> {
    // Get current task from environment
    let task = std::env::var("OTTER_TASK").unwrap_or_else(|_| {
        // Try to detect from current directory
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    });

    let phase = std::env::var("OTTER_PHASE").unwrap_or_else(|_| "unknown".to_string());

    // Try multiple locations for the store
    let store = find_store()?;

    let pipeline = match store.load_pipeline(&task) {
        Ok(p) => p,
        Err(_) => {
            // Try to find by prefix match
            let ids = store.list_pipelines()?;
            let matching: Vec<_> = ids.iter().filter(|id| id.contains(&task)).collect();

            if matching.len() == 1 {
                store.load_pipeline(matching[0])?
            } else if matching.is_empty() {
                bail!("No pipeline found for task '{}'. Available: {:?}", task, ids);
            } else {
                bail!("Multiple pipelines match '{}': {:?}", task, matching);
            }
        }
    };

    let event = match &error {
        Some(reason) => PipelineEvent::PhaseFailed { reason: reason.clone() },
        None => PipelineEvent::PhaseComplete,
    };

    let clock = SystemClock;
    let (pipeline, _effects) = pipeline.transition(event, &clock);
    store.save_pipeline(&pipeline)?;

    match &error {
        Some(reason) => {
            println!("Pipeline '{}' failed: {}", task, reason);
            println!("Phase: {} -> failed", phase);
        }
        None => {
            println!("Pipeline '{}' phase complete", task);
            println!("Phase: {} -> {}", phase, pipeline.phase.name());
        }
    }

    Ok(())
}

pub async fn handle_checkpoint() -> anyhow::Result<()> {
    let task = std::env::var("OTTER_TASK").unwrap_or_else(|_| "unknown".to_string());
    let phase = std::env::var("OTTER_PHASE").unwrap_or_else(|_| "unknown".to_string());

    println!("Checkpoint saved for pipeline '{}' in phase '{}'", task, phase);
    println!("(Note: Full checkpoint support not yet implemented)");

    Ok(())
}

fn find_store() -> anyhow::Result<JsonStore> {
    // Try current directory first
    if std::path::Path::new(".build/operations").exists() {
        return Ok(JsonStore::open(".build/operations")?);
    }

    // Try parent directories
    let mut dir = std::env::current_dir()?;
    for _ in 0..5 {
        let store_path = dir.join(".build/operations");
        if store_path.exists() {
            return Ok(JsonStore::open(store_path)?);
        }
        if !dir.pop() {
            break;
        }
    }

    // Default to current directory
    Ok(JsonStore::open(".build/operations")?)
}

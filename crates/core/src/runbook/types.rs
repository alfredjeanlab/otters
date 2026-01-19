// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Raw runbook types that mirror TOML structure exactly.
//!
//! These types are used for parsing only. They are converted to validated
//! runtime types by the loader after validation.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

/// A runbook containing all primitives.
///
/// All fields are optional because a runbook may only define
/// a subset of primitives (e.g., just a command and queue).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawRunbook {
    /// Command entrypoints (user-facing CLI commands)
    pub command: HashMap<String, RawCommand>,
    /// Workers that process queues
    pub worker: HashMap<String, RawWorker>,
    /// Work queues
    pub queue: HashMap<String, RawQueue>,
    /// Multi-phase pipelines
    pub pipeline: HashMap<String, RawPipeline>,
    /// Task definitions for Claude invocations
    pub task: HashMap<String, RawTask>,
    /// Guard conditions (pre/post checks)
    pub guard: HashMap<String, RawGuard>,
    /// Strategy fallback chains
    pub strategy: HashMap<String, RawStrategy>,
    /// Distributed locks
    pub lock: HashMap<String, RawLock>,
    /// Counting semaphores
    pub semaphore: HashMap<String, RawSemaphore>,
    /// Global configuration values
    pub config: HashMap<String, toml::Value>,
    /// Named shell functions
    pub functions: BTreeMap<String, String>,
    /// Event handlers
    pub events: Option<RawEvents>,

    // Scheduling primitives
    /// Runbook metadata
    pub meta: Option<RawMeta>,
    /// Scheduled cron jobs
    pub cron: HashMap<String, RawCron>,
    /// Named actions with cooldowns
    pub action: HashMap<String, RawAction>,
    /// Condition watchers
    pub watcher: HashMap<String, RawWatcher>,
    /// Resource scanners
    pub scanner: HashMap<String, RawScanner>,
}

/// A command entrypoint.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawCommand {
    /// Argument specification (e.g., "<name> <prompt> [--priority <n>]")
    pub args: Option<String>,
    /// Short aliases for arguments (e.g., { p = "priority" })
    pub aliases: BTreeMap<String, String>,
    /// Default argument values
    pub defaults: HashMap<String, toml::Value>,
    /// Shell command to run
    pub run: Option<String>,
}

/// A worker that processes a queue.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWorker {
    /// Name of the queue to process
    pub queue: Option<String>,
    /// Handler (pipeline or task name)
    pub handler: Option<String>,
    /// Maximum concurrent items
    pub concurrency: Option<u32>,
    /// What to do when idle (e.g., "wait:30s", "exit")
    pub idle_action: Option<String>,
    /// Events that wake this worker
    pub wake_on: Vec<String>,
    /// Action when unhealthy (e.g., "restart")
    pub on_unhealthy: Option<String>,
}

/// A work queue.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawQueue {
    /// Command to fetch queue items dynamically
    pub source: Option<String>,
    /// Filter expression for source items
    pub filter: Option<String>,
    /// Sort order (e.g., "priority DESC, created_at ASC")
    pub order: Option<String>,
    /// How long items are invisible after being taken
    #[serde(with = "humantime_serde", default)]
    pub visibility_timeout: Option<Duration>,
    /// Maximum retry attempts
    pub max_retries: Option<u32>,
    /// Action when retries exhausted (e.g., "dead", "escalate")
    pub on_exhaust: Option<String>,
    /// Dead letter queue configuration
    pub dead: Option<RawDeadLetterConfig>,
}

/// Dead letter queue configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawDeadLetterConfig {
    /// How long to keep dead items
    pub retention: Option<String>,
    /// Action when item is added to dead letter
    pub on_add: Option<String>,
}

/// A multi-phase pipeline.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawPipeline {
    /// Required inputs (variable names)
    pub inputs: Vec<String>,
    /// Default values for variables (can reference inputs)
    pub defaults: BTreeMap<String, String>,
    /// Pipeline phases (ordered)
    pub phase: Vec<RawPhase>,
}

/// A single phase in a pipeline.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawPhase {
    /// Phase name (required)
    pub name: String,
    /// Shell command to run
    pub run: Option<String>,
    /// Task to spawn
    pub task: Option<String>,
    /// Strategy to execute
    pub strategy: Option<String>,
    /// Pre-conditions (guard names)
    pub pre: Vec<String>,
    /// Post-conditions (guard names)
    pub post: Vec<String>,
    /// Lock to acquire
    pub lock: Option<String>,
    /// Semaphore to acquire
    pub semaphore: Option<String>,
    /// Next phase on success
    pub next: Option<String>,
    /// Action on failure (phase name, "escalate", strategy name)
    pub on_fail: Option<String>,
}

/// A task definition for Claude invocations.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawTask {
    /// Command to run (e.g., "claude --print")
    pub command: Option<String>,
    /// Path to prompt template file
    pub prompt_file: Option<String>,
    /// Inline prompt template
    pub prompt: Option<String>,
    /// Environment variables
    pub env: BTreeMap<String, String>,
    /// Working directory
    pub cwd: Option<String>,
    /// Heartbeat source (e.g., "output", "api")
    pub heartbeat: Option<String>,
    /// Maximum execution time
    #[serde(with = "humantime_serde", default)]
    pub timeout: Option<Duration>,
    /// Time without output before considering stuck
    #[serde(with = "humantime_serde", default)]
    pub idle_timeout: Option<Duration>,
    /// Actions when stuck (string or array)
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub on_stuck: Vec<String>,
    /// Action when timed out
    pub on_timeout: Option<String>,
    /// How often to checkpoint
    #[serde(with = "humantime_serde", default)]
    pub checkpoint_interval: Option<Duration>,
    /// Checkpoint command
    pub checkpoint: Option<String>,
}

/// A guard condition.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawGuard {
    /// Shell command that exits 0 if condition is met
    pub condition: Option<String>,
    /// Events that may unblock this guard
    pub wake_on: Vec<String>,
    /// Maximum time to wait
    #[serde(with = "humantime_serde", default)]
    pub timeout: Option<Duration>,
    /// Action when timeout (e.g., "escalate", "fail")
    pub on_timeout: Option<String>,
    /// Retry configuration
    pub retry: Option<RawRetry>,
}

/// Retry configuration for guards.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawRetry {
    /// Maximum retry attempts
    pub max: Option<u32>,
    /// Interval between retries
    #[serde(with = "humantime_serde", default)]
    pub interval: Option<Duration>,
}

/// A strategy (fallback chain).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawStrategy {
    /// Command to capture checkpoint value
    pub checkpoint: Option<String>,
    /// Action when all attempts exhausted
    pub on_exhaust: Option<String>,
    /// Ordered list of attempts
    pub attempt: Vec<RawAttempt>,
}

/// An attempt within a strategy.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawAttempt {
    /// Attempt name (required)
    pub name: String,
    /// Shell command to run
    pub run: Option<String>,
    /// Task to spawn
    pub task: Option<String>,
    /// Maximum execution time
    #[serde(with = "humantime_serde", default)]
    pub timeout: Option<Duration>,
    /// Rollback command (has access to {checkpoint})
    pub rollback: Option<String>,
}

/// A distributed lock.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawLock {
    /// Lock timeout
    #[serde(with = "humantime_serde", default)]
    pub timeout: Option<Duration>,
    /// Heartbeat interval
    #[serde(with = "humantime_serde", default)]
    pub heartbeat: Option<Duration>,
    /// Actions when lock becomes stale
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub on_stale: Vec<String>,
}

/// A counting semaphore.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawSemaphore {
    /// Maximum permits
    pub max: Option<u32>,
    /// Per-slot timeout
    #[serde(with = "humantime_serde", default)]
    pub slot_timeout: Option<Duration>,
    /// Per-slot heartbeat interval
    #[serde(with = "humantime_serde", default)]
    pub slot_heartbeat: Option<Duration>,
    /// Action for orphaned slots
    pub on_orphan: Option<String>,
    /// Action for work in orphaned slots
    pub on_orphan_work: Option<String>,
}

/// Event handlers.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawEvents {
    /// All other event handlers (dynamic keys)
    #[serde(flatten)]
    pub handlers: BTreeMap<String, String>,
}

// ============================================================================
// Scheduling Primitives
// ============================================================================

/// Runbook metadata.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawMeta {
    /// Runbook name
    pub name: Option<String>,
    /// Runbook description
    pub description: Option<String>,
}

/// A scheduled cron job.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawCron {
    /// Interval between runs
    #[serde(default, with = "humantime_serde::option")]
    pub interval: Option<Duration>,
    /// Whether the cron is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Shell command to run
    pub run: Option<String>,
    /// Watchers to run on each tick
    #[serde(default)]
    pub watchers: Vec<String>,
    /// Scanners to run on each tick
    #[serde(default)]
    pub scanners: Vec<String>,
}

/// A named action with cooldown enforcement.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawAction {
    /// Cooldown duration between invocations
    #[serde(default, with = "humantime_serde::option")]
    pub cooldown: Option<Duration>,
    /// Shell command to run
    pub command: Option<String>,
    /// Task to invoke
    pub task: Option<String>,
    /// Arguments for command/task
    #[serde(default)]
    pub args: HashMap<String, toml::Value>,
    /// Decision rules (evaluated in order)
    #[serde(default)]
    pub rules: Vec<RawDecisionRule>,
}

/// A decision rule for rule-based action execution.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawDecisionRule {
    /// Condition to evaluate
    #[serde(rename = "if")]
    pub condition: Option<String>,
    /// Whether this is an else clause
    #[serde(rename = "else")]
    pub is_else: Option<bool>,
    /// Action to take if condition matches
    #[serde(default)]
    pub then: String,
    /// Delay before executing
    #[serde(default, with = "humantime_serde::option")]
    pub delay: Option<Duration>,
}

/// A condition watcher with response chain.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcher {
    /// What to monitor
    #[serde(default)]
    pub source: RawWatcherSource,
    /// When to trigger
    #[serde(default)]
    pub condition: RawWatcherCondition,
    /// How often to check
    #[serde(default, with = "humantime_serde::option")]
    pub check_interval: Option<Duration>,
    /// Events that wake this watcher
    #[serde(default)]
    pub wake_on: Vec<String>,
    /// Response chain (escalates on failure)
    #[serde(default)]
    pub response: Vec<RawWatcherResponse>,
}

/// Watcher source configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherSource {
    /// Source type (session, queue, events)
    #[serde(rename = "type", default)]
    pub source_type: String,
    /// Pattern for matching (e.g., "*" for all sessions)
    pub pattern: Option<String>,
    /// Named resource to watch
    pub name: Option<String>,
}

/// Watcher condition configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherCondition {
    /// Condition type (idle, exceeds, consecutive_failures, matches)
    #[serde(rename = "type", default)]
    pub condition_type: String,
    /// Duration threshold (for idle conditions)
    #[serde(default, with = "humantime_serde::option")]
    pub threshold: Option<Duration>,
    /// Count threshold (for consecutive_failures, exceeds)
    pub count: Option<u32>,
    /// Pattern to match (for matches condition)
    pub pattern: Option<String>,
}

/// A watcher response action.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherResponse {
    /// Action to trigger
    #[serde(default)]
    pub action: String,
    /// Delay before triggering
    #[serde(default, with = "humantime_serde::option")]
    pub delay: Option<Duration>,
    /// Only trigger if previous response failed
    #[serde(default)]
    pub requires_previous_failure: bool,
    /// Override args for the action
    #[serde(default)]
    pub args: HashMap<String, toml::Value>,
}

/// A resource scanner with cleanup.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScanner {
    /// What to scan
    #[serde(default)]
    pub source: RawScannerSource,
    /// Condition for cleanup
    #[serde(default)]
    pub condition: RawScannerCondition,
    /// Cleanup action to take
    #[serde(default)]
    pub cleanup: RawCleanupAction,
    /// Scan interval
    #[serde(default, with = "humantime_serde::option")]
    pub interval: Option<Duration>,
}

/// Scanner source configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScannerSource {
    /// Source type (locks, semaphores, sessions, worktrees, queue, pipelines, tasks)
    #[serde(rename = "type", default)]
    pub source_type: String,
    /// Named resource (for queue source)
    pub name: Option<String>,
}

/// Scanner condition configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScannerCondition {
    /// Condition type (stale, orphaned, exceeded_attempts, terminal_for)
    #[serde(rename = "type", default)]
    pub condition_type: String,
    /// Duration threshold
    #[serde(default, with = "humantime_serde::option")]
    pub threshold: Option<Duration>,
    /// Maximum count (for exceeded_attempts)
    pub max: Option<u32>,
}

/// Scanner cleanup action configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawCleanupAction {
    /// Action type (release, delete, archive, dead_letter)
    #[serde(rename = "type", default)]
    pub action_type: String,
    /// Destination path (for archive)
    pub destination: Option<String>,
}

/// Custom deserializer for fields that can be a string or array of strings.
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct StringOrVec;

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![v.to_owned()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(item) = seq.next_element()? {
                vec.push(item);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

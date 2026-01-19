# Epic 9a: Runbook Scheduling Types

**Depends on**: None
**Blocks**: Epic 9b (Engine Integration), Epic 9f (Cross-Runbook Validation)
**Root Feature:** `otters-7906`

## Problem Statement

System runbooks (watchdog.toml, janitor.toml, triager.toml) define scheduling primitives (`watcher`, `scanner`, `cron`, `action`, `meta`) but `RawRunbook` in `types.rs` is missing these fields. The TOML files exist but cannot be parsed by the runbook system.

## Goal

Enable parsing of system runbooks so scheduling primitives can be loaded and validated.

## Implementation

### 1. Add Types to `crates/core/src/runbook/types.rs`

```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawRunbook {
    // ... existing fields ...

    // NEW: Scheduling primitives
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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawMeta {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawCron {
    #[serde(default, with = "humantime_serde::option")]
    pub interval: Option<Duration>,
    #[serde(default)]
    pub enabled: bool,
    pub run: Option<String>,
    #[serde(default)]
    pub watchers: Vec<String>,
    #[serde(default)]
    pub scanners: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawAction {
    #[serde(default, with = "humantime_serde::option")]
    pub cooldown: Option<Duration>,
    pub command: Option<String>,
    pub task: Option<String>,
    #[serde(default)]
    pub args: HashMap<String, toml::Value>,
    #[serde(default)]
    pub rules: Vec<RawDecisionRule>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawDecisionRule {
    #[serde(rename = "if")]
    pub condition: Option<String>,
    #[serde(rename = "else")]
    pub is_else: Option<bool>,
    #[serde(default)]
    pub then: String,
    #[serde(default, with = "humantime_serde::option")]
    pub delay: Option<Duration>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcher {
    #[serde(default)]
    pub source: RawWatcherSource,
    #[serde(default)]
    pub condition: RawWatcherCondition,
    #[serde(default, with = "humantime_serde::option")]
    pub check_interval: Option<Duration>,
    #[serde(default)]
    pub wake_on: Vec<String>,
    #[serde(default)]
    pub response: Vec<RawWatcherResponse>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherSource {
    #[serde(rename = "type", default)]
    pub source_type: String,
    pub pattern: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherCondition {
    #[serde(rename = "type", default)]
    pub condition_type: String,
    #[serde(default, with = "humantime_serde::option")]
    pub threshold: Option<Duration>,
    pub count: Option<u32>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawWatcherResponse {
    #[serde(default)]
    pub action: String,
    #[serde(default, with = "humantime_serde::option")]
    pub delay: Option<Duration>,
    #[serde(default)]
    pub requires_previous_failure: bool,
    #[serde(default)]
    pub args: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScanner {
    #[serde(default)]
    pub source: RawScannerSource,
    #[serde(default)]
    pub condition: RawScannerCondition,
    #[serde(default)]
    pub cleanup: RawCleanupAction,
    #[serde(default, with = "humantime_serde::option")]
    pub interval: Option<Duration>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScannerSource {
    #[serde(rename = "type", default)]
    pub source_type: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawScannerCondition {
    #[serde(rename = "type", default)]
    pub condition_type: String,
    #[serde(default, with = "humantime_serde::option")]
    pub threshold: Option<Duration>,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawCleanupAction {
    #[serde(rename = "type", default)]
    pub action_type: String,
    pub destination: Option<String>,
}
```

### 2. Add Loader Conversions in `crates/core/src/runbook/loader.rs`

```rust
use crate::scheduling::{
    ActionConfig, ActionExecution, CronConfig, DecisionRule,
    ScannerCondition, ScannerConfig, ScannerSource, CleanupAction,
    WatcherCondition, WatcherConfig, WatcherResponse, WatcherSource,
};

impl RunbookLoader {
    fn load_cron(&self, name: &str, raw: &RawCron) -> Result<CronConfig, LoaderError> {
        // Extract required interval, build CronConfig with watchers/scanners
    }

    fn load_action(&self, name: &str, raw: &RawAction) -> Result<ActionConfig, LoaderError> {
        // Extract required cooldown, attach command/task/rules based on which is present
    }

    fn load_decision_rule(&self, raw: &RawDecisionRule) -> Result<DecisionRule, LoaderError> {
        // Build DecisionRule from condition, is_else flag, delay
    }

    fn load_watcher(&self, name: &str, raw: &RawWatcher) -> Result<WatcherConfig, LoaderError> {
        // Load source/condition, extract check_interval, collect responses
    }

    fn load_watcher_source(&self, raw: &RawWatcherSource) -> Result<WatcherSource, LoaderError> {
        // Match source_type: "session" | "queue" | "events" -> WatcherSource variant
    }

    fn load_watcher_condition(&self, raw: &RawWatcherCondition) -> Result<WatcherCondition, LoaderError> {
        // Match condition_type: "idle" | "exceeds" | "consecutive_failures" | "matches"
    }

    fn load_watcher_response(&self, raw: &RawWatcherResponse) -> Result<WatcherResponse, LoaderError> {
        // Direct field mapping to WatcherResponse struct
    }

    fn load_scanner(&self, name: &str, raw: &RawScanner) -> Result<ScannerConfig, LoaderError> {
        // Load source/condition/cleanup, extract required interval
    }

    fn load_scanner_source(&self, raw: &RawScannerSource) -> Result<ScannerSource, LoaderError> {
        // Match source_type: "locks" | "semaphores" | "sessions" | "worktrees" | "queue"
    }

    fn load_scanner_condition(&self, raw: &RawScannerCondition) -> Result<ScannerCondition, LoaderError> {
        // Match condition_type: "stale" | "orphaned" | "exceeds_retries"
    }

    fn load_cleanup_action(&self, raw: &RawCleanupAction) -> Result<CleanupAction, LoaderError> {
        // Match action_type: "release" | "delete" | "archive" | "dead_letter"
    }
}
```

### 3. Add Validation Rules in `crates/core/src/runbook/validator.rs`

```rust
impl RunbookValidator {
    fn validate_scheduling(&self, runbook: &RawRunbook) -> Vec<ValidationError> {
        // Check watcher.response[].action references exist in runbook.action
        // Check cron.watchers[] references exist in runbook.watcher
        // Check cron.scanners[] references exist in runbook.scanner
        // Return collected UndefinedReference errors
    }
}
```

## Files

- `crates/core/src/runbook/types.rs` - Add scheduling types
- `crates/core/src/runbook/loader.rs` - Add conversion functions
- `crates/core/src/runbook/validator.rs` - Add validation rules
- `crates/core/src/runbook/types_tests.rs` - Test parsing system runbooks
- `crates/core/src/runbook/loader_tests.rs` - Test loading scheduling primitives

## Tests

```rust
#[test]
fn parse_watchdog_runbook() {
    let content = include_str!("../../../../runbooks/watchdog.toml");
    let runbook: RawRunbook = toml::from_str(content).unwrap();

    assert!(runbook.meta.is_some());
    assert!(!runbook.watcher.is_empty());
    assert!(!runbook.action.is_empty());
    assert!(!runbook.scanner.is_empty());
}

#[test]
fn parse_janitor_runbook() {
    let content = include_str!("../../../../runbooks/janitor.toml");
    let runbook: RawRunbook = toml::from_str(content).unwrap();

    assert!(!runbook.scanner.is_empty());
    assert!(!runbook.cron.is_empty());
}

#[test]
fn parse_triager_runbook() {
    let content = include_str!("../../../../runbooks/triager.toml");
    let runbook: RawRunbook = toml::from_str(content).unwrap();

    assert!(!runbook.watcher.is_empty());
    assert!(!runbook.action.is_empty());
}

#[test]
fn validate_watcher_action_references() {
    let mut runbook = RawRunbook::default();
    runbook.watcher.insert("test".into(), RawWatcher {
        response: vec![RawWatcherResponse {
            action: "nonexistent".into(),
            ..Default::default()
        }],
        ..Default::default()
    });

    let errors = RunbookValidator::new().validate(&runbook).unwrap_err();
    assert!(errors.iter().any(|e| matches!(e,
        ValidationError::UndefinedReference { kind, name, .. }
        if kind == "action" && name == "nonexistent"
    )));
}
```

## Landing Checklist

- [ ] All system runbooks (watchdog, janitor, triager) parse without errors
- [ ] Loader converts raw types to scheduling configs
- [ ] Validator catches undefined action/watcher/scanner references
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

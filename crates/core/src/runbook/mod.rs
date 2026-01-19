// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Runbook parsing, validation, and loading.
//!
//! A runbook is a TOML file that defines primitives for orchestrating
//! agentic workflows. This module provides:
//!
//! - **types**: Raw data types that mirror TOML structure
//! - **parser**: TOML parsing (syntactic layer)
//! - **validator**: Semantic validation
//! - **template**: Jinja2-style templates
//! - **input**: Shell command output parsing
//! - **loader**: Runtime type conversion and cross-runbook references
//!
//! # Architecture
//!
//! ```text
//! TOML file → parser → RawRunbook → validator → ValidatedRunbook → loader → Runbook
//! ```
//!
//! # Example
//!
//! ```ignore
//! use oj_core::runbook::{parse_runbook, parse_runbook_file};
//!
//! // Parse from string
//! let runbook = parse_runbook(r#"
//!     [command.hello]
//!     run = "echo hello"
//! "#)?;
//!
//! // Parse from file
//! let runbook = parse_runbook_file(Path::new("runbooks/build.toml"))?;
//! ```

pub mod input;
pub mod loader;
pub mod parser;
pub mod template;
pub mod types;
pub mod validator;

// Re-export commonly used items
pub use input::{parse_input, InputError, InputFormat};
pub use loader::{
    load_runbook, load_runbook_file, AttemptDef, Command, DeadLetterConfig, ExhaustedAction,
    FailAction, GuardDef, LoadError, LockDef, PhaseAction, PhaseDef, PhaseNext, PipelineDef,
    Runbook, RunbookRegistry, SemaphoreDef, StrategyDef, TaskDef,
};
pub use parser::{parse_runbook, parse_runbook_file, runbook_name, ParseError};
pub use template::{Context, ContextValue, TemplateEngine, TemplateError};
pub use types::{
    RawAction, RawAttempt, RawCleanupAction, RawCommand, RawCron, RawDeadLetterConfig,
    RawDecisionRule, RawEvents, RawGuard, RawLock, RawMeta, RawPhase, RawPipeline, RawQueue,
    RawRetry, RawRunbook, RawScanner, RawScannerCondition, RawScannerSource, RawSemaphore,
    RawStrategy, RawTask, RawWatcher, RawWatcherCondition, RawWatcherResponse, RawWatcherSource,
    RawWorker,
};
pub use validator::{
    validate_cross_references, validate_runbook, validate_with_registry, CrossRefError,
    ValidatedRunbook, ValidationError, ValidationErrors,
};

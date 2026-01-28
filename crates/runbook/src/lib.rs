// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

// Allow panic!/unwrap/expect in test code
#![cfg_attr(test, allow(clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! Runbook parsing and definition

mod agent;
mod command;
mod parser;
mod pipeline;
mod template;
mod worker;

pub use agent::{ActionConfig, AgentAction, AgentDef, ErrorActionConfig, ErrorMatch, ErrorType};
pub use command::{
    parse_arg_spec, ArgDef, ArgSpec, ArgSpecError, ArgValidationError, CommandDef, FlagDef,
    OptionDef, RunDirective, VariadicDef,
};
pub use parser::{parse_runbook, ParseError, Runbook};
pub use pipeline::{PhaseDef, PipelineDef};
pub use template::interpolate;
pub use worker::WorkerDef;

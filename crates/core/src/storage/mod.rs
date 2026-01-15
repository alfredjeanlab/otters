// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Storage module for JSON-based persistence

pub mod json;

pub use json::{JsonStore, StorageError};

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Template variable interpolation

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Regex pattern for {variable_name} - this is a constant valid pattern
// Allow expect here as the regex is compile-time verified to be valid
#[allow(clippy::expect_used)]
static VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}").expect("constant regex pattern is valid")
});

// Regex pattern for ${VAR:-default} environment variable expansion
#[allow(clippy::expect_used)]
static ENV_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{(\w+):-([^}]*)\}").expect("constant regex pattern is valid"));

/// Interpolate `{name}` placeholders with values from the vars map
///
/// Also expands `${VAR:-default}` patterns from environment variables.
/// Environment variables are expanded first, then template variables.
///
/// Unknown template variables are left as-is.
pub fn interpolate(template: &str, vars: &HashMap<String, String>) -> String {
    // First expand ${VAR:-default} patterns from environment
    let result = ENV_PATTERN
        .replace_all(template, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_value = &caps[2];
            std::env::var(var_name).unwrap_or_else(|_| default_value.to_string())
        })
        .to_string();

    // Then expand {var} patterns from provided vars
    VAR_PATTERN
        .replace_all(&result, |caps: &regex::Captures| {
            let name = &caps[1];
            vars.get(name)
                .cloned()
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;

// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn interpolate_simple() {
    let vars: HashMap<String, String> = [("name".to_string(), "test".to_string())]
        .into_iter()
        .collect();
    assert_eq!(interpolate("Hello {name}!", &vars), "Hello test!");
}

#[test]
fn interpolate_multiple() {
    let vars: HashMap<String, String> = [
        ("a".to_string(), "1".to_string()),
        ("b".to_string(), "2".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(interpolate("{a} + {b} = {a}{b}", &vars), "1 + 2 = 12");
}

#[test]
fn interpolate_unknown_left_alone() {
    let vars: HashMap<String, String> = HashMap::new();
    assert_eq!(interpolate("Hello {unknown}!", &vars), "Hello {unknown}!");
}

#[test]
fn interpolate_no_vars() {
    let vars: HashMap<String, String> = HashMap::new();
    assert_eq!(interpolate("No variables here", &vars), "No variables here");
}

#[test]
fn interpolate_nested_braces_not_matched() {
    let vars: HashMap<String, String> =
        [("x".to_string(), "val".to_string())].into_iter().collect();
    // Only simple {name} is matched, not nested
    assert_eq!(interpolate("{{x}}", &vars), "{val}");
}

#[test]
fn interpolate_env_var_with_default_uses_env() {
    // Set an env var for this test
    std::env::set_var("TEMPLATE_TEST_VAR", "from_env");
    let vars: HashMap<String, String> = HashMap::new();
    assert_eq!(
        interpolate("${TEMPLATE_TEST_VAR:-default}", &vars),
        "from_env"
    );
    std::env::remove_var("TEMPLATE_TEST_VAR");
}

#[test]
fn interpolate_env_var_with_default_uses_default() {
    // Ensure env var is not set
    std::env::remove_var("TEMPLATE_UNSET_VAR");
    let vars: HashMap<String, String> = HashMap::new();
    assert_eq!(
        interpolate("${TEMPLATE_UNSET_VAR:-fallback}", &vars),
        "fallback"
    );
}

#[test]
fn interpolate_env_and_template_vars() {
    std::env::set_var("TEMPLATE_CMD_VAR", "custom_cmd");
    let vars: HashMap<String, String> = [("name".to_string(), "test".to_string())]
        .into_iter()
        .collect();
    assert_eq!(
        interpolate("${TEMPLATE_CMD_VAR:-default} --name {name}", &vars),
        "custom_cmd --name test"
    );
    std::env::remove_var("TEMPLATE_CMD_VAR");
}

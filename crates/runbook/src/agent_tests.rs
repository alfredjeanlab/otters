// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn agent_build_command() {
    let agent = AgentDef {
        name: "planner".to_string(),
        run: "claude -p \"{prompt}\"".to_string(),
        prompt: None,
        prompt_file: None,
        env: HashMap::new(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars: HashMap<String, String> = [("prompt".to_string(), "Add login".to_string())]
        .into_iter()
        .collect();

    assert_eq!(agent.build_command(&vars), "claude -p \"Add login\"");
}

#[test]
fn agent_build_env() {
    let agent = AgentDef {
        name: "executor".to_string(),
        run: "claude".to_string(),
        prompt: Some("Execute the plan".to_string()),
        prompt_file: None,
        env: [
            ("OJ_PIPELINE".to_string(), "{pipeline_id}".to_string()),
            ("OJ_NAME".to_string(), "{name}".to_string()),
        ]
        .into_iter()
        .collect(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars: HashMap<String, String> = [
        ("pipeline_id".to_string(), "pipe-1".to_string()),
        ("name".to_string(), "feature".to_string()),
    ]
    .into_iter()
    .collect();

    let env = agent.build_env(&vars);
    assert!(env.contains(&("OJ_PIPELINE".to_string(), "pipe-1".to_string())));
    assert!(env.contains(&("OJ_NAME".to_string(), "feature".to_string())));
}

#[test]
fn agent_get_prompt_from_field() {
    let agent = AgentDef {
        name: "worker".to_string(),
        run: "claude".to_string(),
        prompt: Some("Do {task} for {name}".to_string()),
        prompt_file: None,
        env: HashMap::new(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars: HashMap<String, String> = [
        ("task".to_string(), "coding".to_string()),
        ("name".to_string(), "feature-1".to_string()),
    ]
    .into_iter()
    .collect();

    let prompt = agent.get_prompt(&vars).unwrap();
    assert_eq!(prompt, "Do coding for feature-1");
}

#[test]
fn agent_get_prompt_empty_when_unset() {
    let agent = AgentDef {
        name: "worker".to_string(),
        run: "claude".to_string(),
        prompt: None,
        prompt_file: None,
        env: HashMap::new(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars = HashMap::new();
    let prompt = agent.get_prompt(&vars).unwrap();
    assert_eq!(prompt, "");
}

#[test]
fn agent_get_prompt_from_file() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "Do {{task}} for {{name}}").unwrap();

    let agent = AgentDef {
        name: "worker".to_string(),
        run: "claude".to_string(),
        prompt: None,
        prompt_file: Some(file.path().to_path_buf()),
        env: HashMap::new(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars: HashMap<String, String> = [
        ("task".to_string(), "coding".to_string()),
        ("name".to_string(), "feature-1".to_string()),
    ]
    .into_iter()
    .collect();

    let prompt = agent.get_prompt(&vars).unwrap();
    assert!(prompt.contains("Do coding for feature-1"));
}

#[test]
fn agent_get_prompt_file_not_found() {
    let agent = AgentDef {
        name: "worker".to_string(),
        run: "claude".to_string(),
        prompt: None,
        prompt_file: Some(PathBuf::from("/nonexistent/path/to/prompt.md")),
        env: HashMap::new(),
        cwd: None,
        on_idle: ActionConfig::default(),
        on_exit: default_on_exit(),
        on_error: default_on_error(),
    };

    let vars = HashMap::new();
    assert!(agent.get_prompt(&vars).is_err());
}

// =============================================================================
// Action Configuration Tests
// =============================================================================

#[test]
fn parses_simple_action() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default)]
        on_idle: ActionConfig,
        #[serde(default = "default_on_exit")]
        on_exit: ActionConfig,
    }

    let toml = r#"
        on_idle = "nudge"
        on_exit = "escalate"
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.on_idle.action(), &AgentAction::Nudge);
    assert_eq!(config.on_exit.action(), &AgentAction::Escalate);
}

#[test]
fn parses_action_with_message() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default)]
        on_idle: ActionConfig,
    }

    let toml = r#"
        on_idle = { action = "nudge", message = "Keep going" }
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.on_idle.action(), &AgentAction::Nudge);
    assert_eq!(config.on_idle.message(), Some("Keep going"));
    assert!(!config.on_idle.append());
}

#[test]
fn parses_action_with_append() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default = "default_on_exit")]
        on_exit: ActionConfig,
    }

    let toml = r#"
        on_exit = { action = "recover", message = "Previous attempt exited.", append = true }
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.on_exit.action(), &AgentAction::Recover);
    assert_eq!(config.on_exit.message(), Some("Previous attempt exited."));
    assert!(config.on_exit.append());
}

#[test]
fn parses_per_error_actions() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default = "default_on_error")]
        on_error: ErrorActionConfig,
    }

    let toml = r#"
        [[on_error]]
        match = "no_internet"
        action = "recover"
        message = "Network restored"

        [[on_error]]
        action = "escalate"
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();

    // Match specific error type
    let action = config.on_error.action_for(Some(&ErrorType::NoInternet));
    assert_eq!(action.action(), &AgentAction::Recover);
    assert_eq!(action.message(), Some("Network restored"));

    // Fall through to catch-all
    let action = config.on_error.action_for(Some(&ErrorType::Unauthorized));
    assert_eq!(action.action(), &AgentAction::Escalate);
}

#[test]
fn error_action_config_simple() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default = "default_on_error")]
        on_error: ErrorActionConfig,
    }

    let toml = r#"
        on_error = "escalate"
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();

    let action = config.on_error.action_for(Some(&ErrorType::NoInternet));
    assert_eq!(action.action(), &AgentAction::Escalate);
}

#[test]
fn error_action_config_default_when_no_match() {
    #[derive(Debug, Deserialize)]
    struct TestConfig {
        #[serde(default = "default_on_error")]
        on_error: ErrorActionConfig,
    }

    // Only matches rate_limited, no catch-all
    let toml = r#"
        [[on_error]]
        match = "rate_limited"
        action = "recover"
    "#;
    let config: TestConfig = toml::from_str(toml).unwrap();

    // Should default to escalate when no match
    let action = config.on_error.action_for(Some(&ErrorType::NoInternet));
    assert_eq!(action.action(), &AgentAction::Escalate);
}

#[test]
fn action_config_defaults() {
    // Defaults: on_idle = "nudge", on_exit = "escalate", on_error = "escalate"
    let default_idle = ActionConfig::default();
    assert_eq!(default_idle.action(), &AgentAction::Nudge);

    let default_exit = default_on_exit();
    assert_eq!(default_exit.action(), &AgentAction::Escalate);

    let default_error = default_on_error();
    let action = default_error.action_for(Some(&ErrorType::Unauthorized));
    assert_eq!(action.action(), &AgentAction::Escalate);
}

#[test]
fn parses_full_agent_with_actions() {
    let toml = r#"
        name = "worker"
        run = "claude"
        prompt = "Do the task."
        on_idle = { action = "nudge", message = "Keep going" }
        on_exit = "escalate"
        on_error = "escalate"
    "#;
    let agent: AgentDef = toml::from_str(toml).unwrap();
    assert_eq!(agent.on_idle.action(), &AgentAction::Nudge);
    assert_eq!(agent.on_idle.message(), Some("Keep going"));
    assert_eq!(agent.on_exit.action(), &AgentAction::Escalate);
}

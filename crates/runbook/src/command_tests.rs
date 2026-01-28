// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

// Argument validation tests
#[test]
fn validate_required_positional_missing() {
    let cmd = CommandDef {
        name: "build".to_string(),
        args: parse_arg_spec("<name> <prompt>").unwrap(),
        defaults: HashMap::new(),
        run: RunDirective::Shell("echo".to_string()),
    };

    // Missing both required args
    let result = cmd.validate_args(&[], &HashMap::new());
    assert!(matches!(
        result,
        Err(ArgValidationError::MissingPositional(name)) if name == "name"
    ));

    // Missing second required arg
    let result = cmd.validate_args(&["foo".to_string()], &HashMap::new());
    assert!(matches!(
        result,
        Err(ArgValidationError::MissingPositional(name)) if name == "prompt"
    ));

    // All required args provided
    let result = cmd.validate_args(&["foo".to_string(), "bar".to_string()], &HashMap::new());
    assert!(result.is_ok());
}

#[test]
fn validate_required_positional_with_default() {
    let cmd = CommandDef {
        name: "build".to_string(),
        args: parse_arg_spec("<name>").unwrap(),
        defaults: [("name".to_string(), "default-name".to_string())]
            .into_iter()
            .collect(),
        run: RunDirective::Shell("echo".to_string()),
    };

    // Default satisfies requirement
    let result = cmd.validate_args(&[], &HashMap::new());
    assert!(result.is_ok());
}

#[test]
fn validate_required_option_missing() {
    let cmd = CommandDef {
        name: "deploy".to_string(),
        args: parse_arg_spec("--env <environment>").unwrap(),
        defaults: HashMap::new(),
        run: RunDirective::Shell("deploy.sh".to_string()),
    };

    // Missing required option
    let result = cmd.validate_args(&[], &HashMap::new());
    assert!(matches!(
        result,
        Err(ArgValidationError::MissingOption(name)) if name == "env"
    ));

    // Required option provided via named args
    let result = cmd.validate_args(
        &[],
        &[("env".to_string(), "prod".to_string())]
            .into_iter()
            .collect(),
    );
    assert!(result.is_ok());
}

#[test]
fn validate_required_variadic_missing() {
    let cmd = CommandDef {
        name: "copy".to_string(),
        args: parse_arg_spec("<files...>").unwrap(),
        defaults: HashMap::new(),
        run: RunDirective::Shell("cp".to_string()),
    };

    // Missing required variadic
    let result = cmd.validate_args(&[], &HashMap::new());
    assert!(matches!(
        result,
        Err(ArgValidationError::MissingVariadic(name)) if name == "files"
    ));

    // Required variadic provided
    let result = cmd.validate_args(&["file1".to_string()], &HashMap::new());
    assert!(result.is_ok());
}

#[test]
fn validate_optional_args_not_required() {
    let cmd = CommandDef {
        name: "test".to_string(),
        args: parse_arg_spec("[name] [-v/--verbose] [files...]").unwrap(),
        defaults: HashMap::new(),
        run: RunDirective::Shell("test.sh".to_string()),
    };

    // All optional - should pass with no args
    let result = cmd.validate_args(&[], &HashMap::new());
    assert!(result.is_ok());
}

// ArgSpec parsing tests
#[test]
fn parse_simple_positional() {
    let spec = parse_arg_spec("<name> <prompt>").unwrap();
    assert_eq!(spec.positional.len(), 2);
    assert!(spec.positional[0].required);
    assert_eq!(spec.positional[0].name, "name");
    assert!(spec.positional[1].required);
    assert_eq!(spec.positional[1].name, "prompt");
}

#[test]
fn parse_optional_positional() {
    let spec = parse_arg_spec("<name> [description]").unwrap();
    assert!(spec.positional[0].required);
    assert!(!spec.positional[1].required);
}

#[test]
fn parse_flags_and_options() {
    let spec = parse_arg_spec("<env> [-t/--tag <version>] [-f/--force]").unwrap();
    assert_eq!(spec.positional.len(), 1);
    assert_eq!(spec.options.len(), 1);
    assert_eq!(spec.options[0].name, "tag");
    assert_eq!(spec.options[0].short, Some('t'));
    assert!(!spec.options[0].required);
    assert_eq!(spec.flags.len(), 1);
    assert_eq!(spec.flags[0].name, "force");
    assert_eq!(spec.flags[0].short, Some('f'));
}

#[test]
fn parse_variadic() {
    let spec = parse_arg_spec("<cmd> [args...]").unwrap();
    assert!(spec.variadic.is_some());
    assert!(!spec.variadic.as_ref().unwrap().required);
    assert_eq!(spec.variadic.as_ref().unwrap().name, "args");
}

#[test]
fn parse_required_variadic() {
    let spec = parse_arg_spec("<cmd> <files...>").unwrap();
    assert!(spec.variadic.is_some());
    assert!(spec.variadic.as_ref().unwrap().required);
    assert_eq!(spec.variadic.as_ref().unwrap().name, "files");
}

#[test]
fn parse_empty_spec() {
    let spec = parse_arg_spec("").unwrap();
    assert!(spec.positional.is_empty());
    assert!(spec.flags.is_empty());
    assert!(spec.options.is_empty());
    assert!(spec.variadic.is_none());
}

#[test]
fn parse_required_flag() {
    let spec = parse_arg_spec("--verbose").unwrap();
    assert_eq!(spec.flags.len(), 1);
    assert_eq!(spec.flags[0].name, "verbose");
}

#[test]
fn parse_required_option() {
    let spec = parse_arg_spec("--config <file>").unwrap();
    assert_eq!(spec.options.len(), 1);
    assert_eq!(spec.options[0].name, "config");
    assert!(spec.options[0].required);
}

#[test]
fn parse_complex_spec() {
    let spec = parse_arg_spec("<env> [-t/--tag <version>] [-f/--force] [targets...]").unwrap();
    assert_eq!(spec.positional.len(), 1);
    assert_eq!(spec.positional[0].name, "env");
    assert_eq!(spec.options.len(), 1);
    assert_eq!(spec.flags.len(), 1);
    assert!(spec.variadic.is_some());
}

#[test]
fn parse_error_variadic_not_last() {
    let result = parse_arg_spec("<files...> <other>");
    assert!(result.is_err());
}

#[test]
fn parse_error_optional_before_required() {
    let result = parse_arg_spec("[optional] <required>");
    assert!(result.is_err());
}

#[test]
fn parse_error_duplicate_name() {
    let result = parse_arg_spec("<name> <name>");
    assert!(result.is_err());
}

// RunDirective tests
#[test]
fn run_directive_shell() {
    let directive = RunDirective::Shell("echo hello".to_string());
    assert!(directive.is_shell());
    assert!(!directive.is_pipeline());
    assert_eq!(directive.shell_command(), Some("echo hello"));
}

#[test]
fn run_directive_pipeline() {
    let directive = RunDirective::Pipeline {
        pipeline: "build".to_string(),
    };
    assert!(directive.is_pipeline());
    assert!(!directive.is_shell());
    assert_eq!(directive.pipeline_name(), Some("build"));
}

#[test]
fn run_directive_agent() {
    let directive = RunDirective::Agent {
        agent: "planning".to_string(),
    };
    assert!(directive.is_agent());
    assert_eq!(directive.agent_name(), Some("planning"));
}

#[test]
fn run_directive_strategy() {
    let directive = RunDirective::Strategy {
        strategy: "merge".to_string(),
    };
    assert!(directive.is_strategy());
    assert_eq!(directive.strategy_name(), Some("merge"));
}

// TOML deserialization tests
#[test]
fn deserialize_shell_run() {
    #[derive(Deserialize)]
    struct Test {
        run: RunDirective,
    }
    let toml = r#"run = "echo hello""#;
    let test: Test = toml::from_str(toml).unwrap();
    assert!(test.run.is_shell());
    assert_eq!(test.run.shell_command(), Some("echo hello"));
}

#[test]
fn deserialize_pipeline_run() {
    #[derive(Deserialize)]
    struct Test {
        run: RunDirective,
    }
    let toml = r#"run = { pipeline = "build" }"#;
    let test: Test = toml::from_str(toml).unwrap();
    assert_eq!(test.run.pipeline_name(), Some("build"));
}

#[test]
fn deserialize_agent_run() {
    #[derive(Deserialize)]
    struct Test {
        run: RunDirective,
    }
    let toml = r#"run = { agent = "planning" }"#;
    let test: Test = toml::from_str(toml).unwrap();
    assert_eq!(test.run.agent_name(), Some("planning"));
}

#[test]
fn deserialize_arg_spec_string() {
    #[derive(Deserialize)]
    struct Test {
        args: ArgSpec,
    }
    let toml = r#"args = "<name> <prompt>""#;
    let test: Test = toml::from_str(toml).unwrap();
    assert_eq!(test.args.positional.len(), 2);
    assert_eq!(test.args.positional[0].name, "name");
}

#[test]
fn deserialize_arg_spec_struct() {
    #[derive(Deserialize)]
    struct Test {
        args: ArgSpec,
    }
    let toml = r#"
[args]
positional = ["name", "prompt"]
"#;
    let test: Test = toml::from_str(toml).unwrap();
    assert_eq!(test.args.positional.len(), 2);
    assert_eq!(test.args.positional[0].name, "name");
}

// CommandDef tests
#[test]
fn command_parse_args() {
    let cmd = CommandDef {
        name: "build".to_string(),
        args: ArgSpec {
            positional: vec![
                ArgDef {
                    name: "name".to_string(),
                    required: true,
                },
                ArgDef {
                    name: "prompt".to_string(),
                    required: true,
                },
            ],
            flags: Vec::new(),
            options: Vec::new(),
            variadic: None,
        },
        defaults: [("branch".to_string(), "main".to_string())]
            .into_iter()
            .collect(),
        run: RunDirective::Pipeline {
            pipeline: "build".to_string(),
        },
    };

    let result = cmd.parse_args(
        &["feature".to_string(), "Add login".to_string()],
        &HashMap::new(),
    );

    assert_eq!(result.get("name"), Some(&"feature".to_string()));
    assert_eq!(result.get("prompt"), Some(&"Add login".to_string()));
    assert_eq!(result.get("branch"), Some(&"main".to_string()));
}

#[test]
fn command_named_overrides() {
    let cmd = CommandDef {
        name: "build".to_string(),
        args: ArgSpec {
            positional: vec![ArgDef {
                name: "name".to_string(),
                required: true,
            }],
            flags: Vec::new(),
            options: vec![OptionDef {
                name: "branch".to_string(),
                short: None,
                required: false,
            }],
            variadic: None,
        },
        defaults: [("branch".to_string(), "main".to_string())]
            .into_iter()
            .collect(),
        run: RunDirective::Pipeline {
            pipeline: "build".to_string(),
        },
    };

    let result = cmd.parse_args(
        &["feature".to_string()],
        &[("branch".to_string(), "develop".to_string())]
            .into_iter()
            .collect(),
    );

    assert_eq!(result.get("branch"), Some(&"develop".to_string()));
}

#[test]
fn command_variadic_args() {
    let cmd = CommandDef {
        name: "deploy".to_string(),
        args: ArgSpec {
            positional: vec![ArgDef {
                name: "env".to_string(),
                required: true,
            }],
            flags: Vec::new(),
            options: Vec::new(),
            variadic: Some(VariadicDef {
                name: "targets".to_string(),
                required: false,
            }),
        },
        defaults: HashMap::new(),
        run: RunDirective::Shell("deploy.sh".to_string()),
    };

    let result = cmd.parse_args(
        &["prod".to_string(), "api".to_string(), "worker".to_string()],
        &HashMap::new(),
    );

    assert_eq!(result.get("env"), Some(&"prod".to_string()));
    assert_eq!(result.get("targets"), Some(&"api worker".to_string()));
}

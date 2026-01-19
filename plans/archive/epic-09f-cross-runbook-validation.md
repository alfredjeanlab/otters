# Epic 9f: Cross-Runbook Validation

**Root Feature:** `otters-9936`
**Depends on**: Epic 9a (Runbook Types)
**Blocks**: None

## Problem Statement

Cross-runbook references like `common.task.planning` parse correctly but aren't validated semantically. If the referenced runbook doesn't exist or doesn't contain the referenced item, the error only surfaces at runtime when resolution fails.

## Goal

Validate cross-runbook references at load time so invalid references fail early with clear error messages.

## Implementation

### 1. Enhance Validator in `crates/core/src/runbook/validator.rs`

```rust
impl RunbookValidator {
    pub fn validate_with_registry(&self, runbook: &RawRunbook, registry: &RunbookRegistry) -> Result<ValidatedRunbook, Vec<ValidationError>> {
        // Run internal validation, then validate_cross_references, collect errors
    }

    fn validate_cross_references(&self, runbook: &RawRunbook, registry: &RunbookRegistry) -> Vec<ValidationError> {
        // Check cross-refs in:
        // - pipeline.phase.task, strategy, pre[], post[]
        // - strategy.attempts[].task
        // - worker.handler (as pipeline or task)
        // Return UndefinedReference errors for invalid refs
    }

    fn is_cross_ref(&self, reference: &str) -> bool {
        // True if format is "runbook.kind.name" where kind is task/guard/strategy/pipeline
    }

    fn validate_cross_ref(&self, reference: &str, expected_kind: &str, registry: &RunbookRegistry) -> Result<(), CrossRefError> {
        // Skip if not cross-ref
        // Parse "runbook.kind.name", check kind matches expected
        // Look up runbook in registry, check item exists
    }
}

/// Errors from cross-reference validation
#[derive(Debug, thiserror::Error)]
pub enum CrossRefError {
    #[error("invalid cross-reference format: {reference}")]
    InvalidFormat { reference: String },

    #[error("kind mismatch: expected {expected}, got {actual}")]
    KindMismatch { expected: String, actual: String },

    #[error("runbook not found: {name}")]
    RunbookNotFound { name: String },

    #[error("{kind} '{name}' not found in runbook '{runbook}'")]
    ItemNotFound {
        kind: String,
        name: String,
        runbook: String,
    },

    #[error("unknown reference kind: {kind}")]
    UnknownKind { kind: String },
}
```

### 2. Update RunbookRegistry in `crates/core/src/runbook/loader.rs`

```rust
impl RunbookRegistry {
    pub fn load_directory_validated(&mut self, path: impl AsRef<Path>) -> Result<(), Vec<(String, Vec<ValidationError>)>> {
        // First pass: load all *.toml files into registry
        // Second pass: validate_cross_references for each runbook against registry
        // Return collected errors by runbook name
    }
}
```

### 3. Add Error Messages

```rust
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // UndefinedReference: distinguish cross-ref (contains '.') from local ref
    }
}
```

## Files

- `crates/core/src/runbook/validator.rs` - Cross-reference validation
- `crates/core/src/runbook/loader.rs` - Registry validation method
- `crates/core/src/runbook/validator_tests.rs` - Tests

## Tests

```rust
#[test]
fn valid_cross_reference_passes() {
    let mut registry = RunbookRegistry::new();

    // Add a "common" runbook with a task
    let common_toml = r#"
        [task.shared-task]
        prompt = "Do something"
    "#;
    let common: RawRunbook = toml::from_str(common_toml).unwrap();
    registry.add_raw("common", common.clone());
    registry.add("common", Runbook::from_raw(common).unwrap());

    // Add a runbook that references common.task.shared-task
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.task.shared-task"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    assert!(result.is_ok());
}

#[test]
fn missing_runbook_fails() {
    let registry = RunbookRegistry::new();

    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "nonexistent.task.foo"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(e,
        ValidationError::UndefinedReference { kind, name, .. }
        if kind == "task" && name == "nonexistent.task.foo"
    )));
}

#[test]
fn missing_item_in_runbook_fails() {
    let mut registry = RunbookRegistry::new();

    // Add a "common" runbook without the referenced task
    let common_toml = r#"
        [task.other-task]
        prompt = "Different task"
    "#;
    let common: RawRunbook = toml::from_str(common_toml).unwrap();
    registry.add_raw("common", common.clone());
    registry.add("common", Runbook::from_raw(common).unwrap());

    // Reference a task that doesn't exist
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.task.missing-task"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(e,
        ValidationError::UndefinedReference { name, .. }
        if name == "common.task.missing-task"
    )));
}

#[test]
fn kind_mismatch_detected() {
    let mut registry = RunbookRegistry::new();

    // Add common with a guard (not a task)
    let common_toml = r#"
        [guard.check-exists]
        run = "test -f file.txt"
    "#;
    let common: RawRunbook = toml::from_str(common_toml).unwrap();
    registry.add_raw("common", common.clone());
    registry.add("common", Runbook::from_raw(common).unwrap());

    // Try to use it as a task
    let main_toml = r#"
        [[pipeline.main.phase]]
        name = "do-work"
        task = "common.guard.check-exists"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    // Should fail because we're referencing a guard as a task
    assert!(result.is_err());
}

#[test]
fn local_references_not_affected() {
    let registry = RunbookRegistry::new();

    // Local reference (no dots indicating cross-ref)
    let main_toml = r#"
        [task.local-task]
        prompt = "Local task"

        [[pipeline.main.phase]]
        name = "do-work"
        task = "local-task"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    // Should pass (local references validated separately)
    assert!(result.is_ok());
}

#[test]
fn validate_strategy_cross_refs() {
    let mut registry = RunbookRegistry::new();

    let common_toml = r#"
        [task.fallback]
        prompt = "Fallback approach"
    "#;
    let common: RawRunbook = toml::from_str(common_toml).unwrap();
    registry.add_raw("common", common.clone());
    registry.add("common", Runbook::from_raw(common).unwrap());

    let main_toml = r#"
        [strategy.retry]
        checkpoint = "save_state"

        [[strategy.retry.attempts]]
        task = "common.task.fallback"
    "#;
    let main: RawRunbook = toml::from_str(main_toml).unwrap();

    let validator = RunbookValidator::new();
    let result = validator.validate_with_registry(&main, &registry);

    assert!(result.is_ok());
}

#[test]
fn directory_validation_catches_all_errors() {
    let dir = tempdir().unwrap();

    // Write common runbook
    std::fs::write(
        dir.path().join("common.toml"),
        r#"
            [task.shared]
            prompt = "Shared task"
        "#,
    ).unwrap();

    // Write main runbook with invalid reference
    std::fs::write(
        dir.path().join("main.toml"),
        r#"
            [[pipeline.test.phase]]
            name = "work"
            task = "common.task.nonexistent"
        "#,
    ).unwrap();

    let mut registry = RunbookRegistry::new();
    let result = registry.load_directory_validated(dir.path());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|(name, _)| name == "main"));
}
```

## Landing Checklist

- [ ] Cross-runbook references are validated at load time
- [ ] Missing runbooks produce clear error messages
- [ ] Missing items in runbooks produce clear error messages
- [ ] Kind mismatches are detected (e.g., using guard as task)
- [ ] Local references are not affected by cross-ref validation
- [ ] Directory validation validates all runbooks together
- [ ] All tests pass: `make check`
- [ ] Linting passes: `./checks/lint.sh`

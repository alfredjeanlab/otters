// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::collections::HashMap;

// ============================================================================
// Basic variable interpolation
// ============================================================================

#[test]
fn render_simple_string_variable() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "auth");

    let result = engine.render("Hello {{ name }}", &context).unwrap();
    assert_eq!(result, "Hello auth");
}

#[test]
fn render_multiple_variables() {
    let engine = TemplateEngine::new();
    let context = Context::new()
        .with_string("name", "auth")
        .with_string("action", "build");

    let result = engine
        .render("{{ action }} feature: {{ name }}", &context)
        .unwrap();
    assert_eq!(result, "build feature: auth");
}

#[test]
fn render_number_variable() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_number("count", 42);

    let result = engine.render("Count: {{ count }}", &context).unwrap();
    assert_eq!(result, "Count: 42");
}

#[test]
fn render_bool_variable() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_bool("enabled", true);

    let result = engine.render("Enabled: {{ enabled }}", &context).unwrap();
    assert_eq!(result, "Enabled: true");
}

// ============================================================================
// Nested object access
// ============================================================================

#[test]
fn render_nested_object() {
    let engine = TemplateEngine::new();

    let mut bug = HashMap::new();
    bug.insert("id".to_string(), ContextValue::String("123".to_string()));
    bug.insert(
        "title".to_string(),
        ContextValue::String("Fix login".to_string()),
    );

    let context = Context::new().with_object("bug", bug);

    let result = engine
        .render("Bug #{{ bug.id }}: {{ bug.title }}", &context)
        .unwrap();
    assert_eq!(result, "Bug #123: Fix login");
}

#[test]
fn render_deeply_nested_object() {
    let engine = TemplateEngine::new();

    let mut inner = HashMap::new();
    inner.insert(
        "value".to_string(),
        ContextValue::String("deep".to_string()),
    );

    let mut outer = HashMap::new();
    outer.insert("inner".to_string(), ContextValue::Object(inner));

    let context = Context::new().with_object("outer", outer);

    let result = engine
        .render("Value: {{ outer.inner.value }}", &context)
        .unwrap();
    assert_eq!(result, "Value: deep");
}

// ============================================================================
// Simple brace syntax (backwards compatibility)
// ============================================================================

#[test]
fn render_simple_brace_variable() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "auth");

    let result = engine.render_simple("Hello {name}", &context).unwrap();
    assert_eq!(result, "Hello auth");
}

#[test]
fn render_simple_brace_nested() {
    let engine = TemplateEngine::new();

    let mut bug = HashMap::new();
    bug.insert("id".to_string(), ContextValue::String("456".to_string()));

    let context = Context::new().with_object("bug", bug);

    let result = engine.render_simple("Bug #{bug.id}", &context).unwrap();
    assert_eq!(result, "Bug #456");
}

#[test]
fn render_simple_brace_multiple() {
    let engine = TemplateEngine::new();
    let context = Context::new()
        .with_string("workspace", ".worktrees/auth")
        .with_string("branch", "feature/auth");

    let result = engine
        .render_simple("git worktree add {workspace} -b {branch}", &context)
        .unwrap();
    assert_eq!(result, "git worktree add .worktrees/auth -b feature/auth");
}

#[test]
fn render_preserves_double_braces() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "test");

    // Double braces should be preserved
    let result = engine.render_simple("Hello {{ name }}", &context).unwrap();
    assert_eq!(result, "Hello test");
}

// ============================================================================
// Conditionals
// ============================================================================

#[test]
fn render_if_true() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_bool("enabled", true);

    let result = engine
        .render("{% if enabled %}YES{% endif %}", &context)
        .unwrap();
    assert_eq!(result, "YES");
}

#[test]
fn render_if_false() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_bool("enabled", false);

    let result = engine
        .render("{% if enabled %}YES{% endif %}", &context)
        .unwrap();
    assert_eq!(result, "");
}

#[test]
fn render_if_else() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_bool("enabled", false);

    let result = engine
        .render("{% if enabled %}YES{% else %}NO{% endif %}", &context)
        .unwrap();
    assert_eq!(result, "NO");
}

#[test]
fn render_if_with_variable_truthy() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "auth");

    let result = engine
        .render("{% if name %}Has name: {{ name }}{% endif %}", &context)
        .unwrap();
    assert_eq!(result, "Has name: auth");
}

// ============================================================================
// Loops
// ============================================================================

#[test]
fn render_for_loop() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_list(
        "items",
        vec![
            ContextValue::String("a".to_string()),
            ContextValue::String("b".to_string()),
            ContextValue::String("c".to_string()),
        ],
    );

    let result = engine
        .render("{% for item in items %}{{ item }},{% endfor %}", &context)
        .unwrap();
    assert_eq!(result, "a,b,c,");
}

#[test]
fn render_for_loop_with_objects() {
    let engine = TemplateEngine::new();

    let items = vec![
        ContextValue::Object({
            let mut m = HashMap::new();
            m.insert(
                "name".to_string(),
                ContextValue::String("Alice".to_string()),
            );
            m
        }),
        ContextValue::Object({
            let mut m = HashMap::new();
            m.insert("name".to_string(), ContextValue::String("Bob".to_string()));
            m
        }),
    ];

    let context = Context::new().with_list("users", items);

    let result = engine
        .render(
            "{% for user in users %}{{ user.name }} {% endfor %}",
            &context,
        )
        .unwrap();
    assert_eq!(result, "Alice Bob ");
}

// ============================================================================
// Filters
// ============================================================================

#[test]
fn render_filter_upper() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "auth");

    let result = engine.render("{{ name | upper }}", &context).unwrap();
    assert_eq!(result, "AUTH");
}

#[test]
fn render_filter_lower() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "AUTH");

    let result = engine.render("{{ name | lower }}", &context).unwrap();
    assert_eq!(result, "auth");
}

#[test]
fn render_filter_default() {
    let engine = TemplateEngine::new();
    let context = Context::new(); // No 'name' defined

    let result = engine
        .render("{{ name | default('unknown') }}", &context)
        .unwrap();
    assert_eq!(result, "unknown");
}

#[test]
fn render_filter_default_with_value() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_string("name", "auth");

    let result = engine
        .render("{{ name | default('unknown') }}", &context)
        .unwrap();
    assert_eq!(result, "auth");
}

#[test]
fn render_filter_join() {
    let engine = TemplateEngine::new();
    let context = Context::new().with_list(
        "items",
        vec![
            ContextValue::String("a".to_string()),
            ContextValue::String("b".to_string()),
            ContextValue::String("c".to_string()),
        ],
    );

    let result = engine.render("{{ items | join(', ') }}", &context).unwrap();
    assert_eq!(result, "a, b, c");
}

// ============================================================================
// Context building
// ============================================================================

#[test]
fn context_from_strings() {
    let mut map = HashMap::new();
    map.insert("name".to_string(), "auth".to_string());
    map.insert("action".to_string(), "build".to_string());

    let context = Context::from_strings(map);
    let engine = TemplateEngine::new();

    let result = engine.render("{{ action }} {{ name }}", &context).unwrap();
    assert_eq!(result, "build auth");
}

#[test]
fn context_set_value() {
    let mut context = Context::new();
    context.set_string("name", "auth");
    context.set_value("count", ContextValue::Number(42));

    let engine = TemplateEngine::new();
    let result = engine.render("{{ name }}: {{ count }}", &context).unwrap();
    assert_eq!(result, "auth: 42");
}

// ============================================================================
// Error handling
// ============================================================================

#[test]
fn render_undefined_variable_with_default() {
    let engine = TemplateEngine::new();
    let context = Context::new();

    // Using default filter should not error
    let result = engine
        .render("{{ undefined | default('fallback') }}", &context)
        .unwrap();
    assert_eq!(result, "fallback");
}

#[test]
fn render_syntax_error() {
    let engine = TemplateEngine::new();
    let context = Context::new();

    let result = engine.render("{% if %}", &context);
    assert!(result.is_err());
}

// ============================================================================
// Complex templates (runbook-like)
// ============================================================================

#[test]
fn render_runbook_command() {
    let engine = TemplateEngine::new();
    let context = Context::new()
        .with_string("name", "auth")
        .with_string("workspace", ".worktrees/build-auth")
        .with_string("branch", "build-auth");

    let template = r#"git worktree add {{ workspace }} -b {{ branch }}
wk new feature "Build {{ name }}"
"#;

    let result = engine.render(template, &context).unwrap();
    assert!(result.contains("git worktree add .worktrees/build-auth -b build-auth"));
    assert!(result.contains("Build auth"));
}

#[test]
fn render_runbook_command_simple_braces() {
    let engine = TemplateEngine::new();
    let context = Context::new()
        .with_string("name", "auth")
        .with_string("workspace", ".worktrees/build-auth")
        .with_string("branch", "build-auth");

    let template = r#"git worktree add {workspace} -b {branch}
wk new feature "Build {name}""#;

    let result = engine.render_simple(template, &context).unwrap();
    assert!(result.contains("git worktree add .worktrees/build-auth -b build-auth"));
    assert!(result.contains("Build auth"));
}

// ============================================================================
// Simple brace conversion
// ============================================================================

#[test]
fn convert_simple_braces_basic() {
    let input = "{name}";
    let output = convert_simple_braces(input);
    assert_eq!(output, "{{ name }}");
}

#[test]
fn convert_simple_braces_nested() {
    let input = "{bug.id}";
    let output = convert_simple_braces(input);
    assert_eq!(output, "{{ bug.id }}");
}

#[test]
fn convert_simple_braces_preserves_double() {
    let input = "{{ name }}";
    let output = convert_simple_braces(input);
    assert_eq!(output, "{{ name }}");
}

#[test]
fn convert_simple_braces_preserves_jinja_block() {
    let input = "{% if true %}yes{% endif %}";
    let output = convert_simple_braces(input);
    assert_eq!(output, "{% if true %}yes{% endif %}");
}

#[test]
fn convert_simple_braces_with_filter() {
    let input = "{name | upper}";
    let output = convert_simple_braces(input);
    assert_eq!(output, "{{ name | upper }}");
}

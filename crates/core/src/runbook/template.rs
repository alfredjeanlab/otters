// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Jinja2-style template engine for runbooks.
//!
//! This module provides template rendering for runbook strings.
//! Templates use a Jinja2-like syntax:
//!
//! - Variable interpolation: `{{ name }}`, `{{ bug.id }}`
//! - Conditionals: `{% if condition %}...{% endif %}`
//! - Loops: `{% for item in items %}...{% endfor %}`
//! - Filters: `{{ name | upper }}`, `{{ list | join(", ") }}`
//! - Default values: `{{ name | default("unknown") }}`

use minijinja::{Environment, Value};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during template rendering.
#[derive(Debug, Error)]
pub enum TemplateError {
    /// Template syntax error
    #[error("Template syntax error: {0}")]
    Syntax(String),

    /// Undefined variable
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    /// Render error
    #[error("Render error: {0}")]
    Render(String),
}

impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("undefined") {
            TemplateError::UndefinedVariable(msg)
        } else if msg.contains("syntax") {
            TemplateError::Syntax(msg)
        } else {
            TemplateError::Render(msg)
        }
    }
}

/// Template engine for rendering runbook strings.
#[derive(Debug, Clone)]
pub struct TemplateEngine {
    /// Shared environment configuration
    _strict: bool,
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine {
    /// Create a new template engine.
    pub fn new() -> Self {
        Self { _strict: true }
    }

    /// Create a template engine with lenient undefined variable handling.
    ///
    /// In lenient mode, undefined variables render as empty strings
    /// instead of causing errors.
    pub fn lenient() -> Self {
        Self { _strict: false }
    }

    /// Create a minijinja environment with standard configuration.
    fn create_env(&self) -> Environment<'static> {
        // minijinja uses {{ }}, {% %}, {# #} by default, which is what we want
        Environment::new()
    }

    /// Render a template string with the given context.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let engine = TemplateEngine::new();
    /// let context = Context::new()
    ///     .with("name", "auth")
    ///     .with("count", 42);
    /// let result = engine.render("Hello {{ name }}, count={{ count }}", &context)?;
    /// assert_eq!(result, "Hello auth, count=42");
    /// ```
    pub fn render(&self, template: &str, context: &Context) -> Result<String, TemplateError> {
        let env = self.create_env();
        let tmpl = env.template_from_str(template)?;
        let result = tmpl.render(context.to_value())?;
        Ok(result)
    }

    /// Render a template with simple brace syntax (single braces).
    ///
    /// This converts `{var}` to `{{ var }}` before rendering,
    /// for compatibility with runbook examples that use single braces.
    pub fn render_simple(
        &self,
        template: &str,
        context: &Context,
    ) -> Result<String, TemplateError> {
        let converted = convert_simple_braces(template);
        self.render(&converted, context)
    }
}

/// Convert simple brace syntax `{var}` to Jinja2 syntax `{{ var }}`.
///
/// Only converts single braces that aren't already double braces.
/// Handles nested object access like `{bug.id}`.
fn convert_simple_braces(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        if c == '{' {
            // Check if this is already a double brace or Jinja2 block
            if i + 1 < len {
                let next = chars[i + 1];
                if next == '{' || next == '%' || next == '#' {
                    // Already Jinja2 syntax - find the matching closing delimiter
                    let close_char = match next {
                        '{' => '}',
                        '%' => '%',
                        '#' => '#',
                        _ => unreachable!(),
                    };

                    // Copy opening delimiter
                    result.push(c);
                    result.push(next);
                    i += 2;

                    // Copy until we find the closing delimiter
                    while i < len {
                        if chars[i] == close_char && i + 1 < len && chars[i + 1] == '}' {
                            result.push(chars[i]);
                            result.push(chars[i + 1]);
                            i += 2;
                            break;
                        }
                        result.push(chars[i]);
                        i += 1;
                    }
                    continue;
                }
            }

            // Find the matching closing brace for simple syntax
            let start = i + 1;
            let mut depth = 1;
            let mut end = start;
            while end < len && depth > 0 {
                if chars[end] == '{' {
                    depth += 1;
                } else if chars[end] == '}' {
                    depth -= 1;
                }
                if depth > 0 {
                    end += 1;
                }
            }

            if depth == 0 && end > start {
                // Found a valid simple brace expression
                let expr: String = chars[start..end].iter().collect();
                // Only convert if it looks like a variable reference
                // (contains only alphanumeric, dots, underscores, brackets)
                if is_valid_var_expr(&expr) {
                    result.push_str("{{ ");
                    result.push_str(&expr);
                    result.push_str(" }}");
                    i = end + 1;
                    continue;
                }
            }
        }

        result.push(c);
        i += 1;
    }

    result
}

/// Check if a string looks like a valid variable expression.
fn is_valid_var_expr(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Allow: alphanumeric, dots, underscores, brackets, pipes (for filters)
    // Also allow spaces, quotes, and parentheses for filter arguments
    s.chars().all(|c| {
        c.is_alphanumeric()
            || c == '.'
            || c == '_'
            || c == '['
            || c == ']'
            || c == '|'
            || c == ' '
            || c == '"'
            || c == '\''
            || c == '('
            || c == ')'
            || c == ','
    })
}

/// Template context (variable bindings).
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: HashMap<String, ContextValue>,
}

/// A value in the template context.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextValue {
    String(String),
    Number(i64),
    Float(f64),
    Bool(bool),
    List(Vec<ContextValue>),
    Object(HashMap<String, ContextValue>),
    Null,
}

impl Context {
    /// Create an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string value to the context.
    pub fn with_string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.values
            .insert(key.into(), ContextValue::String(value.into()));
        self
    }

    /// Add a number value to the context.
    pub fn with_number(mut self, key: impl Into<String>, value: i64) -> Self {
        self.values.insert(key.into(), ContextValue::Number(value));
        self
    }

    /// Add a float value to the context.
    pub fn with_float(mut self, key: impl Into<String>, value: f64) -> Self {
        self.values.insert(key.into(), ContextValue::Float(value));
        self
    }

    /// Add a boolean value to the context.
    pub fn with_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.values.insert(key.into(), ContextValue::Bool(value));
        self
    }

    /// Add a list value to the context.
    pub fn with_list(mut self, key: impl Into<String>, value: Vec<ContextValue>) -> Self {
        self.values.insert(key.into(), ContextValue::List(value));
        self
    }

    /// Add an object value to the context.
    pub fn with_object(
        mut self,
        key: impl Into<String>,
        value: HashMap<String, ContextValue>,
    ) -> Self {
        self.values.insert(key.into(), ContextValue::Object(value));
        self
    }

    /// Add a context value.
    pub fn with_value(mut self, key: impl Into<String>, value: ContextValue) -> Self {
        self.values.insert(key.into(), value);
        self
    }

    /// Set a string value in the context.
    pub fn set_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values
            .insert(key.into(), ContextValue::String(value.into()));
    }

    /// Set an arbitrary value in the context.
    pub fn set_value(&mut self, key: impl Into<String>, value: ContextValue) {
        self.values.insert(key.into(), value);
    }

    /// Create a context from a HashMap of strings.
    pub fn from_strings(map: HashMap<String, String>) -> Self {
        let values = map
            .into_iter()
            .map(|(k, v)| (k, ContextValue::String(v)))
            .collect();
        Self { values }
    }

    /// Convert the context to a minijinja Value.
    fn to_value(&self) -> Value {
        context_value_to_minijinja(&ContextValue::Object(self.values.clone()))
    }
}

/// Convert a ContextValue to a minijinja Value.
fn context_value_to_minijinja(cv: &ContextValue) -> Value {
    match cv {
        ContextValue::String(s) => Value::from(s.clone()),
        ContextValue::Number(n) => Value::from(*n),
        ContextValue::Float(f) => Value::from(*f),
        ContextValue::Bool(b) => Value::from(*b),
        ContextValue::List(list) => Value::from(
            list.iter()
                .map(context_value_to_minijinja)
                .collect::<Vec<_>>(),
        ),
        ContextValue::Object(obj) => {
            let map: Vec<(String, Value)> = obj
                .iter()
                .map(|(k, v)| (k.clone(), context_value_to_minijinja(v)))
                .collect();
            Value::from_iter(map)
        }
        ContextValue::Null => Value::from(()),
    }
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;

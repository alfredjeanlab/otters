// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use std::str::FromStr;

// ============================================================================
// JSON parsing
// ============================================================================

#[test]
fn parse_json_object() {
    let input = r#"{"name": "auth", "count": 42}"#;
    let result = parse_input(input, InputFormat::Json).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
        assert_eq!(map.get("count"), Some(&ContextValue::Number(42)));
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_json_array() {
    let input = r#"["a", "b", "c"]"#;
    let result = parse_input(input, InputFormat::Json).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], ContextValue::String("a".to_string()));
        assert_eq!(list[1], ContextValue::String("b".to_string()));
        assert_eq!(list[2], ContextValue::String("c".to_string()));
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_json_nested() {
    let input = r#"{"bug": {"id": 123, "title": "Fix login"}}"#;
    let result = parse_input(input, InputFormat::Json).unwrap();

    if let ContextValue::Object(map) = result {
        if let Some(ContextValue::Object(bug)) = map.get("bug") {
            assert_eq!(bug.get("id"), Some(&ContextValue::Number(123)));
            assert_eq!(
                bug.get("title"),
                Some(&ContextValue::String("Fix login".to_string()))
            );
        } else {
            panic!("Expected nested object");
        }
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_json_with_types() {
    let input = r#"{"str": "hello", "int": 42, "float": 3.25, "bool": true, "null": null}"#;
    let result = parse_input(input, InputFormat::Json).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("str"),
            Some(&ContextValue::String("hello".to_string()))
        );
        assert_eq!(map.get("int"), Some(&ContextValue::Number(42)));
        assert_eq!(map.get("float"), Some(&ContextValue::Float(3.25)));
        assert_eq!(map.get("bool"), Some(&ContextValue::Bool(true)));
        assert_eq!(map.get("null"), Some(&ContextValue::Null));
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_json_invalid() {
    let input = "not json";
    let result = parse_input(input, InputFormat::Json);
    assert!(result.is_err());
}

// ============================================================================
// Lines parsing
// ============================================================================

#[test]
fn parse_lines_basic() {
    let input = "line1\nline2\nline3";
    let result = parse_input(input, InputFormat::Lines).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], ContextValue::String("line1".to_string()));
        assert_eq!(list[1], ContextValue::String("line2".to_string()));
        assert_eq!(list[2], ContextValue::String("line3".to_string()));
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_lines_single() {
    let input = "single line";
    let result = parse_input(input, InputFormat::Lines).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], ContextValue::String("single line".to_string()));
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_lines_empty() {
    let input = "";
    let result = parse_input(input, InputFormat::Lines).unwrap();

    if let ContextValue::List(list) = result {
        // Empty string produces zero lines
        assert_eq!(list.len(), 0);
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_lines_preserves_whitespace() {
    let input = "  indented\ttabbed  ";
    let result = parse_input(input, InputFormat::Lines).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(
            list[0],
            ContextValue::String("  indented\ttabbed  ".to_string())
        );
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

// ============================================================================
// CSV parsing
// ============================================================================

#[test]
fn parse_csv_basic() {
    let input = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    let result = parse_input(input, InputFormat::Csv).unwrap();

    if let ContextValue::List(rows) = result {
        assert_eq!(rows.len(), 2);

        if let ContextValue::Object(row0) = &rows[0] {
            assert_eq!(
                row0.get("name"),
                Some(&ContextValue::String("Alice".to_string()))
            );
            assert_eq!(
                row0.get("age"),
                Some(&ContextValue::String("30".to_string()))
            );
            assert_eq!(
                row0.get("city"),
                Some(&ContextValue::String("NYC".to_string()))
            );
        } else {
            panic!("Expected object in row 0");
        }

        if let ContextValue::Object(row1) = &rows[1] {
            assert_eq!(
                row1.get("name"),
                Some(&ContextValue::String("Bob".to_string()))
            );
        } else {
            panic!("Expected object in row 1");
        }
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_csv_headers_only() {
    let input = "name,age,city";
    let result = parse_input(input, InputFormat::Csv).unwrap();

    if let ContextValue::List(rows) = result {
        assert_eq!(rows.len(), 0);
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_csv_with_spaces() {
    let input = "name, age, city\n  Alice  , 30 , NYC  ";
    let result = parse_input(input, InputFormat::Csv).unwrap();

    if let ContextValue::List(rows) = result {
        assert_eq!(rows.len(), 1);
        if let ContextValue::Object(row) = &rows[0] {
            assert_eq!(
                row.get("age"),
                Some(&ContextValue::String("30".to_string()))
            );
        } else {
            panic!("Expected object");
        }
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_csv_empty() {
    let input = "";
    let result = parse_input(input, InputFormat::Csv).unwrap();

    if let ContextValue::List(rows) = result {
        assert_eq!(rows.len(), 0);
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

// ============================================================================
// Key-value parsing
// ============================================================================

#[test]
fn parse_kv_equals() {
    let input = "name=auth\ncount=42\nenabled=true";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
        assert_eq!(map.get("count"), Some(&ContextValue::Number(42)));
        assert_eq!(map.get("enabled"), Some(&ContextValue::Bool(true)));
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_kv_colon() {
    let input = "name: auth\ncount: 42";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
        assert_eq!(map.get("count"), Some(&ContextValue::Number(42)));
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_kv_with_quotes() {
    let input = "name=\"hello world\"\npath='some/path'";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("hello world".to_string()))
        );
        assert_eq!(
            map.get("path"),
            Some(&ContextValue::String("some/path".to_string()))
        );
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_kv_type_inference() {
    let input = "int=42\nfloat=3.25\nbool_true=yes\nbool_false=no\nnull=null\nstr=hello";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(map.get("int"), Some(&ContextValue::Number(42)));
        assert_eq!(map.get("float"), Some(&ContextValue::Float(3.25)));
        assert_eq!(map.get("bool_true"), Some(&ContextValue::Bool(true)));
        assert_eq!(map.get("bool_false"), Some(&ContextValue::Bool(false)));
        assert_eq!(map.get("null"), Some(&ContextValue::Null));
        assert_eq!(
            map.get("str"),
            Some(&ContextValue::String("hello".to_string()))
        );
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_kv_ignores_comments() {
    let input = "# This is a comment\nname=auth\n# Another comment";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_kv_ignores_empty_lines() {
    let input = "name=auth\n\ncount=42\n\n";
    let result = parse_input(input, InputFormat::KeyValue).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(map.len(), 2);
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

// ============================================================================
// Raw parsing
// ============================================================================

#[test]
fn parse_raw() {
    let input = "some raw\n  content\twith whitespace";
    let result = parse_input(input, InputFormat::Raw).unwrap();

    assert_eq!(
        result,
        ContextValue::String("some raw\n  content\twith whitespace".to_string())
    );
}

// ============================================================================
// Auto detection
// ============================================================================

#[test]
fn parse_auto_json_object() {
    let input = r#"{"name": "auth"}"#;
    let result = parse_input(input, InputFormat::Auto).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_auto_json_array() {
    let input = r#"["a", "b"]"#;
    let result = parse_input(input, InputFormat::Auto).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(list.len(), 2);
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_auto_key_value() {
    let input = "name=auth\ncount=42";
    let result = parse_input(input, InputFormat::Auto).unwrap();

    if let ContextValue::Object(map) = result {
        assert_eq!(
            map.get("name"),
            Some(&ContextValue::String("auth".to_string()))
        );
    } else {
        panic!("Expected object, got {:?}", result);
    }
}

#[test]
fn parse_auto_lines() {
    let input = "line one\nline two\nline three";
    let result = parse_input(input, InputFormat::Auto).unwrap();

    if let ContextValue::List(list) = result {
        assert_eq!(list.len(), 3);
    } else {
        panic!("Expected list, got {:?}", result);
    }
}

#[test]
fn parse_auto_single_line() {
    let input = "just a single line";
    let result = parse_input(input, InputFormat::Auto).unwrap();

    assert_eq!(
        result,
        ContextValue::String("just a single line".to_string())
    );
}

// ============================================================================
// Format parsing
// ============================================================================

#[test]
fn format_from_str() {
    assert_eq!(InputFormat::from_str("json").unwrap(), InputFormat::Json);
    assert_eq!(InputFormat::from_str("JSON").unwrap(), InputFormat::Json);
    assert_eq!(InputFormat::from_str("lines").unwrap(), InputFormat::Lines);
    assert_eq!(InputFormat::from_str("line").unwrap(), InputFormat::Lines);
    assert_eq!(InputFormat::from_str("csv").unwrap(), InputFormat::Csv);
    assert_eq!(InputFormat::from_str("kv").unwrap(), InputFormat::KeyValue);
    assert_eq!(
        InputFormat::from_str("keyvalue").unwrap(),
        InputFormat::KeyValue
    );
    assert_eq!(
        InputFormat::from_str("key-value").unwrap(),
        InputFormat::KeyValue
    );
    assert_eq!(InputFormat::from_str("raw").unwrap(), InputFormat::Raw);
    assert_eq!(InputFormat::from_str("text").unwrap(), InputFormat::Raw);
    assert_eq!(InputFormat::from_str("auto").unwrap(), InputFormat::Auto);
    assert_eq!(InputFormat::from_str("").unwrap(), InputFormat::Auto);
}

#[test]
fn format_from_str_invalid() {
    assert!(InputFormat::from_str("invalid").is_err());
    assert!(InputFormat::from_str("xml").is_err());
}

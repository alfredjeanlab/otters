// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Protocol unit tests

use std::collections::HashMap;

use super::*;
use oj_core::Event;

#[test]
fn encode_decode_roundtrip_request() {
    let request = Request::Event {
        event: Event::CommandInvoked {
            command: "build".to_string(),
            args: HashMap::from([("name".to_string(), "test".to_string())]),
        },
    };

    let encoded = encode(&request).expect("encode failed");
    let decoded: Request = decode(&encoded).expect("decode failed");

    assert_eq!(request, decoded);
}

#[test]
fn encode_decode_roundtrip_response() {
    let response = Response::Status {
        uptime_secs: 3600,
        pipelines_active: 5,
        sessions_active: 3,
    };

    let encoded = encode(&response).expect("encode failed");
    let decoded: Response = decode(&encoded).expect("decode failed");

    assert_eq!(response, decoded);
}

#[test]
fn encode_decode_query() {
    let request = Request::Query {
        query: Query::GetPipeline {
            id: "pipe-123".to_string(),
        },
    };

    let encoded = encode(&request).expect("encode failed");
    let decoded: Request = decode(&encoded).expect("decode failed");

    assert_eq!(request, decoded);
}

#[test]
fn encode_returns_json_without_length_prefix() {
    let response = Response::Ok;
    let encoded = encode(&response).expect("encode failed");

    // encode() returns raw JSON, no length prefix
    let json_str = std::str::from_utf8(&encoded).expect("should be valid UTF-8");
    assert!(
        json_str.starts_with('{'),
        "should be JSON object: {}",
        json_str
    );
}

#[test]
fn pipeline_summary_serialization() {
    let summary = PipelineSummary {
        id: "pipe-1".to_string(),
        name: "build feature".to_string(),
        kind: "build".to_string(),
        phase: "Execute".to_string(),
        phase_status: "Running".to_string(),
    };

    let response = Response::Pipelines {
        pipelines: vec![summary.clone()],
    };

    let encoded = encode(&response).expect("encode failed");
    let decoded: Response = decode(&encoded).expect("decode failed");

    match decoded {
        Response::Pipelines { pipelines } => {
            assert_eq!(pipelines.len(), 1);
            assert_eq!(pipelines[0], summary);
        }
        _ => panic!("Expected Pipelines response"),
    }
}

#[tokio::test]
async fn read_write_message_roundtrip() {
    let original = b"hello world";

    let mut buffer = Vec::new();
    write_message(&mut buffer, original)
        .await
        .expect("write failed");

    // write_message adds 4-byte length prefix
    assert_eq!(buffer.len(), 4 + original.len());

    let mut cursor = std::io::Cursor::new(buffer);
    let read_back = read_message(&mut cursor).await.expect("read failed");

    assert_eq!(read_back, original);
}

#[tokio::test]
async fn write_message_adds_length_prefix() {
    let data = b"test data";

    let mut buffer = Vec::new();
    write_message(&mut buffer, data)
        .await
        .expect("write failed");

    // First 4 bytes are the length prefix
    let len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

    // Length should match the data size
    assert_eq!(len, data.len());
    assert_eq!(&buffer[4..], data);
}

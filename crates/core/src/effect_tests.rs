// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn effect_serialization_roundtrip() {
    let effects = vec![
        Effect::Spawn {
            workspace_id: "ws-1".to_string(),
            command: "claude".to_string(),
            env: vec![("KEY".to_string(), "value".to_string())],
            cwd: Some(PathBuf::from("/custom/path")),
        },
        Effect::SetTimer {
            id: "timer-1".to_string(),
            duration: Duration::from_secs(60),
        },
        Effect::Kill {
            session_id: "sess-1".to_string(),
        },
        Effect::Shell {
            pipeline_id: "pipe-1".to_string(),
            phase: "init".to_string(),
            command: "echo hello".to_string(),
            cwd: PathBuf::from("/tmp"),
            env: [("KEY".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
        },
    ];

    for effect in effects {
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);
    }
}

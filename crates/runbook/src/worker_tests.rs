// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;

#[test]
fn worker_defaults() {
    let worker = WorkerDef {
        name: "builds".to_string(),
        concurrency: 1,
        pipelines: vec!["build".to_string()],
    };

    assert_eq!(worker.concurrency, 1);
    assert!(worker.pipelines.contains(&"build".to_string()));
}

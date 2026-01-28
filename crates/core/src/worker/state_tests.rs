// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

use super::*;
use crate::FakeClock;

#[test]
fn worker_lifecycle() {
    let clock = FakeClock::new();
    let mut worker = Worker::new("builds".to_string(), &clock);

    assert_eq!(worker.status, WorkerStatus::Stopped);
    assert!(!worker.is_available());

    worker.start(&clock);
    assert_eq!(worker.status, WorkerStatus::Idle);
    assert!(worker.is_available());

    worker.begin_processing("pipe-1".to_string(), &clock);
    assert_eq!(worker.status, WorkerStatus::Processing);
    assert_eq!(worker.current_pipeline, Some("pipe-1".to_string()));
    assert!(!worker.is_available());

    worker.finish_processing(&clock);
    assert_eq!(worker.status, WorkerStatus::Idle);
    assert!(worker.current_pipeline.is_none());
    assert!(worker.is_available());

    worker.stop();
    assert_eq!(worker.status, WorkerStatus::Stopped);
}

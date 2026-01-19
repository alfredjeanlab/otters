use super::*;
use tempfile::TempDir;

fn make_test_log() -> (EventLog, TempDir) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.log");
    let log = EventLog::open(path).unwrap();
    (log, tmp)
}

#[test]
fn append_and_read_events() {
    let (mut log, _tmp) = make_test_log();

    let event1 = Event::PipelineCreated {
        id: "p-1".to_string(),
        kind: "build".to_string(),
    };
    let event2 = Event::PipelineComplete {
        id: "p-1".to_string(),
    };

    log.append(event1).unwrap();
    log.append(event2).unwrap();

    let records = log.read_all().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].sequence, 1);
    assert_eq!(records[1].sequence, 2);
    assert_eq!(records[0].name, "pipeline:created");
    assert_eq!(records[1].name, "pipeline:complete");
}

#[test]
fn query_by_pattern() {
    let (mut log, _tmp) = make_test_log();

    log.append(Event::PipelineCreated {
        id: "p-1".to_string(),
        kind: "build".to_string(),
    })
    .unwrap();
    log.append(Event::TaskStarted {
        id: crate::task::TaskId("t-1".to_string()),
        session_id: crate::session::SessionId("s-1".to_string()),
    })
    .unwrap();
    log.append(Event::PipelineComplete {
        id: "p-1".to_string(),
    })
    .unwrap();

    let pattern = EventPattern::new("pipeline:*");
    let results = log.query(&pattern).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn query_after_sequence() {
    let (mut log, _tmp) = make_test_log();

    for i in 1..=5 {
        log.append(Event::TimerFired {
            id: format!("timer-{}", i),
        })
        .unwrap();
    }

    let results = log.after(3).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].sequence, 4);
    assert_eq!(results[1].sequence, 5);
}

#[test]
fn persists_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.log");

    // Write some events
    {
        let mut log = EventLog::open(path.clone()).unwrap();
        log.append(Event::PipelineComplete {
            id: "p-1".to_string(),
        })
        .unwrap();
        log.append(Event::PipelineComplete {
            id: "p-2".to_string(),
        })
        .unwrap();
    }

    // Reopen and verify
    {
        let log = EventLog::open(path).unwrap();
        assert_eq!(log.current_sequence(), 2);

        let records = log.read_all().unwrap();
        assert_eq!(records.len(), 2);
    }
}

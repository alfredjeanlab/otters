# Epic 7c: Session Heartbeat Durability

**Root Feature:** `otters-6751`

Fix missing heartbeat durability by implementing `SessionHeartbeat` WAL operation as specified in the design doc.

## 1. Problem Statement

Session and task heartbeats are not persisted to WAL. After crash recovery:
- `Task.last_heartbeat` is `None` (see `snapshot.rs:207`, `state.rs:156`)
- Tasks may incorrectly transition to `Stuck` state on next tick
- Design doc (`docs/04-architecture/05-storage.md:140-142`) specifies `SessionHeartbeat` but it was never implemented

Lock and Semaphore heartbeats ARE durable (inconsistent).

## 2. Design Rationale

Per `docs/01-concepts/EXECUTION.md`:
> **Session** provides: Isolation, **Monitoring**, Control

Session owns the "monitoring" responsibility. Heartbeat tracking belongs at the Session level:

| Module | Responsibility |
|--------|----------------|
| Session | "Am I alive? When was last activity?" |
| Task | "Is my session responsive? Am I stuck?" |

Current implementation incorrectly routes heartbeats through Task. This plan fixes that.

## 3. Implementation Phases

### Phase 1: Add `last_heartbeat` to Session

**File: `crates/core/src/session.rs`**

Add field to Session struct:

```rust
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub state: SessionState,
    pub idle_threshold: Duration,
    // NEW: Last detected activity
    pub last_heartbeat: Option<Instant>,
}
```

Add heartbeat event and transition:

```rust
pub enum SessionEvent {
    // ... existing variants ...
    /// Activity detected in session
    Heartbeat { timestamp: Instant },
}

impl Session {
    pub fn transition(&self, event: SessionEvent, now: Instant) -> (Session, Vec<Effect>) {
        match (&self.state, event) {
            // ... existing transitions ...

            // Any state: heartbeat updates last activity
            (_, SessionEvent::Heartbeat { timestamp }) => {
                let session = Session {
                    last_heartbeat: Some(timestamp),
                    ..self.clone()
                };
                (session, vec![])
            }
        }
    }

    /// Time since last heartbeat (for stuck detection)
    pub fn idle_time(&self, now: Instant) -> Option<Duration> {
        self.last_heartbeat.map(|hb| now.duration_since(hb))
    }

    /// Check if session is idle beyond threshold
    pub fn is_idle(&self, now: Instant) -> bool {
        self.idle_time(now)
            .map(|idle| idle > self.idle_threshold)
            .unwrap_or(false)
    }
}
```

**File: `crates/core/src/session_tests.rs`**

Add tests:

```rust
#[test]
fn heartbeat_updates_last_activity() {
    let clock = FakeClock::new();
    let session = Session::new(...);

    assert!(session.last_heartbeat.is_none());

    let (session, _) = session.transition(
        SessionEvent::Heartbeat { timestamp: clock.now() },
        clock.now(),
    );

    assert!(session.last_heartbeat.is_some());
}

#[test]
fn idle_time_calculated_correctly() {
    let clock = FakeClock::new();
    let session = Session {
        last_heartbeat: Some(clock.now()),
        ..Default::default()
    };

    clock.advance(Duration::from_secs(60));

    assert_eq!(session.idle_time(clock.now()), Some(Duration::from_secs(60)));
}
```

**Milestone**: Session tracks heartbeats in memory.

---

### Phase 2: Add SessionHeartbeat WAL Operation

**File: `crates/core/src/storage/wal/operation.rs`**

Add operation:

```rust
// Session operations
SessionCreate(SessionCreateOp),
SessionTransition(SessionTransitionOp),
SessionHeartbeat(SessionHeartbeatOp),  // NEW
SessionDelete(SessionDeleteOp),

// ...

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHeartbeatOp {
    pub id: String,
    pub timestamp_micros: i64,
}
```

**File: `crates/core/src/storage/wal/state.rs`**

Add handler in `apply()`:

```rust
Operation::SessionHeartbeat(hb) => {
    let id = SessionId(hb.id.clone());
    if let Some(session) = self.sessions.get_mut(&id) {
        // Reconstruct Instant from timestamp using age pattern
        session.last_heartbeat = Some(clock.now());
    }
}
```

**File: `crates/core/src/storage/wal/operation_tests.rs`**

Add serialization test:

```rust
#[test]
fn session_heartbeat_roundtrip() {
    let op = Operation::SessionHeartbeat(SessionHeartbeatOp {
        id: "session-123".to_string(),
        timestamp_micros: 1705123456789000,
    });
    let json = serde_json::to_string(&op).unwrap();
    let parsed: Operation = serde_json::from_str(&json).unwrap();
    assert_eq!(op, parsed);
}
```

**Milestone**: SessionHeartbeat operation exists and applies correctly.

---

### Phase 3: Add WalStore Method

**File: `crates/core/src/storage/wal/store.rs`**

Add method:

```rust
/// Record a session heartbeat
pub fn session_heartbeat(&mut self, session_id: &str) -> Result<(), WalStoreError> {
    let now = SystemClock.now_system();
    let timestamp_micros = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| WalStoreError::ClockError)?
        .as_micros() as i64;

    self.execute(Operation::SessionHeartbeat(SessionHeartbeatOp {
        id: session_id.to_string(),
        timestamp_micros,
    }))
}
```

**File: `crates/core/src/storage/wal/store_tests.rs`**

Add durability test:

```rust
#[test]
fn session_heartbeat_survives_recovery() {
    let dir = tempdir().unwrap();
    let path = dir.path();

    // Create store, add session, heartbeat it
    {
        let mut store = WalStore::open_default(path).unwrap();
        store.session_create("session-1", "workspace-1", 300).unwrap();
        store.session_heartbeat("session-1").unwrap();
    }

    // Reopen and verify heartbeat was restored
    {
        let store = WalStore::open_default(path).unwrap();
        let session = store.state().session(&SessionId("session-1".into())).unwrap();
        assert!(session.last_heartbeat.is_some());
    }
}
```

**Milestone**: Session heartbeats persist and recover.

---

### Phase 4: Persist in Snapshot

**File: `crates/core/src/storage/wal/snapshot.rs`**

Update `StorableSession`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableSession {
    pub id: String,
    pub workspace_id: String,
    pub state: String,
    pub idle_threshold_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_reason: Option<String>,
    // NEW: Heartbeat age for reconstruction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_age_micros: Option<u64>,
}

impl From<&Session> for StorableSession {
    fn from(session: &Session) -> Self {
        let clock = SystemClock;
        Self {
            id: session.id.0.clone(),
            workspace_id: session.workspace_id.0.clone(),
            state: session_state_to_string(&session.state),
            idle_threshold_secs: session.idle_threshold.as_secs(),
            death_reason: extract_death_reason(&session.state),
            last_heartbeat_age_micros: session.last_heartbeat
                .map(|hb| clock.now().duration_since(hb).as_micros() as u64),
        }
    }
}
```

Update `to_session()`:

```rust
impl StorableSession {
    pub fn to_session(&self, clock: &impl Clock) -> Session {
        Session {
            id: SessionId(self.id.clone()),
            workspace_id: WorkspaceId(self.workspace_id.clone()),
            state: session_state_from_string(&self.state, &self.death_reason),
            idle_threshold: Duration::from_secs(self.idle_threshold_secs),
            last_heartbeat: self.last_heartbeat_age_micros
                .map(|age| clock.now() - Duration::from_micros(age)),
        }
    }
}
```

**File: `crates/core/src/storage/wal/snapshot_tests.rs`**

Add roundtrip test:

```rust
#[test]
fn session_heartbeat_survives_snapshot() {
    let clock = FakeClock::new();
    let session = Session {
        id: SessionId("s1".into()),
        last_heartbeat: Some(clock.now()),
        ..Default::default()
    };

    let storable = StorableSession::from(&session);
    let restored = storable.to_session(&clock);

    assert!(restored.last_heartbeat.is_some());
}
```

**Milestone**: Session heartbeats survive snapshot/restore cycle.

---

### Phase 5: Refactor Task Stuck Detection

**File: `crates/core/src/task.rs`**

Remove `last_heartbeat` from Task (or deprecate):

```rust
pub struct Task {
    pub id: TaskId,
    pub pipeline_id: PipelineId,
    pub session_id: Option<crate::session::SessionId>,
    pub state: TaskState,
    pub started_at: Instant,
    pub stuck_threshold: Duration,
    pub heartbeat_interval: Duration,
    // REMOVE or mark deprecated:
    // pub last_heartbeat: Option<Instant>,
}
```

Update stuck detection to use external session check:

```rust
impl Task {
    /// Check if task is stuck (session idle too long)
    ///
    /// Note: Caller must provide session idle time. Task no longer
    /// tracks heartbeats directly - that's Session's responsibility.
    pub fn is_stuck(&self, session_idle_time: Option<Duration>) -> bool {
        match session_idle_time {
            Some(idle) => idle > self.stuck_threshold,
            None => false, // No session or no heartbeat yet
        }
    }
}
```

Update `TaskEvent::Tick` handling:

```rust
// Old: Task tracked heartbeat internally
(TaskState::Running, TaskEvent::Tick) => {
    if let Some(last) = self.last_heartbeat {
        if now.duration_since(last) > self.stuck_threshold {
            // transition to Stuck
        }
    }
}

// New: Engine provides session idle time
(TaskState::Running, TaskEvent::Tick { session_idle_time }) => {
    if self.is_stuck(session_idle_time) {
        // transition to Stuck
    }
}
```

**Milestone**: Task delegates heartbeat tracking to Session.

---

### Phase 6: Update Engine Signal Handling

**File: `crates/core/src/engine/signals.rs`**

Update `process_heartbeat` to persist to Session:

```rust
/// Process heartbeat from session output monitoring
pub async fn process_heartbeat(
    &mut self,
    session_id: &crate::session::SessionId,
) -> Result<(), EngineError> {
    // Persist heartbeat to WAL (Session owns heartbeat state)
    self.store.session_heartbeat(&session_id.0)?;

    // Update in-memory session state
    if let Some(session) = self.sessions.get(&session_id) {
        let (new_session, effects) = session.transition(
            SessionEvent::Heartbeat { timestamp: self.clock().now() },
            self.clock().now(),
        );
        self.sessions.insert(session_id.clone(), new_session);
        self.execute_effects(effects).await?;
    }

    Ok(())
}
```

Update `poll_sessions` to pass session idle time to task tick:

```rust
pub async fn tick_tasks(&mut self) -> Result<(), EngineError> {
    for task in self.tasks.values() {
        if let TaskState::Running = task.state {
            // Get session idle time for stuck detection
            let session_idle_time = task.session_id
                .as_ref()
                .and_then(|sid| self.sessions.get(sid))
                .and_then(|session| session.idle_time(self.clock().now()));

            // Tick task with session context
            let (new_task, effects) = task.transition(
                TaskEvent::Tick { session_idle_time },
                self.clock().now(),
            );

            // ... apply transition
        }
    }
    Ok(())
}
```

**Milestone**: Engine routes heartbeats to sessions correctly.

---

### Phase 7: Remove Task Heartbeat Tracking

**File: `crates/core/src/task.rs`**

Remove deprecated field and events:

```rust
// Remove from Task struct:
// pub last_heartbeat: Option<Instant>,

// Remove TaskEvent variant (or keep for backward compat):
pub enum TaskEvent {
    // ...
    // Heartbeat { timestamp: Instant },  // REMOVE
    // ...
}

// Update Tick to include session context:
pub enum TaskEvent {
    // ...
    Tick { session_idle_time: Option<Duration> },
    // ...
}
```

**File: `crates/core/src/storage/wal/snapshot.rs`**

Remove from StorableTask:

```rust
// Remove:
// pub last_heartbeat_age_micros: Option<u64>,
```

**File: `crates/core/src/storage/wal/state.rs`**

Update task creation to not set heartbeat:

```rust
// Remove:
// last_heartbeat: None,
```

**Milestone**: Task no longer owns heartbeat state.

---

### Phase 8: Update Documentation

**File: `docs/04-architecture/05-storage.md`**

Verify `SessionHeartbeat` is documented (it already is at lines 140-142).

**File: `crates/core/src/storage/wal/CLAUDE.md`**

Update operation table to include SessionHeartbeat:

```markdown
| Session | create, transition, heartbeat, delete |
```

**File: `crates/core/src/session.rs` (module doc)**

Add note about heartbeat responsibility:

```rust
//! ## Heartbeat Tracking
//!
//! Session owns heartbeat state (`last_heartbeat`). This is persisted via
//! the `SessionHeartbeat` WAL operation. Tasks check session liveness via
//! `Session::idle_time()` for stuck detection.
```

**Milestone**: Documentation matches implementation.

---

## 4. File Summary

| File | Changes |
|------|---------|
| `session.rs` | Add `last_heartbeat`, `idle_time()`, `is_idle()`, heartbeat event |
| `session_tests.rs` | Add heartbeat tests |
| `operation.rs` | Add `SessionHeartbeat(SessionHeartbeatOp)` |
| `state.rs` | Handle `SessionHeartbeat` in apply() |
| `store.rs` | Add `session_heartbeat()` method |
| `snapshot.rs` | Add `last_heartbeat_age_micros` to StorableSession |
| `task.rs` | Remove `last_heartbeat`, update stuck detection |
| `signals.rs` | Route heartbeats to Session, pass idle time to Task tick |
| `CLAUDE.md` | Update operation table |

## 5. Verification

```bash
# Run tests
cargo test -p otters-core session
cargo test -p otters-core heartbeat

# Verify no Task heartbeat references remain
grep -r "Task.*heartbeat\|last_heartbeat" crates/core/src/task*.rs

# Verify SessionHeartbeat exists
grep -r "SessionHeartbeat" crates/core/src/storage/wal/

# Full check
make check
```

## 6. Migration Notes

- **Backward compatibility**: Old WAL files without `SessionHeartbeat` entries work fine - sessions start with `last_heartbeat: None`
- **Snapshot compatibility**: `#[serde(default)]` on new field ensures old snapshots load correctly
- **No data migration needed**: Heartbeat state is ephemeral; starting fresh after upgrade is acceptable

## 7. Relationship to Epic 8b

Epic 8b's watchers may monitor session/task state for stuck detection. This fix ensures:
- Watchers get accurate `session.idle_time()` after recovery
- No false positives from missing heartbeat data
- Consistent behavior pre/post crash

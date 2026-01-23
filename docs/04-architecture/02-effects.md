# Effects

All side effects are represented as data, not function calls. The functional core returns effects; the imperative shell executes them.

## Effect Types

```rust
pub enum Effect {
    // Events
    Emit { event: Event },

    // Session management
    Spawn { workspace: WorkspaceId, command: String },
    Send { session: SessionId, input: String },
    Kill { session: SessionId },

    // Git operations
    WorktreeAdd { branch: String, path: PathBuf },
    WorktreeRemove { path: PathBuf },

    // Coordination
    AcquireLock { name: String, holder: HolderId },
    ReleaseLock { name: String, holder: HolderId },
    AcquireSemaphore { name: String, holder: HolderId, slots: u32 },
    ReleaseSemaphore { name: String, holder: HolderId },

    // Queue
    DeadLetter { queue: QueueId, item: QueueItem },

    // Worker
    WakeWorker { worker: WorkerId },

    // Timers
    SetTimer { id: String, duration: Duration },
    CancelTimer { id: String },

    // Notification
    Notify { title: String, message: String },

    // Storage
    Persist { operation: Operation },
}
```

## Why Effects as Data

Effects as data enables:

1. **Testability** - Assert on effects without executing I/O
2. **Logging** - Inspect effects before execution
3. **Dry-run** - Validate without side effects
4. **Replay** - Debug by replaying effect sequences

## Execution

The executor loop:

```
loop {
    event = next_event()
    (new_state, effects) = state.transition(event, clock)
    for effect in effects {
        execute(effect, adapters)
    }
    state = new_state
}
```

Effects are executed via adapters:

| Effect | Adapter |
|--------|---------|
| Spawn, Send, Kill | SessionAdapter |
| WorktreeAdd, WorktreeRemove | RepoAdapter |
| Notify | NotifyAdapter |
| Persist | Storage (WAL) |

Coordination effects (Lock, Semaphore) update in-memory state and persist via WAL.

## Timer Effects

Timers schedule future events:

```rust
// State machine returns timer effect
Effect::SetTimer {
    id: "monitor:agent_idle:check",
    duration: Duration::from_secs(30)
}

// Later, scheduler delivers timer event
Event::Timer { id: "monitor:agent_idle:check" }
```

Timer IDs use namespacing (`{component}:{instance}:{action}`) to avoid collisions.

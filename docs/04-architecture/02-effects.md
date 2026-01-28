# Effects

All side effects are represented as data, not function calls. The functional core returns effects; the imperative shell executes them.

## Effect Types

```rust
pub enum Effect {
    // Events
    Emit { event: Event },

    // Session management
    Spawn {
        workspace_id: String,
        command: String,
        env: Vec<(String, String)>,
        cwd: Option<PathBuf>,
    },
    Send { session_id: String, input: String },
    Kill { session_id: String },

    // Git operations
    WorktreeAdd { branch: String, path: PathBuf },
    WorktreeRemove { path: PathBuf },

    // Shell execution
    Shell {
        pipeline_id: String,
        phase: String,
        command: String,
        cwd: PathBuf,
        env: HashMap<String, String>,
    },

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
| Shell | Direct subprocess |
| Persist | Storage (WAL) |

## Instrumentation

Effects implement `TracedEffect` for consistent observability:

```rust
pub trait TracedEffect {
    /// Effect name for log spans (e.g., "spawn", "shell")
    fn name(&self) -> &'static str;

    /// Key-value pairs for structured logging
    fn fields(&self) -> Vec<(&'static str, String)>;
}
```

The executor wraps all effect execution with tracing:

```rust
pub async fn execute(&self, effect: Effect) -> Result<Option<Event>, ExecuteError> {
    // Create span with effect name
    // Log entry with effect-specific fields
    // Record start time
    // Execute effect
    // Log completion or error with elapsed time
}
```

This provides:
- Entry logging with effect-specific fields
- Timing metrics on every operation
- Consistent error logging with context

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

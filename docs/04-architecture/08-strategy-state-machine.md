# Strategy State Machine Diagram

Detailed state machine diagram for the Strategy primitive.

```
                    ┌─────────────────────────────────────┐
                    │                                     │
                    ▼                                     │
┌──────┐  Start  ┌──────────────┐  CheckpointComplete  ┌──────────┐
│Ready │────────▶│Checkpointing │──────────────────────▶│Trying(n) │
└──────┘         └──────────────┘                       └──────────┘
    │                   │                                    │
    │                   │ CheckpointFailed                   │
    │                   ▼                                    │
    │              ┌────────┐                                │
    │              │Failed  │                                │
    │              └────────┘                                │
    │                                                        │
    │  Start (no checkpoint)                                 │
    └───────────────────────────────────────────────────────▶│
                                                             │
                    AttemptSucceeded                         │
                    TaskComplete                             │
              ┌─────────────────────────────────────────────┤
              │                                             │
              ▼                                             │
        ┌───────────┐                                       │
        │Succeeded  │                                       │
        └───────────┘                                       │
                                                             │
              AttemptFailed (with rollback)                  │
              TaskFailed (with rollback)                     │
              AttemptTimeout (with rollback)                 │
                                    ┌───────────────────────┤
                                    │                       │
                                    ▼                       │
                              ┌───────────────┐             │
                              │RollingBack(n) │             │
                              └───────────────┘             │
                                    │                       │
              RollbackComplete      │                       │
          ┌────────────────────────┤                       │
          │                        │                       │
          │  (more attempts)       │ RollbackFailed        │
          │       │                ▼                       │
          │       │          ┌────────┐                    │
          │       │          │Failed  │                    │
          │       │          └────────┘                    │
          │       ▼                                        │
          │   ┌──────────┐                                 │
          └──▶│Trying(n+1)│─────────────────────────────────┘
              └──────────┘
                    │
                    │ AttemptFailed (no rollback, no more attempts)
                    │ TaskFailed (no rollback, no more attempts)
                    │ AttemptTimeout (no rollback, no more attempts)
                    ▼
              ┌───────────┐
              │Exhausted  │
              └───────────┘
```

## State Descriptions

| State | Description |
|-------|-------------|
| Ready | Initial state, waiting to start |
| Checkpointing | Running checkpoint command to capture current state |
| Trying(n) | Executing attempt n |
| RollingBack(n) | Rolling back after attempt n failed |
| Succeeded | Strategy completed successfully |
| Exhausted | All attempts failed, action determined by `on_exhaust` |
| Failed | Unrecoverable failure (checkpoint or rollback failed) |

## Transitions

- **Start**: Begins strategy execution, enters Checkpointing if checkpoint defined
- **CheckpointComplete**: Checkpoint captured, begins first attempt
- **AttemptSucceeded/TaskComplete**: Attempt succeeded, strategy complete
- **AttemptFailed/TaskFailed/AttemptTimeout**: Attempt failed, rollback if defined, then try next
- **RollbackComplete**: Rollback finished, advance to next attempt
- **RollbackFailed**: Unrecoverable, enter Failed state

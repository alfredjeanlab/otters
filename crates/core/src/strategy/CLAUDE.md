# Strategy Module

See also [docs/04-architecture/08-strategy-state-machine.md](/docs/04-architecture/08-strategy-state-machine.md).

A pure functional state machine for fallback chains with checkpoint/rollback semantics.

## Overview

The Strategy primitive executes a sequence of approaches (attempts) until one succeeds. Key features:

- **Checkpoint**: Optionally capture state before attempting (e.g., `git rev-parse HEAD`)
- **Ordered attempts**: Tries approaches in sequence
- **Rollback**: If an attempt has a rollback command, it runs before trying the next approach
- **Exhaustion handling**: Configurable action when all attempts fail (escalate, fail, retry)

## State Machine

```mermaid
%%{init: {'flowchart': {'curve': 'linear'}}}%%
graph TD
    Start([Start])
    Ready["Ready"]
    Checkpointing["Checkpointing"]
    Trying["Trying"]
    RollingBack["RollingBack"]
    Succeeded["Succeeded"]
    Exhausted["Exhausted"]
    Failed["Failed"]
    End([End])

    Start --> Ready
    Ready -->|Start with checkpoint| Checkpointing
    Ready -->|Start no checkpoint| Trying
    Checkpointing -->|CheckpointComplete| Trying
    Checkpointing -->|CheckpointFailed| Failed
    Trying -->|AttemptSucceeded| Succeeded
    Trying -->|AttemptFailed with rollback| RollingBack
    Trying -->|AttemptFailed no rollback, more attempts| Trying
    Trying -->|AttemptFailed no rollback, no more| Exhausted
    RollingBack -->|RollbackComplete| Trying
    RollingBack -->|RollbackFailed| Failed
    Succeeded --> End
    Exhausted --> End
    Failed --> End
```

## Landing Checklist

- [ ] New effects are data structures (not closures)
- [ ] Effect handlers are async
- [ ] Error events emitted on failure
- [ ] Recovery path tested

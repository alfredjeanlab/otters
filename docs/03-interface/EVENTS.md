# Events

Events provide observability and enable loose coupling between components.

## System Events

Emitted by the runtime:

| Event | When |
|-------|------|
| `pipeline:started` | Pipeline begins |
| `pipeline:phase` | Phase transition |
| `pipeline:complete` | Pipeline finished successfully |
| `pipeline:failed` | Pipeline failed |
| `worker:started` | Worker daemon started |
| `worker:idle` | Worker has no work |
| `worker:stopped` | Worker daemon stopped |
| `session:stuck` | Session idle too long |
| `escalate` | Recovery actions exhausted, needs human |

## Runbook Events

Runbooks can emit custom events. Convention uses `category:action` format.

| Event | When |
|-------|------|
| `build:queued` | Build added to queue |
| `build:complete` | Build merged successfully |
| `merge:conflict` | Merge has unresolved conflicts |

## Emitting Events

Runbooks define events in their `[events]` section:

```toml
[events]
on_phase_change = "oj emit pipeline:phase --id {name} --phase {phase}"
on_complete = "oj emit pipeline:complete --id {name}"
on_fail = "oj emit pipeline:fail --id {name} --error '{error}'"
```

Event names are arbitrary strings. Convention uses `category:action` format.

## Consuming Events

### Wake Workers

Workers can wake on specific events instead of polling:
```toml
[worker.bugfix]
wake_on = ["bug:created", "bug:prioritized"]

[worker.builds]
wake_on = ["build:queued"]
```

### Wake Guards

Guards can wait for events instead of polling:
```toml
[guard.after_merged]
condition = "oj pipeline show {after} --phase | grep -q done"
wake_on = ["pipeline:{after}:complete"]
```

When the event fires, the guard re-evaluates its condition immediately.

### Notifications

Some events can trigger platform notifications:

- `pipeline:complete` → "auth merged"
- `escalate` → "build-auth needs attention"

Which events become notifications is configurable - not every event should notify. Most are for observability and coordination only.

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
| `agent:stuck` | Agent idle too long |
| `escalate` | Recovery actions exhausted, needs human |

## Runbook Events

Runbooks can emit custom events. Convention uses `category:action` format.

Example from `mergeq.toml`:

| Event | When |
|-------|------|
| `merge:queued` | Branch added to merge queue |
| `merge:complete` | Branch merged successfully |
| `merge:conflict` | Merge has unresolved conflicts |

## Emitting Events

Runbooks define events in their `[events]` section:

```toml
[events]
on_phase_change = "sp emit pipeline:phase --id {name} --phase {phase}"
on_complete = "sp emit pipeline:complete --id {name}"
on_fail = "sp emit pipeline:fail --id {name} --error '{error}'"
```

Event names are arbitrary strings. Convention uses `category:action` format.

## Consuming Events

### Wake Workers

Workers can wake on specific events instead of polling:
```toml
[worker.bugfix]
wake_on = ["bug:created", "bug:prioritized"]

[worker.mergeq]
wake_on = ["merge:queued"]
```

### Notifications

Some events can trigger platform notifications (see [MACOS.md](MACOS.md)).

Examples:
- `merge:queued` → "auth submitted for merge"
- `merge:complete` → "auth merged"
- `pipeline:started` → "auth implementation started"

Which events become notifications is configurable - not every event should notify.
Most are for observability and worker coordination only.

# Pipeline State Machine Diagram

Detailed state diagram for dynamic pipelines.

```
         +---------------------------------------------+
         v                                             |
       Init -> [Runbook Phases] -> Done               |
         |              |                              |
         +--------------+                              |
                   v                                   |
                Blocked <------------------------------+
                   |
                   v
                Failed
```

## State Flow

All pipelines are `PipelineKind::Dynamic`, created from runbook definitions. The actual phase sequence is determined by the runbook metadata stored in `outputs._runbook_phase` and `outputs._runbook_pipeline`.

### States

| State | Description |
|-------|-------------|
| Init | Pipeline created, waiting to start |
| [Runbook Phases] | Dynamic phases defined in runbook TOML |
| Blocked | Waiting on guard, lock, or external dependency |
| Done | Pipeline completed successfully |
| Failed | Unrecoverable failure |

### Transitions

- Phases transition based on `next` field in phase definition
- Any phase can enter `Blocked` waiting for resources
- Blocked phases resume when condition is met
- `on_fail` determines transition to Failed or escalation

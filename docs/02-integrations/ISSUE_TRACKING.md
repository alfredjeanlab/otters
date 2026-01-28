# Issue Tracking

How the issue tracker (`wok`) integrates with external systems.

## Core Concepts

Issues are work items with:
- **Type**: bug, feature, task
- **Status**: todo, in_progress, done, closed
- **Labels**: Arbitrary tags for filtering and grouping
- **Dependencies**: Blocking relationships between issues
- **Notes**: Append-only comments/updates

## CLI Integration

External systems interact via `wok` commands:

```bash
# Query
wok list [-l label] [-s status] [--json]
wok show <id> [--json]
wok ready                    # Unblocked todo issues

# Lifecycle
wok new <type> "title"       # Create issue
wok start <id>               # Claim (todo → in_progress)
wok done <id>                # Complete (in_progress → done)
wok close <id>               # Close without completing

# Metadata
wok label <id> <label>       # Add label
wok unlabel <id> <label>     # Remove label
wok note <id> "content"      # Add note

# Dependencies
wok dep <a> blocks <b>       # A blocks B
wok dep <epic> contains <x>  # Epic contains sub-issue
```

JSON output enables programmatic integration.

## Labels

Labels connect issues to external concepts:

| Pattern | Use |
|---------|-----|
| `mod:{path}` | Affected module/directory |
| `priority:N` | Priority (0=critical, 4=backlog) |
| `assigned` | Currently claimed |
| `needs-review` | Requires human attention |
| `blocked` | Waiting on dependency |

Systems define their own label schemes for tracking and filtering.

## Integration Patterns

### Work Queues

Issues feed work queues:

```text
wok list -l bug -s todo --json
    │
    ▼
queue filters/orders
    │
    ▼
worker claims (wok start)
    │
    ▼
worker completes (wok done)
```

### Progress Tracking

Large work decomposes into trackable issues:

```text
epic created
    │
    ├── sub-issue 1 (done)
    ├── sub-issue 2 (in_progress)
    └── sub-issue 3 (todo)

Progress: 1/3 complete
```

### Verification

External systems verify state before proceeding:

```bash
# All issues with label closed?
wok list -l build:auth -s todo,in_progress --count
# Returns 0 → safe to proceed
```

### Event Source

Issue changes can trigger external actions:
- Issue created → notify channel
- Issue labeled `urgent` → alert on-call
- All sub-issues done → proceed to next phase

### Context for Agents

Issues provide context for Claude Code:

```bash
wok show 42 --json
# → title, description, labels, notes, dependencies
```

Agents read this for task context, update via notes for progress tracking.

## Runbook Integration

Runbooks use issues for:
- **Queues**: Source work items from issue queries
- **Pipelines**: Create/track issue hierarchies
- **Guards**: Verify issue state before phase transitions
- **Workers**: Claim and complete issues

But issue tracking is independent - it can integrate with any system that speaks the CLI.

## Summary

| Integration | How |
|-------------|-----|
| **Query** | `wok list/show --json` |
| **Lifecycle** | `wok start/done/close` |
| **Labeling** | `wok label/unlabel` |
| **Progress** | `wok note`, dependency tracking |
| **Verification** | Count open issues with filters |

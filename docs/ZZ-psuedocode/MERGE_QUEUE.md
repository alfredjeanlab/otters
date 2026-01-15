# Merge Queue

The merge queue tracks **branches**, not workspaces. This enables multi-machine operation - a branch can be merged regardless of which machine created it.

## Queue Entry

| Field | Description |
|-------|-------------|
| branch | Branch name to merge |
| pipeline | Associated pipeline (optional) |
| priority | Higher = processed first |
| status | pending, processing, completed, failed |

## Readiness Check

Before processing an entry:

1. Branch exists on remote
2. If linked to pipeline: no active session, all issues closed
3. No other merge in progress

```ts
is_ready(entry):
    if not git.fetch_branch(entry.branch):
        return false  # branch gone

    if entry.pipeline:
        if session_running(entry.pipeline):
            return false  # still executing
        if not all_issues_closed(entry.pipeline):
            return false  # work incomplete

    if queue.any_processing():
        return false  # serialize merges

    return true
```

## Merge Strategy

Try approaches in order of preference:

1. **Fast-forward** - Cleanest, no merge commit
2. **Rebase + fast-forward** - Replay commits on main
3. **Agent resolution** - Spawn session to resolve conflicts
4. **Escalate** - Mark for human review

```ts
process_merge(entry):
    entry.status = "processing"
    checkpoint = git.rev_parse("HEAD")

    git.fetch_branch(entry.branch)

    # Try fast-forward
    if git.merge_ff_only(entry.branch):
        return finish_merge(entry)

    # Try rebase
    if git.rebase_onto_main(entry.branch):
        if git.merge_ff_only(entry.branch):
            return finish_merge(entry)
    git.rebase_abort()
    git.reset_hard(checkpoint)

    # Try agent resolution
    result = run_agent("conflict_resolution", entry, timeout=30m)
    if result == "success":
        return finish_merge(entry)
    git.reset_hard(checkpoint)

    # Give up
    entry.status = "failed"
    escalate("merge failed, needs intervention", entry)

finish_merge(entry):
    git.push_main()
    git.delete_remote_branch(entry.branch)
    entry.status = "completed"

    if entry.pipeline:
        entry.pipeline.phase = "done"
        unblock_dependents(entry.pipeline)
```

## Worker Loop

```ts
merge_worker():
    while running:
        entry = queue.next_pending_by_priority()

        if entry and is_ready(entry):
            lock.acquire("main_branch")
            process_merge(entry)
            lock.release("main_branch")
        else:
            wait(30s)
```

## Priority

Higher priority entries are processed first:

- Urgent bug fixes
- Dependencies that block other work
- Time-sensitive features

## Recovery

| Situation | Action |
|-----------|--------|
| Branch deleted | Remove from queue |
| Conflict unresolved | Escalate |
| Lock stale | Release → Rollback |
| Worker unhealthy | Restart |

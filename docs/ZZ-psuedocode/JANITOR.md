# Janitor

Clean up stale state and orphaned resources.

## Resources

| Resource | Stale When | Action |
|----------|-----------|--------|
| Queue entries | Branch deleted | Remove |
| Worktrees | No pipeline, branch merged | Remove (if clean) |
| Sessions | Dead, no pipeline | Kill |
| Locks | Heartbeat missed | Release |
| State files | Merged >N days | Archive |

## Safety Guards

- Never delete worktrees with uncommitted changes
- Verify branch merged before deleting worktree
- Audit log of cleanup actions
- Dry-run mode for testing

## Logic

```ts
janitor_scan():
    // Clean stale queue entries
    for entry in queue.list_all():
        if not branch_exists(entry.branch):
            queue.remove(entry)

    // Clean orphaned worktrees
    for worktree in git.worktrees():
        if no_pipeline_for(worktree) and is_clean(worktree):
            if branch_merged(worktree.branch):
                git.worktree_remove(worktree)
            else:
                log("orphaned worktree with unmerged branch", worktree)

    // Clean dead sessions
    for session in sessions.list(prefix: "sp-"):
        if not session.alive() and no_pipeline_for(session):
            session.kill()

    // Clean stale locks
    for lock in locks.list():
        if lock.is_stale():
            lock.force_release()
```

## Dirty Worktrees

Worktrees with uncommitted changes are never auto-deleted. Instead:

```ts
if no_pipeline_for(worktree) and not is_clean(worktree):
    escalate("dirty orphaned worktree needs manual cleanup", worktree)
```

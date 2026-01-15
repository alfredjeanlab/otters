# Build Feature Pipeline

Operations progress through a defined state machine:

```
init → [blocked] → plan → decompose → execute → merge → done
```

The `blocked` state handles dependencies, waiting for blocker operations to merge before proceeding.

**Terminal states:** Failed, Cancelled

## Phase Descriptions

| Phase | Description |
|-------|-------------|
| init | Create workspace and branch, check dependencies |
| blocked | Wait for blocker operation to merge |
| plan | Run agent with planning prompt |
| decompose | Run agent to create issues from plan |
| execute | Agent works on issues; wait for completion |
| merge | Integrate branch to main |
| done | Cleanup workspace, delete branch |

## Transitions

```ts
run_phase(phase, state):
    match phase:
        "init":
            git.worktree_add(state.workspace, state.branch)
            state.epic = issues.create_feature(state.prompt)

            if state.blocked_by and not is_merged(state.blocked_by):
                return "blocked"
            return "plan"

        "blocked":
            wait_until(() => is_merged(state.blocked_by), timeout=24h)
            return "plan"

        "plan":
            run_agent("planning", state, timeout=30m, idle_timeout=2m)
            assert(file_exists("plans/{name}.md"))
            return "decompose"

        "decompose":
            run_agent("decomposition", state, timeout=15m, idle_timeout=2m)
            assert(epic_has_issues(state.epic))
            return "execute"

        "execute":
            run_agent("execution", state,
                timeout=4h,
                idle_timeout=5m,
                on_stuck=["nudge", "restart"])

            // Verify work complete before merge
            if all_issues_closed(state.epic):
                return "merge"
            else if not state.execute_retried:
                state.execute_retried = true
                return "execute"              // retry once
            else:
                escalate("issues remain open")
                return "failed"

        "merge":
            queue.add("merges", { branch: state.branch, pipeline: state.name })
            wait_until(() => is_merged(state.name))
            return "done"

        "done":
            git.worktree_remove(state.workspace)
            git.push_delete_branch(state.branch)
            issues.close(state.epic)
            return "done"
```

See [MERGE_QUEUE.md](MERGE_QUEUE.md) for merge strategy details.

## Agent Monitoring

Agents are monitored for liveness via output heartbeat:

```ts
run_agent(name, state, timeout, idle_timeout, on_stuck=[]):
    session = spawn_agent(prompt=load_template(name, state), cwd=state.workspace)
    last_output = now()

    while session.running:
        if session.has_output():
            last_output = now()

        if now() - last_output > idle_timeout:
            for action in on_stuck:
                if action == "nudge":
                    session.send_signal()
                    wait(30s)
                    if session.has_output(): break
                if action == "restart":
                    session.kill()
                    session = spawn_agent(...)
                    break

        if now() - start > timeout:
            escalate("agent timeout")
            return "failed"

    return session.exit_code == 0 ? "success" : "failed"
```

## Recovery

| Situation | Action |
|-----------|--------|
| Agent idle | Nudge → Restart → Escalate |
| Agent timeout | Escalate |
| Guard timeout | Escalate |

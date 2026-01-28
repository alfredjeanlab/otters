# Runbook Concepts

A runbook is a file that defines **entrypoints** (things that run) and the **building blocks** they use.

## Summary

```
┌─────────────────────────────────────────────────────────────┐
│ ENTRYPOINTS (things that run)                               │
│                                                             │
│   command ──► user invokes, runs pipeline or queues work    │
│   worker ───► queue-driven daemon                           │
│   cron ─────► time-driven, runs monitors                    │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ BUILDING BLOCKS (composed by entrypoints)                   │
│                                                             │
│   queue ────► work items for workers                        │
│   pipeline ─► phased execution                              │
│   agent ────► AI agent invocation                           │
│   monitor ──► condition checking + response                 │
│   action ───► named operations                              │
│   guard ────► pre/post conditions                           │
│   strategy ─► fallback chain                                │
│   lock ─────► exclusive access                              │
│   semaphore ► limited concurrency                           │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ RECOVERY & OBSERVABILITY                                    │
│                                                             │
│   actions ──► nudge, restart, requeue, escalate, ...        │
│   events ───► emitted for tracking                          │
└─────────────────────────────────────────────────────────────┘
```

## Entrypoints

Three primitives define things that run:

| Primitive | Trigger | Lifecycle | Use case |
|-----------|---------|-----------|----------|
| **command** | User invokes | Runs once | `oj run build auth "Add auth"` |
| **worker** | Queue item | Start/stop/wake | Process bugs, merge branches |
| **cron** | Schedule | Enable/disable | Cleanup, monitoring |

```text
User ─── oj run ───► Command ─┬─► Pipeline ───► Agent (direct)
                              │
                              └─► Queue ───► Worker ───► Pipeline (background)
                                                │
Timer ─── interval ───► Cron ───► Monitor ───► Action
```

### Command

User-facing entrypoint. Accepts arguments, runs once.

```toml
[command.build]
args = "<name> <prompt>"
run = { pipeline = "build" }
```

Invoked: `oj run build auth "Add authentication"`

The `run` field specifies what to execute:
- Pipeline: `run = { pipeline = "build" }`
- Shell: `run = "echo hello"`

#### Argument Syntax

| Pattern | Meaning |
|---------|---------|
| `<name>` | Required positional |
| `[name]` | Optional positional |
| `<files...>` | Required variadic (1+) |
| `[files...]` | Optional variadic (0+) |
| `--flag` | Boolean flag |
| `-f/--flag` | Boolean flag with short alias |
| `--opt <val>` | Required flag with value |
| `[--opt <val>]` | Optional flag with value |
| `[-o/--opt <val>]` | Optional flag with value and short alias |

Complex example:
```toml
[command.deploy]
args = "<env> [-t/--tag <version>] [-f/--force] [targets...]"
defaults = { tag = "latest" }
```

Invoked: `oj run deploy prod -t v1.2 --force api worker`

### Worker

Queue-driven daemon. Processes items until stopped.

```toml
[worker.bugfix]
queue = "bugs"
handler = "pipeline.fix"
concurrency = 1
idle_action = "wait:30s"
wake_on = ["bug:created"]
```

Lifecycle: `oj worker start bugfix`, `oj worker stop bugfix`, `oj worker wake bugfix`

### Cron

Time-driven daemon. Runs monitors on schedule.

```toml
[cron.watchdog]
interval = "30s"
monitors = ["agent_idle", "phase_timeout", "stale_locks"]
```

Lifecycle: `oj cron enable watchdog`, `oj cron disable watchdog`

## Building Blocks

Primitives that entrypoints compose:

### Queue

Holds work items for workers to process.

```toml
[queue.bugs]
source = "wok list -l bug -s todo --json"
order = "priority"
visibility_timeout = "30m"
max_retries = 2
on_exhaust = "dead"

[queue.bugs.dead]
retention = "7d"
```

- **visibility_timeout**: How long item is hidden while processing
- **max_retries**: Attempts before dead letter
- **on_exhaust**: What to do when retries exhausted

### Pipeline

Phased execution with state tracking. Commands or workers invoke pipelines.

```toml
[pipeline.fix]
inputs = ["bug"]

[[pipeline.fix.phase]]
name = "setup"
run = "git worktree add {workspace} -b {branch}"
next = "fix"

[[pipeline.fix.phase]]
name = "fix"
run = { agent = "fix" }
semaphore = "agents"
post = ["tests_pass"]
next = "merge"
on_fail = "escalate"
```

The `run` field specifies what to execute:
- Shell command: `run = "git worktree add ..."`
- Agent reference: `run = { agent = "fix" }`
- Strategy reference: `run = { strategy = "merge" }`
- Pipeline reference: `run = { pipeline = "build" }` (from commands)

Phases can also:
- Require guards (`pre = [...]`, `post = [...]`)
- Acquire locks (`lock = "..."`)
- Acquire semaphore slots (`semaphore = "..."`)

Pipeline instances are tracked via `oj pipeline`:
```bash
oj pipeline list                 # Running pipelines
oj pipeline show build-auth      # State, phase, errors
oj pipeline transition build-auth merge
oj pipeline resume build-auth
```

### Agent

An AI agent invocation - runs Claude in a monitored session.

```toml
[agent.fix]
run = "claude --print"
prompt = "Fix the bug: {bug.description}"
cwd = "{workspace}"
on_idle = { action = "nudge", message = "Continue working on fixing the bug." }
on_exit = { action = "recover", message = "Previous attempt exited. Try again." }
on_error = "escalate"
```

- **on_idle**: What to do when agent is waiting for input (`nudge`, `done`, `escalate`)
- **on_exit**: What to do when agent process exits (`done`, `recover`, `restart`, `escalate`)
- **on_error**: What to do on API errors (`fail`, `escalate`)

**Note:** Agents can run indefinitely - there's no timeout. State detection uses Claude's session log, not arbitrary timeouts.

### Templates

Agents use templates for prompts. Templates support variable interpolation.

```toml
[agent.execution]
prompt_file = "templates/execute.md"
```

Example template (Jinja2-style):
```
# {{ name }}

## Issues to Complete

{% for issue in issues %}
- [ ] `{{ issue.id }}` - {{ issue.title }}
{% endfor %}

## Constraints

- Work in `{{ workspace }}/`
- Signal completion with `./done`
```

Templates receive context from the pipeline/agent:
- Pipeline inputs (`name`, `prompt`, etc.)
- Workspace details (`workspace`, `branch`)
- Output from source commands

```toml
[agent.execution]
inputs = [
    { name = "issues", source = "wok list -l plan:{name} --json" },
    { name = "plan", source = "cat plans/{name}.md" },
]
```

### Guard

Shell condition that must be true before/after a phase. Exit code 0 means condition met.

```toml
[guard.tests_pass]
condition = "make test"
retry = { max = 3, interval = "10s" }
timeout = "5m"
on_timeout = "escalate"
```

Used as `pre` (before phase) or `post` (after phase) conditions.

Guards can wait for events instead of polling:

```toml
[guard.after_merged]
condition = "test -z '{after}' || oj pipeline show {after} --phase | grep -q merged"
wake_on = ["pipeline:{after}:merged"]
```

### Strategy

Ordered fallback chain. Try approaches until one succeeds.

```toml
[strategy.merge]
checkpoint = "git rev-parse HEAD"
on_exhaust = "escalate"

[[strategy.merge.attempt]]
name = "fast-forward"
run = "git merge --ff-only FETCH_HEAD"
timeout = "1m"

[[strategy.merge.attempt]]
name = "rebase"
run = "git rebase origin/main"
timeout = "5m"
rollback = "git rebase --abort; git reset --hard {checkpoint}"

[[strategy.merge.attempt]]
name = "agent-resolve"
run = { agent = "conflict_resolution" }
timeout = "30m"
rollback = "git reset --hard {checkpoint}"
```

- **checkpoint**: State to restore on rollback
- **rollback**: Cleanup command if attempt fails
- **on_exhaust**: Action when all attempts fail

### Lock

Exclusive access to a resource.

```toml
[lock.main_branch]
timeout = "30m"
heartbeat = "30s"
on_stale = ["release", "rollback", "escalate"]
```

Only one holder at a time. Stale locks (no heartbeat) are reclaimed.

### Semaphore

Limited concurrency - N simultaneous holders.

```toml
[semaphore.agents]
max = 4
slot_timeout = "2h"
slot_heartbeat = "1m"
on_orphan = "reclaim"
on_orphan_work = "requeue"
```

Used to limit concurrent agent sessions, API calls, etc.

### Monitor

Checks a condition and triggers a response chain. Crons run monitors on schedule.

```toml
[monitor.agent_idle]
source = "oj pipeline list --phase execute --json"
condition = "oj session idle-time {session} > 5m"
response = ["nudge", "restart:2", "escalate"]
```

```toml
[monitor.stale_locks]
source = "oj lock list --json"
condition = "oj lock is-stale {id}"
run = "oj lock force-release {id}"
```

Monitors unify two use cases:
- **Watching**: Check condition, trigger response chain on match
- **Scanning**: Find resources, clean up those matching condition

The `source` provides items to check. The `condition` (shell command, exit 0 = match) filters them. Then either `response` (action chain with escalation) or `run` (direct shell command) handles matches.

### Action

Named operation with cooldown enforcement.

```toml
[action.nudge]
run = "oj session nudge {session}"
cooldown = "30s"

[action.restart]
run = """
oj session kill {session}
oj pipeline resume {name}
"""
cooldown = "5m"
max_attempts = 2
```

Actions are referenced by monitors and recovery chains.

## Recovery

Recovery actions form chains - try each until one succeeds:

| Action | Effect |
|--------|--------|
| `nudge` | Send message prompting agent to continue |
| `restart` | Kill and restart |
| `restart:N` | Restart up to N times |
| `requeue` | Put back in queue |
| `dead` | Move to dead letter |
| `escalate` | Alert for human intervention |
| `abandon` | Give up |
| `rollback` | Restore checkpoint |
| `release` | Release lock/semaphore |
| `reclaim` | Reclaim orphaned resource |

Example chain: `["nudge", "restart:2", "escalate"]`
1. Try nudge
2. If still stuck, restart (up to 2 times)
3. If still stuck, escalate to human

## Events

Emitted for observability. Can wake workers and guards.

Events are scoped to the primitive that owns them:

```toml
[pipeline.build.events]
on_phase = "echo '{name} -> {phase}' >> .oj/events.log"
on_complete = "echo '{name} complete' >> .oj/events.log"
on_fail = "echo '{name} failed: {error}' >> .oj/events.log"

[worker.bugfix]
wake_on = ["bug:created", "bug:prioritized"]

[worker.bugfix.events]
on_start = "echo 'bugfix worker started' >> .oj/events.log"
```

## File Organization

Each runbook file defines related primitives:

| File | Defines | Description |
|------|---------|-------------|
| `build.toml` | command, pipeline, agents | Feature development: plan → execute → merge |
| `bugfix.toml` | command, worker, queue, pipeline | Bug fixing: pick bug → fix → verify → merge |
| `watchdog.toml` | cron, monitors, actions | Stuck detection: nudge → restart → escalate |
| `janitor.toml` | cron, monitors | Cleanup: stale locks, worktrees, sessions |

Primitives are referenced by name within a runbook. Cross-runbook references use `runbook.primitive` syntax.

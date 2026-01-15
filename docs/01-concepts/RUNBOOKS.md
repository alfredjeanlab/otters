# Runbook Concepts

A runbook is a file that defines **entrypoints** (things that run) and the **building blocks** they use.

## Summary

```
┌─────────────────────────────────────────────────────────────┐
│ ENTRYPOINTS (things that run)                               │
│                                                             │
│   command ──► user invokes, runs once                       │
│   worker ───► queue-driven daemon                           │
│   cron ─────► time-driven daemon                            │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ BUILDING BLOCKS (composed by entrypoints)                   │
│                                                             │
│   queue ────► work items                                    │
│   pipeline ─► phased execution                              │
│   task ─────► single unit of work                           │
│   guard ────► pre/post conditions                           │
│   strategy ─► fallback chain                                │
│   lock ─────► exclusive access                              │
│   semaphore ► limited concurrency                           │
│   watcher ──► condition monitoring                          │
│   scanner ──► resource cleanup                              │
│   action ───► named operations                              │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ RECOVERY & OBSERVABILITY                                    │
│                                                             │
│   actions ──► nudge, restart, requeue, escalate, ...        │
│   events ───► emitted for tracking, can wake workers        │
└─────────────────────────────────────────────────────────────┘
```

## Entrypoints

Three primitives define things that run:

| Primitive | Trigger | Lifecycle | Use case |
|-----------|---------|-----------|----------|
| **command** | User invokes | Runs once | `oj run build name=auth` |
| **worker** | Queue item | Start/stop/wake | Process bugs, merge branches |
| **cron** | Schedule | Enable/disable | Cleanup, monitoring |

```
User ─── oj run ───► Command ───► queues work ───► wakes Worker
                                                        │
                                                        ▼
                                                   processes items
                                                        │
Timer ─── interval ───► Cron ───► runs watchers ───────►│
                                                        ▼
                                                   Pipeline/Task
```

### Command

User-facing entrypoint. Accepts arguments, runs once.

```toml
[command.build]
args = "<name> <prompt>"
run = """
oj queue add builds name={name} prompt={prompt}
oj worker wake builds
"""
```

Invoked: `oj run build auth "Add authentication"`

#### Argument Syntax

| Pattern | Meaning |
|---------|---------|
| `<name>` | Required positional |
| `[name]` | Optional positional |
| `<files...>` | Required variadic (1+) |
| `[files...]` | Optional variadic (0+) |
| `--flag` | Boolean flag |
| `--opt <val>` | Required flag with value |
| `[--opt <val>]` | Optional flag with value |
| `-f` | Short alias (defined separately) |

Complex example:
```toml
[command.deploy]
args = "<env> [--tag <version>] [--force] [targets...]"
aliases = { f = "force", t = "tag" }
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

Time-driven daemon. Runs on schedule.

```toml
[cron.watchdog]
interval = "30s"
watchers = ["agent_idle", "phase_timeout"]
```

Lifecycle: `oj cron enable watchdog`, `oj cron disable watchdog`

## Building Blocks

Primitives that entrypoints compose:

### Queue

Holds work items for workers to process.

```toml
[queue.bugs]
source = "wk list -l bug -s todo --json"
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

Phased execution with state tracking. Workers invoke pipelines to process items.

```toml
[pipeline.fix]
inputs = ["bug"]

[[pipeline.fix.phase]]
name = "setup"
run = "git worktree add {workspace} -b {branch}"
next = "fix"

[[pipeline.fix.phase]]
name = "fix"
task = "fix_task"
semaphore = "agents"
post = ["tests_pass"]
next = "merge"
on_fail = "escalate"
```

Phases can:
- Run scripts (`run = "..."`)
- Invoke tasks (`task = "..."`)
- Apply strategies (`strategy = "..."`)
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

### Task

Single unit of work - typically an agent invocation.

```toml
[task.fix_task]
command = "claude --print"
prompt = "Fix the bug: {bug.description}"
cwd = "{workspace}"
timeout = "1h"
idle_timeout = "3m"
heartbeat = "output"
on_stuck = ["nudge", "restart"]
on_timeout = "escalate"
```

- **heartbeat**: How to detect liveness (`output`, `file:/path`, `process`)
- **idle_timeout**: Max time without heartbeat
- **on_stuck**: Recovery chain when idle

### Templates

Tasks use templates for prompts and context files (CLAUDE.md). Templates support variable interpolation and loops.

```toml
[task.execution]
prompt_file = "templates/execute.md"
context_file = "CLAUDE.md"              # Generated in workspace
context_template = "templates/claude.feature.md"
```

Template engines with Rust support and loop constructs:
- **Tera** - Jinja2-like syntax
- **Handlebars** - Mustache-compatible
- **MiniJinja** - Minimal Jinja2

Example template (Jinja2-style):
```
# Task: {{ task.name }}

## Issues to Complete

{% for issue in issues %}
- [ ] `{{ issue.id }}` - {{ issue.title }}
{% endfor %}

## Constraints

- Work in `{{ workspace }}/`
- Signal completion with `./done`
```

Templates receive context from the pipeline/task:
- Pipeline inputs (`name`, `prompt`, etc.)
- Workspace details (`workspace`, `branch`)
- Output from source commands (JSON, or via supported parsers like `ls`, `git branch`)
- Custom variables from task definition

```toml
[task.execution]
inputs = [
    { name = "issues", source = "wk list -l plan:{name} --json" },
    { name = "plan", source = "cat plans/{name}.md" },
    { name = "files", source = "ls src/" },
    { name = "branches", source = "git branch --list 'feature/*'" },
]
```

### Guard

Condition that must be true before/after a phase.

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

```toml
[[pipeline.build.phase]]
name = "init"
pre = ["after_merged"]
# ...
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
task = "conflict_resolution"
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

### Watcher

Condition checker for crons. Monitors resources and triggers responses.

```toml
[watcher.agent_idle]
source = "oj pipeline list --phase execute --json"
condition = "oj session idle-time {session} > 5m"
response = ["nudge", "restart:2", "escalate"]
```

### Scanner

Resource scanner for crons. Finds and cleans up stale resources.

```toml
[scanner.stale_locks]
source = "oj lock list --json"
condition = "oj lock is-stale {id}"
action = "oj lock force-release {id}"
```

### Action

Named operation, used by watchers and decision rules.

```toml
[action.nudge]
run = "oj session nudge {session}"
cooldown = "30s"

[action.decide]
rules = [
    { if = "analysis == 'rate_limited'", then = "wait_retry", delay = "5m" },
    { if = "failure_count < 2", then = "retry" },
    { else = "escalate" }
]
```

## Recovery

Recovery actions form chains - try each until one succeeds:

| Action | Effect |
|--------|--------|
| `nudge` | Poke stuck processor |
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

Emitted for observability. Can wake workers.

```toml
[events]
on_phase_change = "oj emit pipeline:phase --id {name} --phase {phase}"
on_complete = "oj emit pipeline:complete --id {name}"

[worker.bugfix]
wake_on = ["bug:created", "bug:prioritized"]
```

## File Organization

Each runbook file defines related primitives:

| File | Defines | Description |
|------|---------|-------------|
| `build.toml` | command, worker, pipeline | Feature development: plan → decompose → execute → merge |
| `bugfix.toml` | command, worker, pipeline | Bug fixing: pick bug → fix → verify → merge |
| `loop.toml` | worker | Continuous issue resolution from queue |
| `mergeq.toml` | worker, pipeline | Merge queue processor |
| `janitor.toml` | cron, scanners | Cleanup: stale queues, worktrees, sessions |
| `watchdog.toml` | cron, watchers | Stuck detection: nudge → restart → escalate |
| `triager.toml` | cron, watchers, actions | Failure handling: analyze → decide → act |

Primitives are referenced by name within a runbook. Cross-runbook references use `runbook.primitive` syntax.

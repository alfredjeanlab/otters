# Claude Code Integration

How Claude Code integrates with external systems for orchestration, monitoring, and control.

## Sessions

Claude Code can run in managed sessions (tmux) for:
- **Process isolation**: Separate environment per invocation
- **Output capture**: Monitor stdout/stderr for heartbeat detection
- **Input injection**: Send keystrokes to nudge or interrupt stuck agents
- **Attach/detach**: Debug live sessions interactively
- **Clean termination**: Kill sessions when stuck or complete

Tmux enables bidirectional control - not just watching output but sending input (Ctrl-C to interrupt, text to nudge).

Session lifecycle is external to Claude Code - the orchestrator creates, monitors, and destroys sessions.

## Heartbeat Detection

Detecting if Claude Code is alive or stuck:

| Signal | Source |
|--------|--------|
| Terminal output | New content in stdout/stderr |
| Log file | New entries in Claude Code's log |
| Tool calls | API activity indicates processing |
| File writes | Checkpoint or artifact creation |
| Process status | PID still running |

Different contexts need different signals. A planning task produces steady output; a long computation may be silent. Orchestrators configure appropriate detection per use case.

### Log File Watching

Claude Code writes files that external systems can watch:

| Path | Contents |
|------|----------|
| `~/.claude/projects/.../{uuid}.jsonl` | Conversation turns, tool calls, results |
| `~/.claude/projects/.../plans/` | Saved plans |
| `~/.claude/projects/.../todos/` | Todo state |

Uses:
- **Heartbeat**: New JSONL entries indicate activity
- **Security**: Detect dangerous tool calls, policy violations
- **Fallibility**: Detect errors, crashes, incomplete work, off track, churning
- **Events**: Extract tool usage, plan changes, todo completions

Log watching works even when terminal output is suppressed.

## Fallibility

Agents are fallible - they forget to signal, get stuck, exhaust context, or exit unexpectedly. Detection is **multi-channel**:

| Channel | Detects |
|---------|---------|
| MCP tool calls | Explicit signals (done, error, restart) |
| Prompt instructions | "Signal when done" |
| Heartbeat monitoring | Activity/inactivity |
| Log file watching | Errors, tool usage, progress |
| Claude hooks | Implicit events from tool use |
| External events | Git push, issue closed, artifact created |

Redundancy matters - agents may forget one channel but trigger another.

### Completion Signals

Explicit signaling via MCP tools:
- **Success**: Task completed normally
- **Failure**: Task failed, with reason
- **Restart**: Context exhausted, need fresh session

Implicit completion detection:
- Process exited cleanly
- Expected artifact exists (file, issue state, etc.)
- External state matches expectations
- External event received (commit pushed, issue closed)

### Idle + Artifact Detection

Idle alone doesn't mean done. Check both:
1. No output/activity for threshold period
2. Expected outcome exists (artifact created, issues closed, etc.)

If idle but no artifact → stuck, not done.

### External Verification

Never trust agent signals alone. Verify externally before proceeding:
- Agent says "done" → check if issues actually closed
- Agent says "tests pass" → run tests independently
- Agent says "committed" → verify commit exists

Catches: forgot to actually do it, did wrong thing, crashed mid-work.

### Context Exhaustion

When agent signals `--restart`:
1. Current session terminates
2. Progress already persisted externally (issues, files, checkpoints)
3. Fresh session spawns with same workspace
4. Agent continues from saved state

Work survives context limits via external state (issue tracker, filesystem).

### Timeouts

Absolute limits per task/phase. On timeout:
- Preserve state for debugging
- Notify/escalate
- Don't leave resources hanging

### Stuck Recovery

When no heartbeat for too long:
1. **Nudge**: Send interrupt (Ctrl-C) - maybe just needs a poke
2. **Restart**: Kill session, start fresh - clear bad state
3. **Escalate**: Alert human - needs intervention

## MCP Servers

MCP servers extend Claude Code with custom tools via the MCP protocol. Orchestration tools might include:
- Signaling (done, error, restart)
- Events (emit, checkpoint)
- Safeguards (confirm, pause)

MCP tools appear as native tools to Claude - not shell commands.

## Allowed Shell Commands

Alternatively, expose orchestration via shell commands that Claude is permitted to run:

```json
// .claude/settings.local.json
{ "allowed": ["Bash(oj:*)"] }
```

Then `oj done`, `oj emit`, etc. work as bash calls:
```bash
oj done                      # Signal completion
oj done --error "reason"     # Signal failure
oj emit <event> [data]       # Publish event
oj checkpoint "message"      # Record progress
oj confirm "action"          # Request approval (if OTTER_SAFE)
```

Shell commands work from any directory via environment context (`OTTER_TASK`, etc.).

## Hooks

Claude Code hooks intercept execution at various points.

### Session Lifecycle

**SessionStart** hooks run when a session begins:
- Inject initial context (workspace state, task instructions)
- Prime with relevant files or issue details
- Set up session-specific state

**PreCompact** hooks run before context compaction:
- Preserve critical state that shouldn't be lost
- Inject reminders that survive compaction
- Update external systems with progress

### Context Injection

**PreToolUse** hooks inject context before commands run:
- Environment variables
- Current state from external systems
- Dynamic instructions based on context

### Safeguards

**PreToolUse** hooks can block or modify dangerous operations:
- Prevent commands outside allowed directories
- Require confirmation for destructive actions
- Enforce read-only mode

**PostToolUse** hooks verify results:
- Detect error patterns
- Flag suspicious output
- Trigger alerts

### Event Extraction

Hooks parse output to extract implicit events:
- Commit messages → "committed" event
- Test results → "tests passed/failed" event
- Error patterns → "error" event

External systems subscribe to these events.

### Hook Configuration

Orchestrators generate `settings.local.json` in the workspace with task-specific hooks:

```json
{
  "hooks": {
    "Stop": [{"hooks": [{"type": "command", "command": "..."}]}],
    "PreCompact": [{"hooks": [{"type": "command", "command": "wk prime"}]}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "wk prime"}]}]
  }
}
```

**Priming hooks** inject issue tracker context on session start and before compaction, ensuring the agent always has current work state.

**Stop hooks** run when the session ends - update external state, trigger next pipeline phase, notify other systems.

```
on_stop(task):
    completed = issues.list(label=task.label, status="done")
    pipeline.update(task.id, completed=completed)

    if task.merge_queued:
        pipeline.transition(task.id, "pending_merge")
        mergeq.enqueue(task.id)
```

## CLAUDE.md Context

Claude Code reads `CLAUDE.md` files for static context injection:

```
project/
├── CLAUDE.md           # Project-wide: conventions, tools, patterns
├── src/
│   ├── CLAUDE.md       # Module-specific: APIs, patterns
│   └── auth/
│       └── CLAUDE.md   # Component-specific: local context
```

Hierarchical context builds up as Claude navigates the codebase.

### Task-Specific Context via Parent Directory

Create a workspace directory above the source to inject task-specific context:

```
workspace/
├── CLAUDE.md           # Task instructions: "Fix bug #123..."
└── project/            # The actual codebase (clone, worktree, or symlink)
    └── CLAUDE.md       # Project conventions (unchanged)
```

Running Claude Code from `workspace/` injects the task context first, then project context as it descends. The task-specific `CLAUDE.md` can include:
- What to accomplish
- Constraints or focus areas
- How to signal completion
- References to external state (issue ID, pipeline phase)

## Runbooks in Claude Code

Runbooks invoke Claude Code via tasks. Integration points:

**Environment variables** pass runtime context:
```bash
OTTER_TASK="build-auth"        # Task identifier
OTTER_WORKSPACE="build-auth"   # Workspace reference
OTTER_SAFE="true"              # Enable confirmation prompts
```

**Allowed shell commands** via settings (`"allowed": ["Bash(oj:*)"]`):
```bash
oj workspace show
oj pipeline status
oj queue list bugs
```

**MCP tools** for targeted purposes (signaling, safeguards) as native tool calls. Shell for free-form integration.

**Hooks** can read env vars to inject context or enforce safeguards.

**Parent directory CLAUDE.md** provides task instructions (see above).

The runbook orchestrator:
1. Creates workspace and session
2. Sets environment variables
3. Launches Claude Code with prompt
4. Monitors via logs/output
5. Handles completion signals
6. Cleans up session and workspace

## Summary

| Integration | Direction | Purpose |
|-------------|-----------|---------|
| **Sessions** | External → Claude | Isolation, monitoring, control |
| **MCP tools** | Claude → External | Signaling, events, safeguards |
| **Hooks** | Bidirectional | Context injection, interception, extraction |
| **CLAUDE.md** | Static → Claude | Project/module context |
| **Env vars** | External → Claude | Runtime context |

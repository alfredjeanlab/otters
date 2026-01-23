# Claude Code Integration

How Claude Code runs within the oj orchestration system.

## Sessions

Claude Code runs in tmux sessions for:
- **Isolation**: Separate environment per agent
- **Output capture**: Monitor stdout for heartbeat detection
- **Input injection**: Send keystrokes to nudge stuck agents
- **Clean termination**: Kill sessions when stuck or complete

The orchestrator creates, monitors, and destroys sessions. Claude Code doesn't manage its own lifecycle.

## Heartbeat Detection

Detecting if Claude Code is alive or stuck:

| Signal | Source |
|--------|--------|
| Terminal output | New content in stdout (primary) |
| Log file | New entries in `~/.claude/projects/.../` |
| Process status | PID still running |

**Output-based heartbeat works** because Claude Code produces regular output during tool calls. Extended silence typically means stuck, not thinking.

## Completion Detection

Agents are fallible - they forget to signal, get stuck, or exit unexpectedly. Detection is multi-channel:

| Channel | Detects |
|---------|---------|
| Shell commands | Explicit signals (`oj done`) |
| Heartbeat monitoring | Activity/inactivity |
| External events | Git push, issue closed, artifact created |

**Never trust agent signals alone.** Verify externally:
- Agent says "done" → check if issues actually closed
- Agent says "tests pass" → run tests independently

## Stuck Recovery

When no heartbeat for too long:
1. **Nudge**: Send interrupt (Ctrl-C)
2. **Restart**: Kill session, start fresh
3. **Escalate**: Alert human

## Shell Commands

Expose orchestration via allowed shell commands:

```json
// .claude/settings.local.json
{ "allowed": ["Bash(oj:*)"] }
```

Then agents can run:
```bash
oj done                      # Signal completion
oj done --error "reason"     # Signal failure
oj emit <event> [data]       # Publish event
oj checkpoint "message"      # Record progress
```

Shell commands work from any directory via environment context (`OJ_PIPELINE`, etc.).

## Hooks

Claude Code hooks intercept execution:

**SessionStart**: Inject initial context (workspace state, instructions)

**PreCompact**: Preserve critical state before context compaction

**Stop**: Update external state when session ends

Example configuration in `settings.local.json`:
```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "wk prime"}]}],
    "PreCompact": [{"hooks": [{"type": "command", "command": "wk prime"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "oj done"}]}]
  }
}
```

## CLAUDE.md Context

Task-specific context via parent directory:

```
workspace/
├── CLAUDE.md           # Agent instructions: "Fix bug #123..."
└── project/            # The actual codebase (worktree)
    └── CLAUDE.md       # Project conventions
```

Running from `workspace/` injects agent context first, then project context.

## Environment Variables

Runtime context passed to agents:

| Variable | Purpose |
|----------|---------|
| `OJ_PIPELINE` | Pipeline identifier |
| `OJ_PHASE` | Current pipeline phase |
| `OJ_WORKSPACE` | Workspace path |

## Testing

Use [claudeless](https://github.com/anthropics/claudeless) for integration testing. It's a CLI simulator that emulates Claude's interface, TUI, hooks, and permissions without API costs.

```bash
# Run tests with claudeless instead of real claude
PATH="$CLAUDELESS_BIN:$PATH" cargo test
```

Scenario files control responses, making tests deterministic.

## Summary

| Integration | Direction | Purpose |
|-------------|-----------|---------|
| **Sessions** | External → Claude | Isolation, monitoring, control |
| **Shell commands** | Claude → External | Signaling, events |
| **Hooks** | Bidirectional | Context injection, cleanup |
| **CLAUDE.md** | Static → Claude | Agent instructions |
| **Env vars** | External → Claude | Runtime context |
| **Claudeless** | Testing | Deterministic integration tests |

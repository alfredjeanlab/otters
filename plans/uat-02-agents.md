# UAT-02: Agent Spot Check

Manual validation of agent integration functionality.

**Implementation Plans:**
- `mvp-02a-agent-spawn.md` - Basic spawn + workspace + completion signal
- `mvp-02b-session-log.md` - Session log parsing + state detection
- `mvp-02c-agent-config.md` - Action configuration (on_idle, on_exit, on_error)
- `mvp-02d-monitoring.md` - Session monitoring integration
- `mvp-02e-actions.md` - Action handlers (nudge, recover, escalate, etc.)

## Prerequisites

```bash
# Build and install
make install

# Verify claudeless is available (for controlled testing)
which claudeless
# If not installed: cd ../claudeless && cargo install --path .

# Verify tmux is available
tmux -V
```

## 1. Agent Phase Configuration

Verify agent phases parse correctly from runbook.

```bash
cd /tmp/oj-test
mkdir -p .oj/runbooks .oj/scenarios

# First, create the claudeless scenario file
cat > .oj/scenarios/worker.toml << 'EOF'
name = "worker-agent"

[[responses]]
pattern = { type = "any" }

[responses.response]
text = "Hello! I'll complete the task now."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"
tools.Bash.auto_approve = true
EOF

# Then create the runbook that references the scenario
# NOTE: Use absolute path for scenario file in agent.run
cat > .oj/runbooks/agent-test.toml << 'EOF'
[command.test]
args = "<name>"
run = { pipeline = "test" }

[pipeline.test]
inputs = ["name"]

[[pipeline.test.phase]]
name = "init"
run = "echo 'Setting up {name}'"

[[pipeline.test.phase]]
name = "work"
run = { agent = "worker" }

[[pipeline.test.phase]]
name = "done"
run = "echo 'Finished {name}'"

# Agent definitions
# IMPORTANT: The `run` field specifies the command to spawn in tmux.
# For claudeless, use an absolute path to the scenario file.
# The `-p` flag passes the prompt to claudeless (from agent.prompt).
[agent.worker]
run = "claudeless --scenario /tmp/oj-test/.oj/scenarios/worker.toml -p '{prompt}'"
prompt = "Say hello and then run `oj done`"
env = { OJ_PIPELINE = "{pipeline_id}", OJ_PHASE = "work" }
on_idle = { action = "nudge", message = "Keep going. Remember `oj done`." }
on_exit = "escalate"
on_error = "escalate"
EOF
```

**Expected:** Runbook parses without errors.

## 2. Workspace Setup

Verify workspace is prepared correctly before agent spawn.

**Note:** Agent workspaces are created in the global state directory, not the project directory.
Location: `~/.local/state/oj/projects/<hash>/workspaces/<name>/`

```bash
cd /tmp/oj-test
git init
oj daemon start

# Run command that triggers agent phase
oj run test myagent

# Wait for agent phase to start
sleep 2

# Find the workspace in global state dir
# (hash is derived from project path)
WORKSPACE=$(ls -d ~/.local/state/oj/projects/*/workspaces/myagent 2>/dev/null | head -1)
echo "Workspace: $WORKSPACE"

# Check workspace was created
ls -la "$WORKSPACE"

# Verify CLAUDE.md exists with prompt
cat "$WORKSPACE/CLAUDE.md"
# Expected: Contains prompt text and "oj done" instructions

# Verify settings copied
ls -la "$WORKSPACE/.claude/"
cat "$WORKSPACE/.claude/settings.local.json" 2>/dev/null || echo "No settings (OK if project has none)"
```

**Expected:**
- Workspace directory exists in `~/.local/state/oj/projects/<hash>/workspaces/`
- CLAUDE.md contains interpolated prompt
- settings.local.json copied from project (if exists)

## 3. Session Spawning

Verify tmux session is created for agent.

```bash
# Check tmux session exists
tmux list-sessions
# Expected: Session named "oj-myagent" or similar

# Verify session has correct environment
tmux show-environment -t oj-myagent
# Expected: OJ_PIPELINE and OJ_PHASE variables set

# Attach to session to see agent
# (Use Ctrl-B D to detach)
tmux attach -t oj-myagent
```

**Expected:**
- tmux session running
- Claude Code prompt visible (if using real claude)
- Environment variables set

## 4. Completion Signal (`oj done`)

Test completion signaling from agent session.

```bash
# In a separate terminal, simulate agent completion
cd /tmp/oj-test
export OJ_PIPELINE=myagent
export OJ_PHASE=work

# Signal completion
oj done
# Expected: "Done" or similar acknowledgment

# Check pipeline status
oj pipeline list
# Expected: Pipeline advanced to "done" phase or completed

oj pipeline show myagent
# Expected: Shows completed status
```

**Expected:** Pipeline transitions after `oj done`.

## 5. Error Signal (`oj done --error`)

Test error signaling from agent session.

```bash
cd /tmp/oj-test
oj daemon start

# Start a fresh test
oj run test error-test

# Wait for agent phase
sleep 2

# Simulate agent error
export OJ_PIPELINE=error-test
export OJ_PHASE=work
oj done --error "Test error message"

# Check pipeline status
oj pipeline show error-test
# Expected: Shows "failed" phase with error message
```

**Expected:** Pipeline transitions to failed state.

## 6. Session Log Monitoring (Visual)

Observe session monitoring timer behavior.

```bash
# Watch daemon logs for monitoring activity
oj daemon logs -f

# In another terminal, run agent command
oj run test monitor-test

# Observe logs for:
# - "starting session monitor" or SetTimer for session check
# - Session state checks (Working, WaitingForInput)
# - Action triggers (on_idle, on_exit, on_error)
```

**Expected:** Periodic session checks logged (every 10s).

## 7. Idle Detection and Nudge (with claudeless)

Test idle agent detection and nudge action using claudeless.

```bash
# Create claudeless scenario that stops without calling oj done
mkdir -p .oj/scenarios
cat > .oj/scenarios/idle.toml << 'EOF'
name = "idle-agent"

# First response: agent stops at end_turn (no tool calls)
[[responses]]
pattern = { type = "any" }
max_matches = 1

[responses.response]
text = "I think I'm done."
# No tool_calls = stop_reason: end_turn = triggers on_idle

# Second response: after nudge, agent calls oj done
[[responses]]
pattern = { type = "any" }

[responses.response]
text = "Oh right, let me signal completion."

[[responses.response.tool_calls]]
tool = "Bash"
input = { command = "oj done" }

[tool_execution]
mode = "live"
tools.Bash.auto_approve = true
EOF

# Create runbook that uses the idle scenario
cat > .oj/runbooks/idle-test.toml << 'EOF'
[command.idle]
args = "<name>"
run = { pipeline = "idle" }

[pipeline.idle]
inputs = ["name"]

[[pipeline.idle.phase]]
name = "work"
run = { agent = "idle-worker" }

[[pipeline.idle.phase]]
name = "done"
run = "echo 'Completed {name}'"

# Agent with on_idle configured for nudge
[agent.idle-worker]
run = "claudeless --scenario /tmp/oj-test/.oj/scenarios/idle.toml -p '{prompt}'"
prompt = "Complete the task"
env = { OJ_PIPELINE = "{pipeline_id}" }
on_idle = { action = "nudge", message = "Keep going. Remember to call `oj done`." }
on_exit = "escalate"
EOF

# Restart daemon to load new runbook
oj daemon stop && oj daemon start

# Run test
oj run idle idle-test

# Watch daemon logs
oj daemon logs -f

# Expected sequence:
# 1. Agent spawned
# 2. Session log shows stop_reason: end_turn (WaitingForInput)
# 3. on_idle triggers: nudge message sent
# 4. Agent resumes and calls oj done
# 5. Pipeline completes
```

**Expected:** Idle detection triggers nudge, pipeline completes.

## 8. Session Exit Detection

Test handling of agent process exit.

```bash
cd /tmp/oj-test
oj daemon start

# Run agent command
oj run test exit-test

# Wait for agent phase to start
sleep 2

# Force kill the tmux session
tmux kill-session -t oj-exit-test

# Check pipeline status
sleep 1
oj pipeline show exit-test
# Expected: Shows failed or completed depending on exit detection
```

**Expected:** Pipeline detects session exit.

## 9. Cleanup

```bash
cd /tmp/oj-test
oj daemon stop

# Kill any remaining tmux sessions
tmux kill-server 2>/dev/null || true

# Clean up test directory
cd /
rm -rf /tmp/oj-test
```

## Status

- [ ] Agent Phase Configuration
- [ ] Workspace Setup
- [ ] Session Spawning
- [ ] Completion Signal
- [ ] Error Signal
- [ ] Session Log Monitoring
- [ ] Idle Detection and Nudge
- [ ] Session Exit Detection
- [ ] Cleanup

## Bugs Found

Document any bugs discovered during testing:

<!--
- [ ] Bug description
  - Steps to reproduce
  - Expected behavior
  - Actual behavior
  - **Root cause**: (once identified)
  - **Fix**: (once fixed)
-->

## Notes

### Claudeless Integration

For deterministic testing, use [claudeless](https://github.com/anthropics/claudeless) instead of real Claude Code:

```bash
# Install claudeless
cd /path/to/claudeless
cargo install --path .

# Create scenario file
cat > scenario.toml << 'EOF'
[scenario]
name = "test"

[[responses]]
message = "Hello! I'll run oj done now."
tools = [{ bash = "oj done" }]
EOF

# Run tests with claudeless (scenario path embedded in agent run field)
# [agent.worker]
# run = "claudeless --scenario scenario.toml --print '{prompt}'"
oj run test foo
```

### Environment Variables

Agents receive these environment variables:

| Variable | Description |
|----------|-------------|
| `OJ_PIPELINE` | Pipeline identifier |
| `OJ_PHASE` | Current phase name |
| `OJ_WORKSPACE` | Workspace path |

### Tmux Session Naming

Sessions are named `oj-{pipeline_name}` for easy identification:
- `oj-myagent` for pipeline "myagent"
- Use `tmux attach -t oj-myagent` to watch agent work

### Recovery Commands

When a pipeline is escalated (waiting for human intervention):

```bash
oj session attach <id>         # Attach to tmux to see what's happening
oj session send <id> "message" # Send follow-up message to agent
oj pipeline resume <id>        # Resume monitoring after intervention
oj pipeline fail <id>          # Manually mark pipeline as failed
```

### Action Configuration

Agents support configurable actions for different scenarios:

```toml
[agent.worker]
run = "claude -p"
prompt = "Complete the task."
on_idle = { action = "nudge", message = "Keep going. Remember oj done." }
on_exit = "escalate"           # or "done", "recover", "fail", "restart"
on_error = "escalate"          # or per-error: [[agent.worker.on_error]]
```

Available actions: `nudge`, `done`, `fail`, `restart`, `recover`, `escalate`

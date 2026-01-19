# Manual Haiku Validation Runbook

Manual testing procedures for validating `oj` against real Claude (haiku model). Run these tests after major changes to verify the simulator accurately models real behavior.

## Prerequisites

- Claude API key configured (`ANTHROPIC_API_KEY` or `claude` CLI authenticated)
- `oj` binary built: `cargo build`
- Clean test directory available
- tmux installed

## Setup

```bash
# Create isolated test directory
export TEST_DIR=$(mktemp -d)
cd $TEST_DIR

# Initialize git repo
git init
git config user.email "test@test.com"
git config user.name "Test"
echo "# Haiku Validation" > README.md
git add README.md
git commit -m "Initial commit"

# Create operations directory
mkdir -p .build/operations

# Path to oj binary (adjust as needed)
export OJ="/path/to/otters/target/debug/oj"
```

---

## Test 1: Basic Pipeline Creation

**Goal:** Verify `oj run build` spawns a real Claude session.

```bash
$OJ run build haiku-test "Create a simple hello.txt file with the text 'Hello from Claude'"
```

**Expected:**
- Command exits 0
- Output shows "Started build pipeline 'haiku-test'"
- Output shows tmux session name

**Verify:**
```bash
tmux list-sessions | grep oj-haiku-test
tmux attach -t oj-haiku-test-init
```

**Common Issues:**

| Symptom | Cause | Resolution |
|---------|-------|------------|
| "command not found: claude" | Claude CLI not installed | Install Claude CLI |
| Session exits immediately | API key invalid | Check `ANTHROPIC_API_KEY` |
| Claude shows error about model | Wrong model specified | Check default model in code |

---

## Test 2: Claude Receives Prompt

**Goal:** Verify Claude receives the prompt from CLAUDE.md.

```bash
$OJ run build prompt-test "List three prime numbers and save them to primes.txt"
```

**Verify:**
```bash
cat .worktrees/build-prompt-test/CLAUDE.md
tmux attach -t oj-build-prompt-test-init
```

**Expected:**
- CLAUDE.md contains "List three prime numbers"
- Claude's response shows it understood the task

**Common Issues:**

| Symptom | Cause | Resolution |
|---------|-------|------------|
| Claude asks "what would you like?" | CLAUDE.md not passed to session | Check session spawn code |
| Claude does unrelated task | CLAUDE.md content wrong | Check generation function |

---

## Test 3: Signal Done Advances Phase

**Goal:** Verify `oj done` signals Claude to complete and advances the pipeline.

**Note:** Claude does NOT automatically run `oj done`. You must either tell Claude to run it or run it manually from the workspace.

```bash
$OJ run build signal-test "Create a file called done.txt"

# Wait for Claude (or tell it to create the file)
tmux attach -t oj-build-signal-test-init

# From another terminal, signal done manually
cd .worktrees/build-signal-test
export OTTER_PIPELINE="build-signal-test"
$OJ done
```

**Verify:**
```bash
cat .build/operations/pipelines/build-signal-test.json | jq .phase
```

**Expected:**
- `oj done` exits 0
- Phase is no longer "init"

**Common Issues:**

| Symptom | Cause | Resolution |
|---------|-------|------------|
| "Could not detect workspace" | Wrong directory or missing env var | Ensure OTTER_PIPELINE is set |
| Phase still "init" | Signal not processed | Check Engine signal routing |
| Claude keeps working after done | Session not terminated | Check session cleanup |

---

## Test 4: Daemon Processes Sessions

**Goal:** Verify daemon detects session state and processes pipelines.

```bash
$OJ run build daemon-test "Create README.md with project description"
$OJ daemon --once
```

**Expected:**
- Daemon starts and logs intervals
- Daemon completes without error

**Common Issues:**

| Symptom | Cause | Resolution |
|---------|-------|------------|
| "Poll sessions error" | Tmux not running | Start tmux server |
| Daemon hangs | Infinite loop | Check termination condition |

---

## Test 5: Full Lifecycle

**Goal:** Run a pipeline from creation to completion.

```bash
$OJ run build lifecycle "Create hello.py that prints 'Hello World', then run oj done"
tmux attach -t oj-build-lifecycle-init
```

**Wait for Claude to:**
1. Create hello.py
2. Run `oj done` (or do it manually if Claude doesn't)

**Verify:**
```bash
cat .build/operations/pipelines/build-lifecycle.json | jq .
cat .worktrees/build-lifecycle/hello.py
```

**Expected:**
- Pipeline reaches "done" or next phase
- File exists with correct content

**Common Issues:**

| Symptom | Cause | Resolution |
|---------|-------|------------|
| Claude doesn't run `oj done` | Claude ignores CLAUDE.md instructions | Update CLAUDE.md format |
| Phase stuck at "init" | Signal not sent or not processed | Manually run `oj done` |
| Claude creates wrong file | Prompt unclear | Improve prompt wording |

---

## Test 6: Error Handling

**Goal:** Verify `oj done --error` properly fails the pipeline.

```bash
$OJ run build error-test "Attempt something"

cd .worktrees/build-error-test
export OTTER_PIPELINE="build-error-test"
$OJ done --error "Intentional test failure"
```

**Verify:**
```bash
cd $TEST_DIR
cat .build/operations/pipelines/build-error-test.json | jq .phase
```

**Expected:**
- Phase shows "failed" or error state
- Error message is recorded

---

## Test 7: Checkpoint Behavior

**Goal:** Verify `oj checkpoint` saves progress without advancing phase.

```bash
$OJ run build checkpoint-test "Work on a task"

cd .worktrees/build-checkpoint-test
export OTTER_PIPELINE="build-checkpoint-test"
$OJ checkpoint
```

**Verify:**
```bash
cd $TEST_DIR
cat .build/operations/pipelines/build-checkpoint-test.json | jq '{phase, last_checkpoint}'
```

**Expected:**
- Phase remains "init"
- last_checkpoint timestamp is updated

---

## Test 8: Concurrent Pipelines

**Goal:** Verify multiple pipelines run independently.

```bash
$OJ run build concurrent-1 "First task"
$OJ run build concurrent-2 "Second task"

tmux list-sessions | grep oj-
```

**Verify:**
- Both sessions exist
- Signaling done on one doesn't affect the other

---

## Test 9: Session Recovery

**Goal:** Verify daemon detects and handles dead sessions.

```bash
$OJ run build recovery-test "Create something"

# Kill the session manually
tmux kill-session -t oj-build-recovery-test-init

# Run daemon
$OJ daemon --once
```

**Verify:**
```bash
cat .build/operations/pipelines/build-recovery-test.json | jq .
```

**Expected:**
- Daemon detects missing session
- State reflects session death (stuck or needs attention)

---

## Comparison: Simulator vs Real

After running tests with real Claude, compare behavior:

| Behavior | Claudeless | Real Claude | Match? |
|----------|------------|-------------|--------|
| Session spawns | Yes | ? | |
| Receives prompt | Yes | ? | |
| Creates files | simulated | ? | |
| Runs oj done | scripted | ? | |
| Error handling | injected | ? | |

Document any discrepancies for simulator improvement.

---

## Troubleshooting

### Claude Session Exits Immediately

```bash
cd $TEST_DIR/.worktrees/build-test
claude -p "Say hello"
```

### API Errors

```bash
echo $ANTHROPIC_API_KEY
claude -p "test" --model claude-haiku-3
```

### Tmux Issues

```bash
tmux list-sessions
tmux new-session -d -s test-session
tmux kill-session -t test-session
```

### State File Issues

```bash
find .build/operations -name "*.json" -exec cat {} \;
```

---

## Additional QA Considerations

### Timing Validation

During manual tests, note timing behavior:
- How long until Claude responds?
- How quickly does daemon detect phase changes?
- What's the typical full lifecycle duration?

### Resource Usage

Monitor during tests:
```bash
# In separate terminal
watch -n 1 'ps aux | grep -E "(oj|claude)" | head -10'
```

### Log Inspection

Check for unexpected warnings/errors:
```bash
# If oj has log output
$OJ daemon --once 2>&1 | tee daemon.log
grep -i "error\|warn" daemon.log
```

### Edge Case Exploration

Try these during manual testing:
- Very long prompts (1000+ chars)
- Special characters in pipeline names
- Rapid successive commands
- Interrupting Claude mid-task (Ctrl+C in session)

---

## Cleanup

```bash
tmux list-sessions | grep oj- | cut -d: -f1 | xargs -I{} tmux kill-session -t {}
rm -rf $TEST_DIR
```

---

## Success Criteria

- [ ] Runbook is readable and actionable
- [ ] Commands can be copy-pasted
- [ ] All 9 tests documented with expected outcomes
- [ ] Common issues table helps debugging
- [ ] Comparison table documents simulator accuracy

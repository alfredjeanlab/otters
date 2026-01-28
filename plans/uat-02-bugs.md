# UAT-02 Bugs

Issues discovered during UAT-02 agent spot check.

## Bug 1: Agent Session Spawn Silently Fails

**Severity:** Critical - Blocks all agent functionality

**Status:** ✅ Fixed

**Root Cause:**
Multiple issues in the agent spawn path:

1. **Missing OJ_PROJECT_ROOT env var** - Agents run in workspaces (under `~/.local/state/oj/...`) which are not the project root. When `oj done` runs, it needs to find the daemon socket, which requires knowing the project root. Fixed by:
   - Adding `OJ_PROJECT_ROOT` env var to `find_project_root()` in `crates/cli/src/client.rs`
   - Setting `OJ_PROJECT_ROOT` in `build_spawn_effects()` in `crates/engine/src/spawn.rs`

2. **Missing OJ_SOCKET_DIR env var** - For test isolation, tests use a custom socket directory via `OJ_SOCKET_DIR`. This wasn't being passed to spawned agents. Fixed by:
   - Inheriting `OJ_SOCKET_DIR` in `build_spawn_effects()` if set

3. **AgentDef field rename** - The `command` field was renamed to `run` but some code still referenced `command`.

**Files Changed:**
- `crates/cli/src/client.rs` - Check `OJ_PROJECT_ROOT` env var in `find_project_root()`
- `crates/engine/src/spawn.rs` - Set `OJ_PROJECT_ROOT` and inherit `OJ_SOCKET_DIR`
- `docs/03-interface/CLI.md` - Document `OJ_PROJECT_ROOT` env var

**Test Added:**
- `tests/specs/agent/spawn.rs` - End-to-end test that:
  - Creates a project with agent runbook
  - Starts daemon
  - Runs pipeline that spawns agent
  - Agent (claudeless) calls `oj done`
  - Verifies pipeline completes

---

## Bug 2: Workspace Location Unexpected

**Severity:** Low - Documentation/UX issue

**Status:** ✅ Fixed - Documentation updated

**Symptoms:**
- UAT plan expects `worktrees/` in project directory
- Actual location is `~/.local/state/oj/projects/{hash}/workspaces/`

**Impact:**
- UAT validation steps fail to find expected paths
- Users may not know where to look for agent workspaces

**Resolution:**
- ✅ Updated UAT-02 plan to use correct global state paths
- ✅ Added notes to example runbooks (build.toml, build.minimal.toml, bugfix.toml)
  clarifying workspace location and implementation status
- Consider adding `oj workspace list` command (future enhancement)
- Consider symlinking workspaces into project directory (future enhancement)

---

## Bug 3: Session Monitoring Timers Not Firing

**Severity:** Medium - Blocks idle/exit detection

**Status:** 🔴 Open

**Symptoms:**
- `set_timer` effects are logged with `timer_id = "session:*:check"` and `duration_ms = 10000`
- Timer firing events are never logged
- Pipelines remain in "Running" status even after tmux sessions exit
- Idle detection (`on_idle`) never triggers

**Observed Behavior:**
1. Agent session spawns successfully
2. Timer is set via `Effect::SetTimer` with 10s duration
3. Agent completes work (claudeless responds with no tool calls)
4. tmux session exits
5. No timer firing event logged
6. Pipeline stays in "Running" status indefinitely

**Expected Behavior:**
1. Timer should fire after 10 seconds
2. Session check should detect session is dead
3. `on_exit` action should trigger (escalate in test case)

**Potential Root Cause:**
The timer infrastructure may not be fully wired up in the daemon runtime. The `Effect::SetTimer` is being produced but the corresponding timer firing mechanism may be missing or not connected.

**Investigation Needed:**
- Check `ojd` timer handling code
- Verify timer events are being processed
- May need to add timer expiry handling in daemon event loop

---

## Test Results

| Test | Status |
|------|--------|
| 1. Agent Phase Configuration | ✅ Pass |
| 2. Workspace Setup | ✅ Pass |
| 3. Session Spawning | ✅ Pass |
| 4. Completion Signal | ✅ Pass |
| 5. Error Signal | ✅ Pass |
| 6. Session Monitoring | ⚠️ Partial - timers set but don't fire |
| 7. Idle Detection | ❌ Blocked by Bug 3 |
| 8. Session Exit Detection | ❌ Blocked by Bug 3 |
| 9. Cleanup | ✅ Pass |

## Summary

**Core functionality working:**
- Agent spawn, workspace setup, and completion signaling (`oj done`, `oj done --error`) all work correctly
- The critical spawn bug (Bug 1) was fixed by passing OJ_PROJECT_ROOT and OJ_SOCKET_DIR env vars

**Blocked functionality:**
- Session monitoring (idle/exit detection) is blocked by Bug 3 - timers don't fire
- Without working timers, agents that exit or idle without calling `oj done` won't be detected

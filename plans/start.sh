#!/usr/bin/env bash
set -euo pipefail

# __PLAN_MVP_01__="$(cat <<'EOF'
# Plan an architecturally correct MVP to get a [command.build] implementation working.

# Based on the runbook:

#     docs/10-runbooks/build.minimal.toml

# You must implement [command.*], [pipeline.*], [agent.*] and [events.*].

# Do not implement [strategy], [lock], [semaphore], [worker], [queue] nor [guard].

# See docs/01-concepts/EXECUTION.md
# See docs/01-concepts/RUNBOOKS.md
# See docs/02-integrations/CLAUDE_CODE.md
# See docs/02-integrations/ISSUE_TRACKING.md
# See docs/03-interface/CLI.md
# See docs/03-interface/EVENTS.md
# See docs/04-architecture/01-overview.md
# See docs/04-architecture/02-effects.md
# See docs/04-architecture/04-storage.md
# See docs/04-architecture/05-adapters.md
# See docs/10-runbooks/ for comparison.

# Acceptance Criteria:
# - You MUST parse a runbook toml file.
# - You MUST parse basic `[command].args = "..."` syntax.
# - You MUST emit events and execute event handlers.
# - You MUST handle "shell" and { agent = "name" } and { pipeline = "name" } run commands

# EOF
# )"
# v0 feature mvp-01-build "$__PLAN_MVP__"
# v0 feature plans/mvp-02-argparse.md  # --after mvp-01-build
# v0 feature plans/mvp-03-daemon.md --after mvp-02-argparse
# v0 feature plans/mvp-04-execute.md --after mvp-03-daemon
# v0 feature plans/mvp-05-pipeline.md --after mvp-04-execute
# v0 feature plans/daemon-01-robustness.md # --after mvp-05-pipeline
# v0 feature plans/uat-01-spot-check.md # --after daemon-01-robustness
# v0 feature plans/uat-02-bugs.md # --after uat-01-spot-check
# v0 feature plans/uat-03-validate-args.md # --after uat-02-bugs

# MVP-02: Agent Integration
# v0 build mvp-02-prep "review the upcoming plans mvp-02 and uat-2, and put together basic testing tools in tests/specs/[prelude.rs] to prepare for agent and claude testing with claudeless. Review ~/Developer/claudeless and add a new tests/specs/claudeless.rs. Add basic test/specs/CLAUDELESS.md usage documentation with advice about how to fluently write specs tests using claudless using any local helpers we want to add"
# v0 build mvp-02a-specs "Write the tests/specs we expect to pass after mvp-02a (but not including mvp-02b), and mark them as skipped with a todo(implement)" --after mvp-02-prep
# v0 build plans/mvp-02a-agent-spawn.md --after mvp-02a-specs
# v0 build mvp-02a-cleanup "Review the work done for mvp-02a-* and identify if it is clean, complete or has gaps. Cleanup tech debt, DRY-up the code, implementing missing features/gaps, and add / validate tests and functionality" --after mvp-02a-agent-spawn
# v0 build plans/mvp-02b-session-log.md --after mvp-02a-cleanup
# v0 build mvp-02b-cleanup "Review mvp-02b implementation. Verify SessionLogWatcher tests pass, clean up code, ensure session_log module is properly exported" --after mvp-02b-session-log

v0 build plans/mvp-02c-agent-config.md # --after mvp-02b-cleanup
v0 build mvp-02c-cleanup "Review mvp-02c implementation. Verify ActionConfig/AgentAction parsing works, run agent::config tests, clean up code" --after mvp-02c-agent-config

v0 build plans/mvp-02d-monitoring.md --after mvp-02c-cleanup
v0 build mvp-02d-cleanup "Review mvp-02d implementation. Verify session monitoring integrates with event loop, run agent::monitoring tests, clean up code" --after mvp-02d-monitoring

v0 build plans/mvp-02e-actions.md --after mvp-02d-cleanup
v0 build mvp-02e-cleanup "Review mvp-02e implementation. Verify all actions work (nudge/done/fail/restart/recover/escalate), run agent::actions tests, clean up code" --after mvp-02e-actions

# v0 build plans/uat-02-agents.md --after mvp-02e-cleanup

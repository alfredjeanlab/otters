# Implementation Epics

This document breaks down the full system implementation into iterative phases. Each epic builds on the previous, allowing architecture validation and course correction as we progress.

---
title: "Epic 1: MVP - Replace Bash Scripts"
---

Replace the existing bash scripts (`feature`, `bugfix`, `mergeq`, `merge`, `tree`) with a Rust implementation that provides equivalent functionality using simplified versions of the architecture. This validates our core design patterns before building the full system.

The MVP prioritizes getting something working end-to-end over architectural purity. We use simple JSON file storage instead of a WAL, basic tmux session management, and hardcoded pipelines instead of full runbook parsing. This lets us validate the workspace/session model and Claude Code integration quickly.

## In Scope

- **CLI skeleton**: `oj run`, `oj pipeline`, `oj workspace`, `oj session` commands
- **Workspace management**: Git worktree creation/deletion, settings sync, CLAUDE.md generation
- **Session management**: Tmux spawn/kill/send/capture, basic heartbeat via output monitoring
- **Simple state**: JSON files per operation in `.build/operations/<name>/state.json`
- **Hardcoded pipelines**: `build` (plan → decompose → execute → merge) and `bugfix` (setup → fix → merge)
- **Merge queue**: Simple file-based queue, single daemon processing merges sequentially
- **Basic signaling**: `oj done`, `oj done --error`, environment variables for context
- **Adapter traits**: Define `SessionAdapter`, `RepoAdapter` traits with tmux/git implementations
- **Test infrastructure**: FakeClock, fake adapters, basic contract tests

## Out of Scope

- Full WAL with durability guarantees (use JSON files)
- Multi-machine sync
- Runbook TOML parsing (hardcode the two main workflows)
- Locks with heartbeats (use simple file locks)
- Semaphores (limit to single concurrent agent)
- Guards and strategies as first-class primitives
- Events system beyond basic logging
- Template engine (use string interpolation)
- Watchers, scanners, cron jobs

---
title: "Epic 2: Core State Machines"
---

Implement the pure functional core with explicit state machines for Pipeline, Queue, and Task. All state transitions are pure functions returning `(NewState, Vec<Effect>)`. This establishes the foundation for high test coverage and deterministic behavior.

The functional core has zero external dependencies and is 100% unit testable. Effects are data structures describing side effects, not actual I/O. This separation enables property-based testing of state machine invariants and makes the system easier to reason about.

## In Scope

- **Pipeline state machine**: Init → Blocked → Running(phase) → Done/Failed transitions
- **Queue state machine**: Push, take, complete, fail operations with visibility timeout
- **Task state machine**: Pending → Running → Stuck → Done with heartbeat logic
- **Effect types**: Emit, Spawn, Send, Kill, WorktreeAdd, etc. as enum variants
- **Injectable time**: `Clock` trait with `SystemClock` and `FakeClock` implementations
- **Injectable IDs**: `IdGen` trait with `UuidGen` and `SequentialGen` implementations
- **Checkpoint support**: Pipeline checkpoints for recovery
- **Property-based tests**: Verify state machine invariants with proptest
- **Parametrized tests**: Edge cases with yare

## Out of Scope

- Effect execution (that's the engine layer)
- Persistence (state machines operate on in-memory state)
- Cross-primitive coordination (locks, semaphores, guards)
- Runbook-defined pipelines (still hardcoded)

---
title: "Epic 3: Engine & Execution Loop"
---

Build the imperative shell that executes effects from the functional core. The engine loads state, invokes core logic, executes effects via adapters, and persists new state. Effect failures feed back as events to drive recovery.

This epic connects the pure core to real I/O while maintaining testability. Integration tests use fake adapters to verify the execution loop without actual tmux/git calls. The engine handles the feedback loop where effect results become events for further state transitions.

## In Scope

- **Executor**: Main loop that processes commands through the core
- **Effect execution**: Map each Effect variant to adapter calls
- **Feedback loop**: Effect failures become events that drive state machine transitions
- **Adapter composition**: `Adapters` trait aggregating session, repo, agent, issue adapters
- **Fake adapters**: In-memory implementations with call recording and configurable responses
- **Contract tests**: Verify both production and fake adapters implement traits correctly
- **Error handling**: Graceful failure without state corruption
- **Recovery actions**: Nudge, restart, escalate chains for stuck tasks

## Out of Scope

- Scheduler (cron, event-driven wakes)
- Persistent storage (still using JSON files)
- Multi-machine coordination

---
title: "Epic 4: Events & Notifications"
---

Implement the events system for loose coupling and observability. Events are emitted on state changes and can wake workers, trigger notifications, and feed monitoring. This is foundational infrastructure that later epics build upon.

Events enable reactive behavior - workers wake on specific events instead of polling queues. The event bus routes events to subscribers. Notifications surface important events to users via macOS integration, providing immediate feedback on build completion, merge status, and escalations.

## In Scope

- **Event types**: System events (pipeline:*, task:*, worker:*) and custom events
- **Event bus**: Route events to matching subscribers
- **Worker wake**: Subscribe workers to events (wake_on patterns)
- **Event logging**: Audit trail of all events
- **macOS notifications**: osascript integration for desktop alerts
- **Notification configuration**: Which events become notifications
- **Escalation alerts**: Critical events with sound
- **NotifyAdapter trait**: Abstraction for notification delivery

## Out of Scope

- External event sinks (webhooks, etc.)
- Event replay for debugging
- Event persistence (events are ephemeral until WAL epic)

---
title: "Epic 5: Coordination Primitives"
---

Implement Lock, Semaphore, and Guard as first-class primitives with proper heartbeat-based stale detection. These enable safe concurrent access to shared resources like the main branch and agent slots.

Locks provide exclusive access with automatic reclaim of stale holders. Semaphores limit concurrent agent sessions. Guards are composable conditions that gate phase transitions. All follow the pure functional pattern with explicit state and effect generation. Guards can now use event-driven wake instead of polling.

## In Scope

- **Lock state machine**: Free → Held transitions with heartbeat refresh and stale reclaim
- **Semaphore state machine**: Multi-holder with weighted slots and orphan reclaim
- **Guard evaluation**: Condition types (LockFree, IssuesComplete, BranchExists, etc.)
- **Composite guards**: All, Any, Not combinators
- **Guard executor**: Gather inputs via adapters, call pure evaluation
- **Event-driven guard wake**: Wait for events instead of polling conditions
- **Coordination manager**: Unified interface for lock/semaphore operations
- **Periodic maintenance**: Background task to reclaim stale resources
- **Phase gating**: Pre/post guards on pipeline phases

## Out of Scope

- Strategy chains (fallback approaches)
- Cross-runbook coordination

---
title: "Epic 5a: Claude Simulator Core"
---

Create a test crate that emulates the `claude` CLI for integration testing. This epic focuses on the core simulation: CLI interface, response scripting, failure injection, and output capture.

The simulator provides a controllable test double that responds to the same CLI interface as real Claude. It supports scripted responses, configurable delays, and failure injection. This enables deterministic integration testing without API costs or flakiness.

## In Scope

- **New crate**: `crates/claude-sim` with binary `claude-sim` that shadows `claude` in test PATH
- **CLI interface**: Parse and handle flags that oj uses (`--model`, `--print`, `--output-format`, etc.)
- **Response scripting**: JSON/TOML files defining scripted responses per prompt pattern
- **Failure injection**: Simulate error modes:
  - Network unreachable / connection timeout
  - Authentication errors (invalid API key, expired token)
  - Rate limiting (429 responses)
  - Out of credits / billing errors
  - Partial responses / stream interruption
  - Malformed JSON responses
- **Output formats**: Support `--output-format json`, `--output-format stream-json`, text output
- **Output capture**: Record all interactions for test assertions
- **Test helpers**: Rust API for configuring simulator behavior in tests
- **Basic documentation**: Usage guide with examples for common test scenarios

## Out of Scope

- State directory emulation (~/.claude/*) - that's Epic 5b
- Hook simulation - that's Epic 5b
- Actual LLM responses (use canned/scripted responses only)
- Full Claude Code feature parity (focus on features oj uses)

---
title: "Epic 5b: Claude Simulator State"
---

Extend the Claude simulator to emulate Claude Code's state management: the ~/.claude directory structure, permission modes, hook protocols, and session state. This makes the simulator suitable for testing oj's integration with Claude's file-based interfaces.

Claude Code maintains state in ~/.claude including todos, project context, and plans. It also uses hooks for bidirectional communication. Accurately emulating these interfaces enables testing of oj's Claude integration without running real Claude.

## In Scope

- **Directory emulation**: Create and manage simulated ~/.claude structure:
  - `~/.claude/todos` - Todo list state
  - `~/.claude/projects/<project-hash>/` - Per-project context and settings
  - `~/.claude/plans/` - Saved plans
  - `~/.claude/settings.json` - Global settings
- **Permission modes**: Match real Claude Code file permissions (readable by user, etc.)
- **Hook simulation**: Emit and receive hooks that oj can interact with:
  - Pre/post tool execution hooks
  - Notification hooks
  - Permission request hooks
- **Fake time integration**: Configurable delays without wall-clock time (integrate with FakeClock)
- **Session state**: Track conversation state across multi-turn interactions within a session
- **State inspection API**: Test helpers to query/assert simulator state
- **State reset**: Clean slate between tests

## Out of Scope

- MCP server emulation (out of scope for oj)
- IDE integration features
- Actual persistence across test runs (state is ephemeral)

---
title: "Epic 5c: Claude Simulator TUI"
---

Implement a simplified terminal user interface that visually matches Claude Code's TUI rendering. Built with iocraft or crossterm, it responds to keyboard input and shortcuts in the same way as real Claude Code, enabling visual and interaction testing.

The TUI simulator enables testing of oj's integration with Claude's interactive terminal mode. While significantly simplified (no actual LLM processing), it accurately renders the visual layout and responds to the keyboard shortcuts that oj might send or that users interact with during supervised operation.

## In Scope

- **TUI framework**: Build with iocraft or crossterm for terminal rendering
- **Visual fidelity**: Match Claude Code's layout:
  - Input prompt area
  - Response streaming area
  - Status bar (model, token count, etc.)
  - Tool use display blocks
  - Permission prompts
- **Keyboard handling**: Respond to shortcuts that oj or users use:
  - Ctrl+C for interrupt/cancel
  - Ctrl+D for exit
  - Enter for input submission
  - Arrow keys for history navigation
  - Escape for mode switching
- **Permission dialogs**: Render and respond to tool permission prompts (accept/reject)
- **Streaming simulation**: Simulate token-by-token response rendering with configurable speed
- **Integration with simulator core**: TUI mode activated via `claude-sim --tui` or when stdin is a TTY
- **Screenshot capture**: Programmatic capture of terminal state for visual regression testing

## TUI Behaviors Useful for Testing

- **Permission prompt flow**: Test that oj can detect and respond to permission requests
- **Interrupt handling**: Verify oj correctly sends Ctrl+C and detects interruption
- **Output parsing**: Ensure oj can parse streamed output as it renders
- **Session lifecycle**: Test attach/detach behavior with tmux integration
- **Error display**: Verify error states render correctly and are detectable
- **Progress indication**: Test detection of "thinking" vs "responding" states

## Out of Scope

- Full visual parity (focus on layout, not pixel-perfect styling)
- Mouse input handling
- Clipboard integration
- Syntax highlighting accuracy
- Window resize handling (use fixed dimensions)

---
title: "Epic 5d: Simulator Validation"
---

Validate the Claude simulator against real Claude Code behavior and Anthropic documentation. Ensure the simulator accurately models CLI flags, output formats, error conditions, and hook protocols before relying on it for integration testing.

This epic bridges the gap between a working simulator and a trustworthy one. By comparing simulator behavior against real Claude Code and official docs, we catch discrepancies early. Improved test coverage ensures the simulator remains accurate as Claude Code evolves.

## In Scope

- **CLI flag audit**: Compare `claude-sim --help` against `claude --help`; verify all flags oj uses are implemented
- **Output format validation**: Capture real Claude Code output samples; verify simulator matches format exactly
- **Hook protocol verification**: Test hook emission against Claude Code documentation; verify timing and payload structure
- **State directory validation**: Compare ~/.claude structure against real Claude Code; verify paths and permissions match
- **Error behavior comparison**: Trigger real error conditions (invalid API key, etc.) and compare against simulator
- **Documentation review**: Cross-reference simulator behavior with Anthropic's Claude Code docs and API reference
- **Discrepancy fixes**: Update simulator to match observed real behavior
- **Unit test expansion**: Add tests for every CLI flag and output format variation
- **Integration test suite**: End-to-end tests that would pass with both real Claude and simulator
- **Accuracy report**: Document known limitations and intentional simplifications

## Out of Scope

- Testing actual LLM response quality (only testing CLI/protocol behavior)
- Matching internal implementation details (only external behavior)
- Supporting Claude Code features oj doesn't use

---
title: "Epic 5d4: Scenario Format Enhancement"
---

Enhance the TOML/JSON scenario format to support all configuration needed for integration testing. Scenarios control simulator behavior including model selection, timing, user identity, and trust settings.

A rich scenario format enables deterministic, repeatable tests that exercise specific behaviors. Default values match real Claude Code behavior, while overrides enable testing edge cases. This is foundational for the state directory and TUI epics that follow.

## In Scope

- **Core scenario fields**:
  - `default_model`: Model to report (default: "claude-sonnet-4-20250514", overridden by `--model` flag)
  - `claude_version`: Version string (default: "2.1.12")
  - `user_name`: Display name (default: "Alfred")
  - `launch_timestamp`: Session start time (default: current time, enables deterministic tests)
  - `working_directory`: Simulated cwd (default: actual cwd)
  - `trusted`: Whether directory is trusted (default: true, false shows trust prompt)
- **Response configuration**:
  - `default_response`: Fallback response text
  - `responses`: Map of prompt patterns to specific responses
  - `response_delay_ms`: Simulated thinking time
- **Tool execution settings**:
  - `tool_execution.mode`: "simulated" | "passthrough" | "record"
  - `tool_execution.tools.<ToolName>.auto_approve`: Skip permission prompt
  - `tool_execution.tools.<ToolName>.result`: Canned result for tool
- **Permission mode**: `permission_mode`: "default" | "plan" | "full-auto" | "accept-edits"
- **Session identity**:
  - `session_id`: Fixed UUID for deterministic file paths (default: random)
  - `project_path`: Override project path normalization
- **Scenario validation**: Error on unknown fields, type mismatches
- **Documentation**: Scenario format reference with examples

## Out of Scope

- Hot-reloading scenarios mid-session
- Scenario inheritance/composition
- External scenario repositories

---
title: "Epic 5d5: State Directory Implementation"
---

Implement correct ~/.claude directory structure so claude-sim produces the same state files as real Claude Code. This enables testing of oj's integration with Claude's file-based state.

The ~/.claude directory is Claude Code's persistent state. Accurate emulation enables testing without running real Claude. Tests verify file paths, naming conventions, and content formats match exactly.

## In Scope

- **projects/ directory**:
  - Path normalization: replace `/` and `.` with `-`
  - `sessions-index.json`: version, entries array with sessionId, fullPath, fileMtime, firstPrompt, messageCount, created, modified, gitBranch, projectPath, isSidechain
  - Session JSONL files: `{uuid}.jsonl` with line types (user, assistant, queue-operation, summary, file-history-snapshot)
- **todos/ directory**:
  - File naming: `{sessionId}-agent-{sessionId}.json`
  - Content: JSON array of `{content, status, activeForm}`
  - Status values: "pending", "in_progress", "completed"
- **plans/ directory**:
  - File naming: `{adjective}-{verb}-{noun}.md` (random word selection)
  - Content: Markdown plan content from ExitPlanMode tool
- **State directory override**: `CLAUDE_SIM_STATE_DIR` env var for test isolation
- **Passing integration tests**: All `dot_claude_*.rs` tests pass

## Out of Scope

- settings.json emulation (not needed for oj integration)
- Full JSONL message format (simplified for testing)
- TUI tests (those are Epic 5d6)

---
title: "Epic 5d6: Scenario-Driven TUI"
---

Implement a simplified terminal user interface using iocraft that respects scenario configuration and produces output matching real Claude Code's TUI. The TUI enables visual testing and tmux-based integration tests.

The TUI simulator renders the same layout as real Claude Code and responds to keyboard input. Scenario settings control trust prompts, permission dialogs, and model display. Tests run via tmux capture TUI state for assertions.

## In Scope

- **TUI framework**: Build with `iocraft` crate for terminal rendering
- **Visual layout matching real Claude Code**:
  - Input prompt area with user name from scenario
  - Response streaming area
  - Status bar (model from scenario, token count, session info)
  - Permission prompts for tool use
  - Trust prompt when `trusted: false` in scenario
- **Trust prompt flow**:
  - Shows "Do you trust the files in this folder?"
  - Displays working directory path
  - Yes/No options with Enter to confirm, Esc to cancel
  - Mentions security risks
- **Keyboard handling**:
  - Enter: submit input / confirm dialog
  - Escape: cancel dialog / exit
  - Ctrl+C: interrupt
  - Ctrl+D: exit
- **Permission dialogs**: Render tool permission prompts, accept/reject via keyboard
- **Thinking toggle dialog**: Ctrl+T opens thinking mode toggle with enabled/disabled options
- **Scenario integration**:
  - `--tui` flag or TTY detection activates TUI mode
  - Respect `trusted`, `user_name`, `default_model` from scenario
  - Permission mode affects which prompts appear
- **Streaming simulation**: Token-by-token response rendering
- **Passing TUI tests**: All `tui_*.rs` tests pass when run via tmux

## Out of Scope

- Full visual parity (focus on layout, not styling)
- Mouse input
- Window resize handling (use fixed dimensions)

---
title: "Epic 5d7: Settings File Support"
---

Implement support for settings.json and settings.local.json files that configure Claude Code behavior. The simulator reads and respects these settings, enabling tests that verify oj's integration with Claude's configuration system.

Claude Code reads settings from ~/.claude/settings.json (global) and .claude/settings.json or .claude/settings.local.json (project-local). These control allowed/denied tools, permission behaviors, and MCP servers. Accurate emulation ensures oj can rely on settings for automation.

## In Scope

- **Global settings**: Read `~/.claude/settings.json` (or `CLAUDE_SIM_STATE_DIR/settings.json`)
- **Project settings**: Read `.claude/settings.json` in working directory
- **Local overrides**: Read `.claude/settings.local.json` (gitignored, user-specific)
- **Settings merge order**: global < project < local (later overrides earlier)
- **Key settings to respect**:
  - `permissions.allow`: Array of tool patterns to auto-approve
  - `permissions.deny`: Array of tool patterns to always reject
  - `permissions.additionalDirectories`: Extra directories Claude can access
  - `mcpServers`: MCP server configurations (for future use)
  - `env`: Environment variable overrides
- **Scenario interaction**: Scenario `tool_execution.tools` takes precedence over settings
- **Settings inspection API**: Test helpers to query effective settings
- **Integration tests**: Verify settings affect permission prompts correctly

## Out of Scope

- MCP server spawning (just parse config, don't connect)
- Settings file watching/hot-reload
- Settings UI/editing commands
- Full settings schema validation (permissive parsing)

---
title: "Epic 5e: Closing the Gap"
---

Wire up the CLI to actually use the Engine, creating a working imperative shell that exercises the functional core built in Epics 1-5. This "proves the pudding" by running real pipelines end-to-end before adding runbook complexity in Epic 6.

The goal is to validate that our architecture works in practice: effects execute correctly with real tmux/git, the feedback loop handles failures, and session monitoring detects stuck tasks. The Claude simulator handles most testing scenarios; real Claude with haiku model provides spot-check validation.

## In Scope

- **RealAdapters bundle**: Implement `Adapters` trait combining `TmuxAdapter`, `GitAdapter`, `WkAdapter`, `OsascriptNotifier`
- **Wire up `oj run build/bugfix`**: Use Engine to create workspace, spawn session, start task (instead of printing instructions)
- **Add `oj daemon` command**: Main loop calling `poll_sessions()`, `tick_all_tasks()`, `tick_queue()` with configurable intervals
- **Wire up `oj done/checkpoint`**: Route through `engine.signal_done()` / `engine.signal_checkpoint()` instead of direct store access
- **Smoke test script**: `./scripts/smoke-test.sh` that accepts `--model simulated` for CI or `--model haiku` for live validation

## Testing Pyramid

Use the simplest layer that verifies the behavior:

**Layer 1 — Unit tests (FakeAdapters)**: Fast, no simulator. Cover all state machine transitions (Pipeline, Workspace, Session, Queue, Task, Lock, Semaphore, Guard), engine logic (effect dispatch, event routing, recovery chain, poll/tick loops), storage round-trips, error handling, adapter call verification, etc.

**Layer 2 — Integration tests (claude-sim)**: Deterministic Claude behavior. Cover CLI flags and output formats, streaming/partial responses, hook protocol, ~/.claude state files, failure injection (network, auth, rate-limit, timeout, malformed), full pipeline lifecycles, concurrent pipelines, TUI interaction, worktree/branch creation, tmux session spawning, environment variables, CLAUDE.md generation, etc.

**Layer 3 — E2E specs (bats)**: Shell-level verification of CLI behavior that's awkward to test from Rust. Cover command exit codes, stdout/stderr output format, signal handling, daemon lifecycle, multi-process coordination, etc.

**Layer 4 — Manual spot-checks (haiku)**: Validates simulator accuracy against real Claude. Verify real Claude responds, `oj done` advances phase, one full pipeline completes end-to-end, compare real output against simulator, etc.

## Out of Scope

- Guard integration with phase transitions (coordination primitives exist but aren't blocking phases yet)
- Notification configuration loading (hardcode sensible defaults)
- Scheduler for timers (poll-based only)
- TOML runbooks (still hardcoded pipelines)

---
title: "Epic 5f: Code Quality & Metrics"
---

Establish code quality baselines, identify technical debt, and create repeatable measurement infrastructure. This epic focuses on making the codebase maintainable before adding more features.

Quality metrics enable objective tracking of codebase health over time. Dead code and duplication analysis prevents cruft accumulation. File size limits ensure code remains readable and fits in LLM context windows. Parametrized test conversion improves test maintainability.

## In Scope

- **Dead code audit**: Identify unused functions, types, and modules; delete truly dead code; mark future-epic code with `#[allow(dead_code)]` + justifying comment
- **Code duplication analysis**: Use `cargo machete` and manual review to find copy-paste patterns; extract shared utilities where beneficial
- **Test analysis**: Identify tests that should use `yare` parametrization; convert repetitive test patterns
- **File size enforcement**: Source files ≤700 LOC, test files ≤1100 LOC; split large files; use sibling `_tests.rs` pattern
- **Quality measurement scripts**: `./checks/quality/evaluate.sh` producing JSON metrics:
  - LOC by crate (source/test)
  - File size (avg/max by crate)
  - Escape hatches (unsafe, unwrap, expect counts)
  - Test count and coverage percentage
- **Benchmarks**: `./checks/quality/benchmark.sh` measuring performance:
  - Compile time (cold from clean, incremental after touch)
  - Test time (cold after cargo clean, warm cached)
  - Binary size (release, stripped)
  - Basic system performance (engine tick latency, effect execution overhead)
  - Memory usage (peak RSS for common operations)
- **CI reporting**: GitHub Actions job that runs quality/benchmark scripts and uploads report as artifact (viewable, not gating)
- **Baseline report**: `reports/quality/baseline.json` capturing current state
- **Comparison reports**: Script to diff current vs baseline, highlighting regressions

## Out of Scope

- Achieving specific coverage targets (that's Epic 9)
- Refactoring code beyond what's needed for size limits
- Adding new tests (focus on reorganizing existing)

---
title: "Epic 5g: CLAUDE.md & Invariants"
---

Create per-crate and per-module CLAUDE.md files with "Landing the Plane" checklists and document invariants that guide AI assistants working on the code. Identify which invariants can be enforced via static linting and which require documentation.

CLAUDE.md files are automatically loaded when Claude works on related code, providing context-specific guidance. This reduces errors from AI assistants unfamiliar with project conventions. Custom linting catches violations at CI time. Documented invariants serve as a knowledge base for complex constraints.

## In Scope

- **Landing checklists**: Per-crate CLAUDE.md files with completion checklists:
  - `cargo check`, `cargo clippy`, `cargo fmt`
  - Test conventions (sibling `_tests.rs` files)
  - Dead code policy (delete, `#[cfg(test)]`, or justified `#[allow]`)
  - Escape hatch policy (safe alternatives, test-only exceptions)
  - Coverage requirements
- **Crate-specific guidance**: Document crate purpose, key types, common patterns
- **Custom clippy lints**: Identify invariants enforceable via:
  - `#![deny(...)]` directives
  - Custom clippy configuration in `clippy.toml`
  - `#![forbid(unsafe_code)]` where appropriate
- **Invariant documentation**: For each module, document constraints that can't be statically enforced:
  - State machine transition rules
  - Effect ordering requirements
  - Adapter contract assumptions
  - Cross-module coordination rules
- **Per-folder CLAUDE.md**: Place guidance files in directories with complex invariants:
  - `crates/core/src/engine/CLAUDE.md` - execution loop invariants
  - `crates/core/src/coordination/CLAUDE.md` - lock/semaphore safety rules
  - `crates/core/src/adapters/CLAUDE.md` - adapter implementation contracts
- **Lint enforcement script**: `./checks/lint.sh` running all custom checks

## Out of Scope

- External documentation (user-facing docs)
- API documentation (rustdoc)
- Architectural decision records (ADRs)

---
title: "Epic 6: Strategy & Runbook System"
---

Implement strategy chains for fallback approaches and the full runbook parsing system. Strategies try approaches in order until one succeeds. Runbooks are TOML files defining pipelines, tasks, guards, and strategies that the engine can load and execute.

This transforms the hardcoded pipelines into configurable runbooks. The parser separates syntactic parsing from semantic validation. Templates use Jinja2-style syntax for prompt generation with loops and conditionals.

## In Scope

- **Strategy state machine**: Try approaches in order, rollback on failure, escalate on exhaust
- **TOML parser**: Parse raw runbook structure
- **Validator**: Verify semantic correctness (references exist, types match)
- **Loader**: Load validated runbooks into runtime representations
- **Template engine**: Jinja2-style templates with variable interpolation and loops
- **Dynamic pipelines**: Replace hardcoded workflows with runbook definitions
- **Input sources**: Parse output from shell commands (JSON, ls, git branch)
- **Cross-runbook references**: `runbook.primitive` syntax for shared definitions

## Out of Scope

- Live runbook reloading
- Runbook versioning
- Custom functions in runbooks

---
title: "Epic 7: Storage & Durability"
---

Replace JSON file storage with a proper write-ahead log (WAL) for durability and audit. All state changes are operations appended to the log. Current state is derived by replaying entries. Snapshots enable efficient startup.

The WAL is the source of truth. State is never mutated directly - only by appending operations. This enables recovery from any failure point and provides a complete audit trail of all system activity.

## In Scope

- **WAL structure**: Sequence number, timestamp, machine ID, operation, checksum
- **Operation types**: Pipeline, Queue, Lock, Semaphore, Workspace, Session operations
- **WAL writer**: Durable append with fsync, load from disk
- **State materialization**: Rebuild state by replaying operations
- **Store interface**: Open, execute (write + apply), query
- **Snapshots**: Periodic state serialization for faster startup
- **Compaction**: Rewrite WAL keeping only entries after latest snapshot
- **Recovery**: Resume from last snapshot + replay
- **Event persistence**: Events now durable in WAL

## Out of Scope

- Multi-machine sync (local WAL only)
- Conflict resolution

---
title: "Epic 8: Cron, Watchers & Scanners"
---

Implement time-driven execution with cron jobs, resource monitoring with watchers, and cleanup with scanners. These enable proactive system health management without manual intervention.

Crons run on schedule. Watchers check conditions and trigger responses (using the events system for efficient wake). Scanners find stale resources and clean them up. Together they maintain system health, detect stuck processes, and prevent resource leaks.

## In Scope

- **Cron primitive**: Enable/disable, run on interval
- **Scheduler**: Event loop handling cron ticks, events, health checks
- **Watcher primitive**: Source, condition, response chain
- **Scanner primitive**: Source, condition, cleanup action
- **Watchdog runbook**: Agent idle detection with nudge → restart → escalate
- **Janitor runbook**: Stale lock/queue/worktree cleanup
- **Triager runbook**: Failure analysis and decision rules
- **Action primitive**: Named operations with cooldowns

## Out of Scope

- External cron integration (use internal scheduler)
- Complex scheduling (cron expressions)

---
title: "Epic 9: Polish & Production Readiness"
---

Final polish, performance optimization, and production readiness. Fill gaps in test coverage, add comprehensive error messages, improve CLI UX, and document operational procedures.

This epic addresses rough edges discovered during earlier development. Focus on the experience of using and operating the system day-to-day.

## In Scope

- **Test coverage**: Build on baseline from Epic 5f to reach 90%+ overall coverage; use Claude simulator for comprehensive integration testing of edge cases
- **Error messages**: Actionable errors with context and suggestions
- **CLI polish**: Help text, tab completion, progress indicators
- **Performance**: Profile hot paths identified in Epic 5f benchmarks; optimize regressions from baseline
- **Documentation**: Operational runbook, troubleshooting guide
- **Migration**: Script to migrate from bash scripts to new system
- **Graceful shutdown**: Clean termination of workers and sessions
- **Resource limits**: Memory and file handle bounds

## Out of Scope

- New features (stabilize existing functionality)
- API/SDK for external integrations
- Quality measurement infrastructure (established in Epic 5f)

---
title: "Epic 10: Multi-Machine Sync"
---

Enable multiple machines to coordinate via WAL replication. Machines connect to a central server, sync WALs, and reconcile state. Offline machines continue working and catch up on reconnect.

This epic is intentionally last because it requires a cohesive multi-machine story across both `oj` (orchestration) and `wk` (issue tracking). The design needs to address how both systems sync, how conflicts are resolved holistically, and whether they share infrastructure. Deferring this allows the single-machine system to stabilize first.

## In Scope

- **Sync protocol**: WebSocket connection, entry exchange, confirmation
- **Sync state machine**: Disconnected → Connecting → Connected → Syncing
- **Catch-up**: Request entries since last confirmed sequence
- **Entry broadcast**: Send local operations to server, receive remote operations
- **WAL merge**: Detect gaps and divergence, append compatible entries
- **Conflict detection**: Same sequence from different machines
- **Conflict resolution**: Leader wins for locks, first timestamp for queue claims
- **Partition recovery**: Reconcile after extended offline periods
- **Unified sync with wk**: Coordinated multi-machine story across both tools

## Out of Scope

- Complex CRDT-based merging
- Automatic conflict resolution for all cases (some escalate)

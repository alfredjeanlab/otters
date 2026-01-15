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

- **Test coverage**: Reach 90%+ overall coverage targets
- **Error messages**: Actionable errors with context and suggestions
- **CLI polish**: Help text, tab completion, progress indicators
- **Performance**: Profile and optimize hot paths
- **Documentation**: Operational runbook, troubleshooting guide
- **Migration**: Script to migrate from bash scripts to new system
- **Graceful shutdown**: Clean termination of workers and sessions
- **Resource limits**: Memory and file handle bounds

## Out of Scope

- New features (stabilize existing functionality)
- API/SDK for external integrations

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

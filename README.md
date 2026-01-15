# Otters (oj)

An agentic development team to write software and automate other tasks.

Otters coordinates multiple AI coding agents with runbook-defined workflows, to plan features, decompose work into issues, execute tasks, and merge results. Agents work concurrently with coordination primitives (locks, semaphores, queues) ensuring safe access to shared resources.

## Goal

A robust, testable orchestration system that:

- **Coordinates agents** - Pipelines define phased workflows (plan → decompose → execute → merge) with automatic phase transitions
- **Manages concurrency** - Locks protect the main branch, semaphores limit active agents, queues sequence work
- **Monitors health** - Watchers detect stuck agents, recovery chains nudge → restart → escalate automatically
- **Ensures recoverability** - Write-ahead log enables resume from any failure point
- **Maintains observability** - Events at every state transition, notifications on completion/escalation

## Architecture

**Functional Core, Imperative Shell** - Pure state machines generate effects; adapters execute them:

```
┌────────────────────────────────────────────────┐
│              Imperative Shell                  │
│  ┌──────────────────────────────────────────┐  │
│  │  Engine: Load state, execute effects,    │  │
│  │          persist new state               │  │
│  └──────────────────────────────────────────┘  │
│  ┌─────────┬─────────┬─────────┬─────────┐     │
│  │  tmux   │   git   │ claude  │   wk    │     │
│  │ Adapter │ Adapter │ Adapter │ Adapter │     │
│  └─────────┴─────────┴─────────┴─────────┘     │
└────────────────────────────────────────────────┘
                       │
┌──────────────────────┼─────────────────────────┐
│                      │   Functional Core       │
│  ┌───────────────────┴────────────────────┐    │
│  │  Pipeline, Queue, Task, Lock,          │    │
│  │  Semaphore, Guard state machines       │    │
│  │                                        │    │
│  │  transition(state, event) →            │    │
│  │      (new_state, effects)              │    │
│  └────────────────────────────────────────┘    │
└────────────────────────────────────────────────┘
```

## Design Principles

1. **High testability** - Target 90%+ coverage through architectural choices
2. **Composability** - Small modules compose into larger behaviors
3. **Offline-first** - Full functionality without network; sync when available
4. **Observability** - Events and metrics at every boundary
5. **Recoverability** - Checkpoint and resume from any failure

## Current Status

**In Development** - Implementing Epics 5a-5g (Validation & Quality)

Core architecture is in place (Epics 1-5). Current focus on validation, testing, and quality baselines to (a) confirm the existing code is functional and (b) confirm the current architectural direction before building the full runbook system.

**Completed:**
- **Epic 1: MVP** - Basic implementation with hardcoded pipelines, JSON storage, tmux/git adapters
- **Epic 2: Core State Machines** - Pure functional core with explicit state machines for all primitives
- **Epic 3: Engine & Execution** - Effect execution loop connecting core to real I/O
- **Epic 4: Events & Notifications** - Event bus for loose coupling, macOS notifications
- **Epic 5: Coordination Primitives** - Lock, Semaphore, Guard with heartbeat-based stale detection

**In Progress:**
- **Epic 5a-g: Validation & Quality** - AI agent simulator, integration testing, closing the gap, code quality baselines

**Planned:**
- **Epic 6: Runbook System** - TOML runbooks, strategy chains, template engine
- **Epic 7: Storage & Durability** - Write-ahead log with snapshots and compaction
- **Epic 8: Cron, Watchers & Scanners** - Time-driven execution, resource monitoring, cleanup
- **Epic 9: Polish & Production** - Test coverage, error messages, CLI UX, performance optimization
- **Epic 10: Multi-Machine Sync** - WAL replication, conflict resolution

See `EPICS.md` for detailed breakdown, `docs/` for architecture documentation, and `plans/` for individual epic plans.

## Development

This project is in active development. The functional core and coordination primitives are complete. Current work focuses on validation, testing (including an AI agent simulator), and quality baselines to confirm the code is functional and the architectural direction is sound before building the full runbook system.

### Building

```bash
cargo build
make check   # Run all CI checks (fmt, clippy, test, build, audit, deny)
```

## License

Licensed under the Business Source License 1.1
Copyright (c) Alfred Jean LLC
See LICENSE for details.

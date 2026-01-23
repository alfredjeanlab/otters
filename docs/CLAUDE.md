# Documentation

## Structure

```
docs/
├── 01-concepts/        # What things are
│   ├── RUNBOOKS.md     # Primitives: command, worker, pipeline, agent, etc.
│   └── EXECUTION.md    # Workspace and session abstractions
│
├── 02-integrations/    # External systems
│   ├── CLAUDE_CODE.md  # How Claude Code runs in sessions
│   └── ISSUE_TRACKING.md
│
├── 03-interface/       # User-facing
│   ├── CLI.md          # Commands and environment variables
│   ├── EVENTS.md       # Event types and subscriptions
│   └── MACOS.md        # macOS-specific setup
│
├── 04-architecture/    # Implementation
│   ├── 01-overview.md  # Functional core, layers, key decisions
│   ├── 02-effects.md   # Effect types
│   ├── 03-coordination.md  # Lock, Semaphore, Guard
│   ├── 04-storage.md   # WAL persistence
│   └── 05-adapters.md  # tmux, git, wk adapters
│
└── 10-runbooks/        # Example configurations
    ├── build.toml      # Feature development pipeline
    ├── bugfix.toml     # Bug fix pipeline
    ├── watchdog.toml   # Stuck detection monitors
    ├── janitor.toml    # Cleanup monitors
    ├── triager.toml    # Failure handling
    └── mergeq.toml     # Merge queue worker
```

## Conventions

- **CLAUDE.md in source dirs**: Module overview, invariants, landing checklist
- **docs/ files**: Detailed concepts, architecture, examples
- **Numbered prefixes**: `01-`, `02-` for ordering
- **Terminology**: Use "agent" (not "task") for AI invocations

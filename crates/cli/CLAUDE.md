# oj CLI

CLI binary for Otter Jobs.

## Commands

- `oj run build <name> <prompt>` - Start a build pipeline
- `oj run bugfix <id>` - Start a bugfix pipeline
- `oj pipeline list/show/transition` - Manage pipelines
- `oj workspace list/create/show/delete` - Manage workspaces
- `oj session list/show/nudge/kill` - Manage tmux sessions
- `oj queue list/add/take/complete` - Manage queues
- `oj done [--error <msg>]` - Signal phase completion
- `oj checkpoint` - Save checkpoint

## Data Storage

Pipeline and workspace state is stored in `.build/operations/`:

```
.build/operations/
├── pipelines/
├── workspaces/
└── queues/
```

## Environment Variables

- `OTTER_TASK` - Current pipeline name
- `OTTER_WORKSPACE` - Workspace directory
- `OTTER_PHASE` - Current phase

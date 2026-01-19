# oj CLI

CLI binary for Otter Jobs.

## Landing Checklist

Before committing changes to cli:
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -p otters-cli`
- [ ] CLI help text is accurate
- [ ] Error messages are user-friendly

## CLI Conventions
- Use `anyhow` for error handling with context
- `expect()` allowed for argument parsing (panic acceptable)
- Commands should be idempotent where possible

## Commands

### Run Command (Runbook-Driven)

The `oj run` command creates pipelines from runbook definitions:

```bash
# Run a build pipeline
oj run build --input name=auth --input prompt="Add authentication"

# Run a bugfix pipeline
oj run bugfix --input bug=123

# Run any runbook pipeline
oj run <runbook> [--pipeline <name>] --input key=value [--input key2=value2]
```

Options:
- `<runbook>` - Name of the runbook (e.g., "build", "bugfix")
- `--pipeline, -p` - Pipeline name within the runbook (defaults to runbook-specific default)
- `--input, -i` - Pipeline inputs as key=value pairs (can be repeated)

### Other Commands

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

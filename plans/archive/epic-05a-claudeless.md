# Epic 5a: Claudeless - Claude CLI Simulator

## Overview

Build `claudeless`, a standalone CLI tool that emulates the `claude` CLI for integration testing. Enables deterministic testing of otters without API costs.

**Repository:** Separate project (not in otters workspace)
**Binary name:** `claudeless` (installed globally, shadows `claude` via PATH manipulation)

## Architecture

```
claudeless/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── cli.rs           # Argument parsing (mirrors claude flags)
│   ├── scenario.rs      # Scenario loading and pattern matching
│   └── output.rs        # JSON/stream-JSON/text output
├── scenarios/           # Example scenarios
└── tests/
```

### Core Components

1. **CLI Parser** - Accept same flags as real Claude (`-p`, `--output-format`, `--model`, etc.)
2. **Scenario Engine** - Match prompts to scripted responses via TOML config
3. **Output Formatter** - Produce text/JSON/stream-JSON matching real Claude format
4. **Failure Injection** - Simulate network errors, auth failures, rate limits

**Goal:** Behave identically to real `claude` CLI - same errors, same input requirements, same defaults.

## Scenarios

Scenarios are TOML files that script responses based on prompt patterns:

```toml
# scenarios/simple.toml
name = "simple"

[[responses]]
pattern = { type = "contains", text = "hello" }
response = "Hello! How can I help?"

[[responses]]
pattern = { type = "regex", pattern = "fix.*bug" }
response = "I'll fix that bug."

[default_response]
text = "Task completed."
```

**Pattern types:** `exact`, `contains`, `regex`, `glob`, `any`

### Failure Scenarios

```toml
# scenarios/network-failure.toml
name = "network-failure"
failure = { type = "network_error", message = "Connection refused" }
```

## Requirements for Otters Testing

From `epic-05b-closing-gap-part2-integration.md`:

### Installation & Discovery

- Claudeless must be installed globally (`cargo install --path .`)
- Tests find it via `which claudeless`
- Tests create temp directory with `claude` -> `claudeless` symlink
- Symlink directory prepended to PATH for subprocess calls

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `CLAUDELESS_SCENARIO` | Path to scenario TOML file |
| `CLAUDELESS_FAILURE` | Failure mode (network, auth, rate-limit) |
| `CLAUDELESS_DELAY_MS` | Response delay in milliseconds |

### Required Scenarios for Otters

```
crates/cli/tests/scenarios/
├── simple.toml              # Basic responses
├── auto-done.toml           # Auto-completes tasks
├── network-failure.toml     # Connection errors
├── auth-failure.toml        # Auth errors
├── rate-limit.toml          # 429 responses
├── timeout.toml             # Slow/hanging responses
├── malformed.toml           # Invalid JSON output
└── transient-failure.toml   # Fails then succeeds
```

### Test Pattern

```rust
#[test]
fn test_example() {
    // Skip if not installed
    if !claudeless::is_claudeless_available() {
        eprintln!("Skipping: claudeless not found");
        return;
    }

    let temp = setup_test_env();
    let scenario = claudeless::simple_scenario(temp.path());
    let path = claudeless::setup_claudeless_path();

    Command::cargo_bin("oj")
        .env("PATH", &path)
        .env("CLAUDELESS_SCENARIO", scenario.display().to_string())
        .args(["run", "build", "test", "prompt"])
        .assert()
        .success();
}
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (auth, network, etc.) |
| 130 | Interrupted (Ctrl+C) |

## Output Format

Must match real Claude CLI output structure:

**JSON (`--output-format json`):**
```json
{"type":"result","subtype":"success","result":"Response","session_id":"..."}
```

**Stream-JSON (`--output-format stream-json`):**
```jsonl
{"type":"system","subtype":"init","session_id":"..."}
{"type":"assistant","subtype":"message_start"}
{"type":"content_block_delta","delta":"Hello"}
{"type":"result","subtype":"success"}
```

## Additional Required Features

### TUI Mode

Interactive terminal UI matching real Claude CLI:
- Default when running in TTY
- Trust prompt on first run
- Input prompt with cursor and history
- Response streaming display
- Status bar (model, tokens)
- Permission dialogs

When not in TTY: require `-p` with prompt argument or stdin input (match real Claude's error messages).

### MCP Server Support

Emulate MCP (Model Context Protocol) server configuration:
- Load `mcp_servers` from settings files
- Report available tools in `system.init` event
- Accept `--mcp-config` flag

### Permissions & Settings

Emulate `~/.claude` directory structure:
- `settings.json` - global settings
- `projects/<hash>/settings.json` - project settings
- Permission mode handling (`--permission-mode`)
- Allowed/disallowed tools lists

## Verification

- [ ] `claudeless --help` shows expected flags
- [ ] `claudeless -p "hello"` returns scenario response
- [ ] `CLAUDELESS_SCENARIO=x.toml claudeless -p "test"` uses scenario
- [ ] `CLAUDELESS_FAILURE=network claudeless -p "test"` fails appropriately
- [ ] Otters integration tests pass with claudeless in PATH

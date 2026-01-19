# Epic 5b Part 2: Daemon Signal Handling Tests

**Root Feature:** `oj-3316`

## Overview

Add the two missing daemon signal handling tests from the original plan:
- `test_daemon_handles_sigint`
- `test_daemon_handles_sigterm`

## Current State

The daemon already has signal handling implemented in `crates/cli/src/commands/daemon.rs:36-39`:

```rust
ctrlc::set_handler(move || {
    eprintln!("\nShutting down daemon...");
    r.store(false, Ordering::SeqCst);
})?;
```

The `ctrlc` crate handles both SIGINT and SIGTERM. When triggered:
1. Prints "Shutting down daemon..." to stderr
2. Sets `running` flag to false
3. Loop exits and prints "Daemon stopped"

## Implementation

### File: `crates/cli/tests/daemon_polling.rs`

Add two tests at the end of the file:

```rust
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[test]
#[cfg(unix)]
fn test_daemon_handles_sigint() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let temp = setup_test_env();

    // Start daemon in background (not --once)
    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    // Give daemon time to start
    std::thread::sleep(Duration::from_millis(500));

    // Send SIGINT
    let pid = Pid::from_raw(child.id() as i32);
    kill(pid, Signal::SIGINT).expect("Failed to send SIGINT");

    // Wait for exit with timeout
    let status = child.wait().expect("Failed to wait for daemon");

    // On Unix, SIGINT typically results in exit code 0 when handled gracefully
    // or 130 (128 + 2) when terminated by signal
    assert!(
        status.success() || status.code() == Some(130),
        "Daemon should exit cleanly on SIGINT, got: {:?}",
        status
    );
}

#[test]
#[cfg(unix)]
fn test_daemon_handles_sigterm() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let temp = setup_test_env();

    // Start daemon in background (not --once)
    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    // Give daemon time to start
    std::thread::sleep(Duration::from_millis(500));

    // Send SIGTERM
    let pid = Pid::from_raw(child.id() as i32);
    kill(pid, Signal::SIGTERM).expect("Failed to send SIGTERM");

    // Wait for exit with timeout
    let status = child.wait().expect("Failed to wait for daemon");

    // SIGTERM should result in clean exit (0) or signal termination (143 = 128 + 15)
    assert!(
        status.success() || status.code() == Some(143),
        "Daemon should exit cleanly on SIGTERM, got: {:?}",
        status
    );
}
```

### Dependencies

Add `nix` to dev-dependencies in `crates/cli/Cargo.toml`:

```toml
[dev-dependencies]
assert_cmd = "2"
nix = { version = "0.29", features = ["signal", "process"] }
predicates = "3"
serde_json = "1"
tempfile = "3"
```

### Alternative: Without nix dependency

If adding `nix` is undesirable, use `libc` directly (already a transitive dependency):

```rust
#[test]
#[cfg(unix)]
fn test_daemon_handles_sigint() {
    let temp = setup_test_env();

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    std::thread::sleep(Duration::from_millis(500));

    // Send SIGINT using libc
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }

    let status = child.wait().expect("Failed to wait for daemon");
    assert!(
        status.success() || status.code() == Some(130),
        "Daemon should exit cleanly on SIGINT, got: {:?}",
        status
    );
}

#[test]
#[cfg(unix)]
fn test_daemon_handles_sigterm() {
    let temp = setup_test_env();

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_oj"))
        .current_dir(temp.path())
        .args(["daemon", "--poll-interval", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn daemon");

    std::thread::sleep(Duration::from_millis(500));

    // Send SIGTERM using libc
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let status = child.wait().expect("Failed to wait for daemon");
    assert!(
        status.success() || status.code() == Some(143),
        "Daemon should exit cleanly on SIGTERM, got: {:?}",
        status
    );
}
```

For the libc approach, add to dev-dependencies:

```toml
[dev-dependencies]
libc = "0.2"
```

## Verification

```bash
# Run the new tests
cargo test -p oj-cli --test daemon_polling -- test_daemon_handles

# Full test suite
cargo test -p oj-cli --test daemon_polling

# CI check
make check
```

## Success Criteria

- [ ] `test_daemon_handles_sigint` passes
- [ ] `test_daemon_handles_sigterm` passes
- [ ] Tests are `#[cfg(unix)]` gated (signals work differently on Windows)
- [ ] Total daemon_polling.rs test count: 15
- [ ] `make check` passes

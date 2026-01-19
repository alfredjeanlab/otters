# Epic 1: MVP - Replace Bash Scripts

**Root Feature:** `otters-9428`

## Overview

Replace existing bash scripts (`feature`, `bugfix`, `mergeq`, `merge`, `tree`) with a Rust implementation that provides equivalent functionality. The MVP prioritizes getting something working end-to-end over architectural purity:

- **Simple storage**: JSON files instead of WAL
- **Basic session management**: tmux spawn/kill/send/capture with output-based heartbeat
- **Hardcoded pipelines**: `build` and `bugfix` workflows without full runbook parsing
- **Sequential merge queue**: Single daemon processing merges one at a time

This validates the workspace/session model and Claude Code integration before building the full system.

## Project Structure

```
crates/
├── cli/                          # CLI binary
│   ├── Cargo.toml
│   ├── CLAUDE.md
│   ├── src/
│   │   ├── main.rs               # Entry point, arg parsing
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── run.rs            # oj run <command>
│   │   │   ├── pipeline.rs       # oj pipeline list/show/transition
│   │   │   ├── workspace.rs      # oj workspace list/create/delete
│   │   │   ├── session.rs        # oj session list/show/nudge/kill
│   │   │   ├── queue.rs          # oj queue list/add/take/complete
│   │   │   └── signal.rs         # oj done, oj checkpoint
│   │   └── output.rs             # JSON/text formatting
│   └── tests/                    # CLI integration tests
│
└── core/                         # Library crate
    ├── Cargo.toml
    ├── CLAUDE.md
    ├── src/
    │   ├── lib.rs
    │   │
    │   ├── # Pure state (no I/O)
    │   ├── pipeline.rs           # Pipeline state machine (simplified)
    │   ├── pipeline_tests.rs
    │   ├── queue.rs              # Queue operations
    │   ├── queue_tests.rs
    │   ├── workspace.rs          # Workspace state
    │   ├── workspace_tests.rs
    │   ├── session.rs            # Session state + heartbeat
    │   ├── session_tests.rs
    │   ├── effect.rs             # Effect enum
    │   ├── clock.rs              # Clock trait + FakeClock
    │   ├── id.rs                 # IdGen trait + implementations
    │   │
    │   ├── # Engine (orchestration)
    │   ├── engine/
    │   │   ├── mod.rs
    │   │   ├── executor.rs       # Effect execution loop
    │   │   └── worker.rs         # Queue worker (merge daemon)
    │   │
    │   ├── # Adapters (I/O)
    │   ├── adapters/
    │   │   ├── mod.rs
    │   │   ├── traits.rs         # SessionAdapter, RepoAdapter, etc.
    │   │   ├── tmux.rs           # Real tmux implementation
    │   │   ├── git.rs            # Real git implementation
    │   │   ├── claude.rs         # Claude Code integration
    │   │   ├── wk.rs             # Issue tracker (wk CLI)
    │   │   └── fake.rs           # FakeAdapters for testing
    │   │
    │   ├── # Storage (simplified)
    │   ├── storage/
    │   │   ├── mod.rs
    │   │   └── json.rs           # JSON file persistence
    │   │
    │   └── # Hardcoded pipelines
    │   └── pipelines/
    │       ├── mod.rs
    │       ├── build.rs          # plan → decompose → execute → merge
    │       └── bugfix.rs         # setup → fix → verify → merge
    │
    └── tests/                    # Real adapter contract tests
        ├── tmux_contract.rs
        └── git_contract.rs

.build/                           # Runtime data directory
└── operations/
    └── <name>/
        ├── state.json            # Pipeline/operation state
        └── queue.json            # Queue state (for merge queue)
```

## Dependencies

### Core Crate

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
tracing = "0.1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
proptest = "1"
yare = "3"
tempfile = "3"
```

### CLI Crate

```toml
[dependencies]
oj-core = { path = "../core" }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

## Implementation Phases

### Phase 1: Foundation & Adapter Traits

**Goal**: Establish project structure with adapter traits and fake implementations.

**Deliverables**:
1. Cargo workspace with `cli` and `core` crates
2. `Clock` and `IdGen` traits with real/fake implementations
3. Adapter trait definitions:
   - `SessionAdapter` (spawn, send, kill, is_alive, capture_pane)
   - `RepoAdapter` (worktree_add, worktree_remove, is_clean, merge)
   - `IssueAdapter` (list, get, start, done, note)
4. `FakeAdapters` with call recording
5. Basic effect enum with core variants

**Key Code**:

```rust
// core/src/clock.rs
pub trait Clock: Clone + Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Clone)]
pub struct SystemClock;
// impl Clock: returns Instant::now()

#[derive(Clone)]
pub struct FakeClock { current: Arc<Mutex<Instant>> }
// impl Clock: returns current, advance() adds duration
```

```rust
// core/src/adapters/traits.rs
#[async_trait]
pub trait SessionAdapter: Clone + Send + Sync + 'static {
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError>;
    async fn send(&self, id: &SessionId, input: &str) -> Result<(), SessionError>;
    async fn kill(&self, id: &SessionId) -> Result<(), SessionError>;
    async fn is_alive(&self, id: &SessionId) -> Result<bool, SessionError>;
    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError>;
    async fn list(&self) -> Result<Vec<SessionInfo>, SessionError>;
}

#[async_trait]
pub trait RepoAdapter: Clone + Send + Sync + 'static {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError>;
    async fn worktree_remove(&self, path: &Path) -> Result<(), RepoError>;
    async fn worktree_list(&self) -> Result<Vec<WorktreeInfo>, RepoError>;
    async fn is_clean(&self, path: &Path) -> Result<bool, RepoError>;
    async fn merge(&self, path: &Path, branch: &str, strategy: MergeStrategy) -> Result<MergeResult, RepoError>;
}
```

**Verification**:
- `cargo build` succeeds
- `cargo test --lib` passes with fake adapter unit tests
- Contract test stubs exist for real adapters

---

### Phase 2: Workspace & Session State Machines

**Goal**: Implement workspace creation/deletion and session lifecycle with heartbeat detection.

**Deliverables**:
1. `Workspace` state machine (Creating → Ready → InUse → Dirty/Stale)
2. `Session` state machine (Starting → Running → Idle → Dead)
3. Heartbeat evaluation (terminal output monitoring)
4. JSON storage for workspace/session state
5. CLI commands: `oj workspace list/create/delete`, `oj session list/show/nudge/kill`

**Key Code**:

```rust
// core/src/workspace.rs
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub state: WorkspaceState,
    pub created_at: DateTime<Utc>,
}

pub enum WorkspaceState {
    Creating,
    Ready,
    InUse { session_id: SessionId },
    Dirty,
    Stale,
}

pub enum WorkspaceEvent {
    SetupComplete,
    SessionStarted { session_id: SessionId },
    SessionEnded { clean: bool },
    BranchGone,
    Remove,
}

impl Workspace {
    pub fn transition(&self, event: WorkspaceEvent) -> (Workspace, Vec<Effect>) {
        // Match (state, event) → new state + effects
        // Creating + SetupComplete → Ready
        // Ready + SessionStarted → InUse
        // InUse + SessionEnded{clean} → Ready or Dirty
        // Invalid transitions return unchanged
    }
}
```

```rust
// core/src/session.rs
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub state: SessionState,
    pub last_output: Option<Instant>,
    pub idle_threshold: Duration,
    pub created_at: Instant,
}

pub enum SessionState {
    Starting,
    Running,
    Idle { since: Instant },
    Dead { reason: DeathReason },
}

impl Session {
    pub fn evaluate_heartbeat(&self, output_time: Option<Instant>, clock: &impl Clock)
        -> (Session, Vec<Effect>)
    {
        // Running: check if idle threshold exceeded → emit SessionIdle
        // Idle: if new output → transition to Running, emit SessionActive
    }
}
```

**Verification**:
- Unit tests for workspace state transitions (90%+ coverage)
- Unit tests for session heartbeat evaluation
- `oj workspace create build auth` creates worktree at `.worktrees/build-auth`
- `oj workspace list` shows workspaces with state
- `oj session list` shows active tmux sessions

---

### Phase 3: Hardcoded Pipelines

**Goal**: Implement the `build` and `bugfix` pipelines with hardcoded phase logic.

**Deliverables**:
1. Simplified `Pipeline` state machine (phases, transitions, blocked state)
2. `build` pipeline: init → plan → decompose → execute → merge → done
3. `bugfix` pipeline: setup → fix → verify → merge → cleanup
4. Task spawning in tmux sessions
5. `oj done` and `oj done --error` signaling
6. CLI: `oj run build <name> <prompt>`, `oj run bugfix <id>`
7. CLI: `oj pipeline list/show/transition`

**Key Code**:

```rust
// core/src/pipeline.rs
pub struct Pipeline {
    pub id: PipelineId,
    pub kind: PipelineKind,
    pub name: String,
    pub phase: Phase,
    pub inputs: HashMap<String, String>,
    pub workspace_id: Option<WorkspaceId>,
    pub created_at: DateTime<Utc>,
}

pub enum PipelineKind { Build, Bugfix }

pub enum Phase {
    Init,
    Blocked { waiting_on: String },
    Plan, Decompose, Execute,
    Fix, Verify, Merge, Cleanup,
    Done,
    Failed { reason: String },
}

impl Pipeline {
    pub fn transition(&self, event: PipelineEvent, clock: &impl Clock) -> (Pipeline, Vec<Effect>) {
        // Match (kind, phase, event) → next phase + emit PipelinePhase event
        // Build: Init → Plan → Decompose → Execute → Merge → Done
        // Bugfix: Init → Fix → Verify → Merge → Cleanup → Done
    }
}
```

```rust
// core/src/pipelines/build.rs
pub struct BuildPipeline;

impl BuildPipeline {
    pub fn phase_config(phase: &Phase) -> PhaseConfig {
        // Init: run git worktree + wk new, next=Plan
        // Plan: spawn claude --print with plan template, next=Decompose
        // Decompose/Execute/Merge: similar pattern
    }
}
```

**Verification**:
- `oj run build auth "Add authentication"` creates workspace, spawns planning session
- `oj pipeline show auth` displays current phase
- `oj done` in session advances pipeline to next phase
- `oj done --error "compilation failed"` moves pipeline to Failed state
- Unit tests cover all phase transitions for both pipeline types

---

### Phase 4: Merge Queue & Worker

**Goal**: Implement file-based merge queue with sequential processing daemon.

**Deliverables**:
1. `Queue` data structure with priority ordering
2. JSON file persistence for queue state
3. Merge worker daemon (single instance)
4. Merge strategy: fast-forward → rebase → escalate
5. Simple file lock for main branch
6. CLI: `oj queue list/add/complete`, worker start/stop

**Key Code**:

```rust
// core/src/queue.rs
pub struct Queue {
    pub name: String,
    pub items: Vec<QueueItem>,
    pub processing: Option<QueueItem>,
}

pub struct QueueItem {
    pub id: String,
    pub data: HashMap<String, String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
}

impl Queue {
    pub fn push(&self, item: QueueItem) -> Queue {
        // Insert item, sort by priority desc then created_at asc
    }

    pub fn take(&self) -> (Queue, Option<QueueItem>) {
        // If not processing and items exist, remove first item and set as processing
    }

    pub fn complete(&self, id: &str) -> Queue {
        // Clear processing if id matches
    }
}
```

```rust
// core/src/engine/worker.rs
pub struct MergeWorker<A: Adapters> { adapters: A, store: JsonStore }

impl<A: Adapters> MergeWorker<A> {
    pub async fn run_once(&mut self) -> Result<bool> {
        // Load queue, take item, try_merge
        // On success: complete item
        // On failure: requeue (attempts < 2) or dead_letter
    }

    async fn try_merge(&self, branch: &str) -> Result<()> {
        // Try fast-forward → rebase → escalate on conflict
    }
}
```

**Verification**:
- `oj queue add merges branch=feature-x` enqueues merge request
- `oj queue list merges` shows queue with priority order
- Worker processes merges sequentially
- Fast-forward merge completes without issues
- Rebase used when fast-forward fails
- Conflict escalation logged when rebase fails

---

### Phase 5: Real Adapter Implementations

**Goal**: Implement production adapters for tmux, git, wk, and Claude Code.

**Deliverables**:
1. `TmuxAdapter` - spawn, send, kill, capture_pane via tmux CLI
2. `GitAdapter` - worktree operations, merge strategies via git CLI
3. `WkAdapter` - issue operations via wk CLI
4. `ClaudeAdapter` - invoke via session, heartbeat via output + log monitoring
5. Contract tests passing for all real adapters

**Key Code**:

```rust
// core/src/adapters/tmux.rs
pub struct TmuxAdapter { session_prefix: String }

#[async_trait]
impl SessionAdapter for TmuxAdapter {
    async fn spawn(&self, name: &str, cwd: &Path, cmd: &str) -> Result<SessionId, SessionError> {
        // tmux new-session -d -s {prefix}{name} -c {cwd} {cmd}
        // Handle duplicate session error
    }

    async fn capture_pane(&self, id: &SessionId, lines: u32) -> Result<String, SessionError> {
        // tmux capture-pane -t {id} -p -S -{lines}
    }
    // ... other methods: send, kill, is_alive, list
}
```

```rust
// core/src/adapters/git.rs
pub struct GitAdapter { repo_root: PathBuf }

#[async_trait]
impl RepoAdapter for GitAdapter {
    async fn worktree_add(&self, branch: &str, path: &Path) -> Result<(), RepoError> {
        // git worktree add -b {branch} {path}
    }

    async fn merge(&self, path: &Path, branch: &str, strategy: MergeStrategy) -> Result<MergeResult, RepoError> {
        // FastForward: git merge --ff-only
        // Rebase: git rebase
        // Merge: git merge --no-ff
        // Detect conflicts from stderr
    }
}
```

**Verification**:
- Contract tests pass: `cargo test --manifest-path crates/core/Cargo.toml -- --ignored`
- Real tmux sessions created and destroyed
- Git worktrees created in `.worktrees/`
- `wk` commands execute correctly for issue operations

---

### Phase 6: Integration & Polish

**Goal**: Wire everything together for end-to-end workflows.

**Deliverables**:
1. Full `oj run build` flow working end-to-end
2. Full `oj run bugfix` flow working end-to-end
3. Merge queue daemon integrated with pipeline completion
4. Environment variables passed to sessions (OTTER_TASK, OTTER_WORKSPACE, etc.)
5. CLAUDE.md generation for workspaces
6. Basic error messages and logging
7. E2E spec tests for critical paths

**Key Integration Points**:

```rust
// cli/src/commands/run.rs
pub async fn run_build(name: String, prompt: String, adapters: impl Adapters) -> Result<()> {
    // 1. Create workspace via worktree_add
    // 2. Save workspace state
    // 3. Generate CLAUDE.md with task context
    // 4. Create pipeline, save state
    // 5. Spawn session with claude --print
    // 6. Set OTTER_TASK, OTTER_WORKSPACE, OTTER_PHASE env vars
}
```

```rust
// cli/src/commands/signal.rs
pub async fn handle_done(error: Option<String>, adapters: impl Adapters) -> Result<()> {
    // 1. Read OTTER_TASK, OTTER_PHASE from env
    // 2. Load pipeline from store
    // 3. Transition with PhaseComplete or PhaseFailed
    // 4. Save pipeline, execute effects
}
```

**Verification**:
- Complete `oj run build auth "Add auth"` → planning session starts
- `oj done` advances through phases
- `oj run bugfix 42` creates workspace, runs fix task
- Merge queue processes completed builds
- E2E test: build flow from start to merge

## Key Implementation Details

### State Persistence Pattern

All state uses JSON files for simplicity:

```rust
// core/src/storage/json.rs
pub struct JsonStore { base_path: PathBuf }

impl JsonStore {
    pub fn save<T: Serialize>(&self, kind: &str, id: &str, data: &T) -> Result<()>;
    pub fn load<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T>;
    // Saves to {base_path}/{kind}/{id}.json
}
```

File layout: `.build/operations/{pipelines,workspaces,queues}/*.json`

### Heartbeat Detection

Simple output-based heartbeat (MVP version):

```rust
impl Session {
    pub async fn check_heartbeat(&self, adapter: &impl SessionAdapter, clock: &impl Clock)
        -> (Session, Vec<Effect>)
    {
        // Capture pane output, hash to detect changes
        // If changed: update last_activity
        // If idle_threshold exceeded: emit SessionIdle
    }
}
```

### CLAUDE.md Generation

Template includes: task prompt, `oj done` signaling instructions, environment variables (OTTER_TASK, OTTER_WORKSPACE, OTTER_PHASE), and guidelines.

### Simple File Lock

MVP uses filesystem locking (not heartbeat-based):

```rust
pub struct FileLock { path: PathBuf }

impl FileLock {
    pub fn try_acquire(name: &str) -> Result<Option<FileLock>>;
    // Atomic create .build/locks/{name}.lock, write PID

    pub fn release(self) -> Result<()>;
    // Remove lock file
}
```

## Verification Plan

### Unit Tests (Target: 85%+ coverage)

Run with: `cargo test --lib`

| Module | Key Tests |
|--------|-----------|
| `clock` | FakeClock advance, SystemClock returns increasing time |
| `workspace` | All state transitions, invalid transitions ignored |
| `session` | Heartbeat detection, idle threshold, recovery chain |
| `pipeline` | Build phases, bugfix phases, error handling |
| `queue` | Push ordering, take/complete, requeue logic |

### Integration Tests

Run with: `cargo test`

| Test | Description |
|------|-------------|
| `engine_executes_effects` | Effects call correct adapter methods |
| `pipeline_persists_state` | State survives process restart |
| `queue_ordering` | Priority + FIFO ordering works |
| `workspace_creates_worktree` | Real git worktree created |

### Contract Tests (Real Adapters)

Run with: `cargo test -- --ignored`

| Adapter | Contract |
|---------|----------|
| `TmuxAdapter` | spawn/kill/is_alive/capture_pane |
| `GitAdapter` | worktree_add/remove, merge strategies |
| `WkAdapter` | list/get/start/done (requires wk setup) |

### E2E Spec Tests

Run with: `cargo test --manifest-path checks/specs/Cargo.toml`

| Spec | Description |
|------|-------------|
| `build_flow` | Full build pipeline from `oj run build` to merge |
| `bugfix_flow` | Full bugfix pipeline from `oj run bugfix` to cleanup |
| `merge_queue` | Queue processes multiple branches in order |
| `signaling` | `oj done` and `oj done --error` work correctly |

### Manual Verification Checklist

- [ ] `oj run build test-feature "Add feature X"` creates workspace
- [ ] `oj workspace list` shows workspace in Ready state
- [ ] `oj session list` shows tmux session
- [ ] `oj pipeline show test-feature` shows current phase
- [ ] `oj done` advances phase
- [ ] `oj done --error "reason"` moves to Failed
- [ ] `oj queue add merges branch=test-feature` enqueues
- [ ] `oj queue list merges` shows queue contents
- [ ] Merge worker processes queue items
- [ ] `oj workspace delete test-feature` cleans up

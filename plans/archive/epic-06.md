# Epic 6: Strategy & Runbook System

**Root Feature:** `otters-9df4`

Transform hardcoded pipelines into configurable TOML runbooks with strategy chains for fallback approaches.

## 1. Overview

This epic implements a complete runbook system that replaces hardcoded pipeline definitions with configurable TOML files. The system consists of:

- **Strategy state machine**: Tries approaches in order, rolls back on failure, escalates on exhaustion
- **TOML parser**: Parses raw runbook structure (syntactic layer)
- **Validator**: Verifies semantic correctness - references exist, types match
- **Loader**: Converts validated TOML into runtime representations
- **Template engine**: Jinja2-style variable interpolation with loops and conditionals
- **Input sources**: Parses shell command output (JSON, line-based)
- **Cross-runbook references**: `runbook.primitive` syntax for shared definitions

The architecture follows the existing pattern: pure functional state machines with deterministic transitions, effects for side effects, and adapters for I/O.

## 2. Project Structure

```
crates/core/src/
├── runbook/                    # New module
│   ├── mod.rs                  # Module exports
│   ├── types.rs                # Runbook data types (Command, Worker, Queue, etc.)
│   ├── parser.rs               # TOML parsing (syntactic layer)
│   ├── parser_tests.rs         # Parser tests
│   ├── validator.rs            # Semantic validation
│   ├── validator_tests.rs      # Validator tests
│   ├── loader.rs               # Runtime type conversion
│   ├── loader_tests.rs         # Loader tests
│   ├── template.rs             # Template engine wrapper
│   ├── template_tests.rs       # Template tests
│   ├── input.rs                # Input source parsing
│   ├── input_tests.rs          # Input source tests
│   └── CLAUDE.md               # Module documentation
├── strategy/                   # New module
│   ├── mod.rs                  # Strategy state machine
│   ├── strategy_tests.rs       # Strategy tests
│   └── CLAUDE.md               # Module documentation
├── pipelines/
│   ├── mod.rs                  # Keep for PipelineKind, PhaseConfig
│   ├── dynamic.rs              # Replace hardcoded with runbook-driven
│   └── CLAUDE.md               # Updated documentation
└── lib.rs                      # Add runbook, strategy exports
```

## 3. Dependencies

Add to `crates/core/Cargo.toml`:

```toml
[dependencies]
toml = "0.8"                    # TOML parsing
minijinja = "2.0"               # Jinja2-style templates
```

Both crates are well-maintained, widely used, and have no transitive dependency conflicts.

## 4. Implementation Phases

### Phase 1: Strategy State Machine

**Goal**: Implement the Strategy primitive - a state machine that tries approaches in order with rollback/escalate semantics.

**Files**:
- `crates/core/src/strategy/mod.rs`
- `crates/core/src/strategy/strategy_tests.rs`
- `crates/core/src/strategy/CLAUDE.md`

**Types**:
```rust
pub struct Strategy {
    pub name: String,
    pub checkpoint: Option<String>,
    pub attempts: Vec<Attempt>,
    pub state: StrategyState,
    pub current_attempt: usize,
    pub on_exhaust: ExhaustAction,
}

pub struct Attempt {
    pub name: String,
    pub run: Option<String>,
    pub task: Option<String>,
    pub timeout: Duration,
    pub rollback: Option<String>,
}

pub enum StrategyState {
    Ready,
    Checkpointing,
    Trying { attempt_index: usize, started_at: Instant },
    RollingBack { attempt_index: usize },
    Succeeded { attempt_name: String },
    Exhausted,
    Failed { reason: String },
}

pub enum StrategyEvent {
    Start,
    CheckpointComplete { value: String },
    CheckpointFailed { reason: String },
    AttemptSucceeded,
    AttemptFailed { reason: String },
    AttemptTimeout,
    RollbackComplete,
    RollbackFailed { reason: String },
}

pub enum ExhaustAction {
    Escalate,
    Fail,
    Retry { after: Duration },
}
```

**Transition logic**:
- `Ready` + `Start` → `Checkpointing` (if checkpoint defined) or `Trying { 0 }`
- `Checkpointing` + `CheckpointComplete` → `Trying { 0 }`
- `Trying { n }` + `AttemptSucceeded` → `Succeeded`
- `Trying { n }` + `AttemptFailed` → `RollingBack { n }` (if rollback) or `Trying { n+1 }`
- `Trying { last }` + `AttemptFailed` → `Exhausted`
- `RollingBack { n }` + `RollbackComplete` → `Trying { n+1 }`

**Effects**:
```rust
pub enum StrategyEffect {
    RunCheckpoint { command: String },
    RunAttempt { attempt: Attempt },
    SpawnTask { task_name: String },
    RunRollback { command: String },
    Emit(Event),
}
```

**Milestone**: Strategy state machine passes property tests for all state transitions.

---

### Phase 2: TOML Parser (Syntactic Layer)

**Goal**: Parse raw runbook TOML into unvalidated data structures.

**Files**:
- `crates/core/src/runbook/types.rs`
- `crates/core/src/runbook/parser.rs`
- `crates/core/src/runbook/parser_tests.rs`

**Raw types** (mirror TOML structure exactly):
```rust
#[derive(Debug, Deserialize)]
pub struct RawRunbook {
    pub command: Option<HashMap<String, RawCommand>>,
    pub worker: Option<HashMap<String, RawWorker>>,
    pub queue: Option<HashMap<String, RawQueue>>,
    pub pipeline: Option<HashMap<String, RawPipeline>>,
    pub task: Option<HashMap<String, RawTask>>,
    pub guard: Option<HashMap<String, RawGuard>>,
    pub strategy: Option<HashMap<String, RawStrategy>>,
    pub lock: Option<HashMap<String, RawLock>>,
    pub semaphore: Option<HashMap<String, RawSemaphore>>,
    pub config: Option<HashMap<String, toml::Value>>,
    pub functions: Option<HashMap<String, String>>,
    pub events: Option<RawEvents>,
}

#[derive(Debug, Deserialize)]
pub struct RawPipeline {
    pub inputs: Vec<String>,
    pub defaults: Option<HashMap<String, String>>,
    pub phase: Vec<RawPhase>,
}

#[derive(Debug, Deserialize)]
pub struct RawPhase {
    pub name: String,
    pub run: Option<String>,
    pub task: Option<String>,
    pub strategy: Option<String>,
    pub pre: Option<Vec<String>>,
    pub post: Option<Vec<String>>,
    pub lock: Option<String>,
    pub semaphore: Option<String>,
    pub next: Option<String>,
    pub on_fail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawStrategy {
    pub checkpoint: Option<String>,
    pub on_exhaust: Option<String>,
    pub attempt: Vec<RawAttempt>,
}

#[derive(Debug, Deserialize)]
pub struct RawAttempt {
    pub name: String,
    pub run: Option<String>,
    pub task: Option<String>,
    #[serde(with = "humantime_serde", default)]
    pub timeout: Option<Duration>,
    pub rollback: Option<String>,
}
```

**Parser API**:
```rust
pub fn parse_runbook(toml_content: &str) -> Result<RawRunbook, ParseError>;
pub fn parse_runbook_file(path: &Path) -> Result<RawRunbook, ParseError>;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("TOML syntax error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("IO error reading {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}
```

**Milestone**: Parser successfully parses all example runbooks in `docs/10-example-runbooks/`.

---

### Phase 3: Semantic Validator

**Goal**: Validate that parsed runbooks are semantically correct.

**Files**:
- `crates/core/src/runbook/validator.rs`
- `crates/core/src/runbook/validator_tests.rs`

**Validation rules**:
1. **Reference integrity**: All referenced names exist
   - `phase.task` references a defined `[task.*]`
   - `phase.strategy` references a defined `[strategy.*]`
   - `phase.pre`/`post` guards reference defined `[guard.*]`
   - `phase.lock` references a defined `[lock.*]`
   - `phase.semaphore` references a defined `[semaphore.*]`
   - `worker.handler` references a defined `pipeline.*` or `task.*`
   - `worker.queue` references a defined `[queue.*]`
   - `strategy.attempt[].task` references a defined `[task.*]`

2. **Phase graph validity**:
   - All `phase.next` values reference defined phases or terminal states (`done`, `failed`)
   - All `phase.on_fail` values are valid actions (`escalate`, phase name, or strategy)
   - No unreachable phases (all phases reachable from initial phase)
   - Detect cycles with proper termination

3. **Type consistency**:
   - Duration fields parse correctly
   - Event patterns are valid
   - Filter expressions are syntactically valid

4. **Cross-runbook references** (Phase 5):
   - `runbook.primitive` syntax resolves to valid definitions

**Validator API**:
```rust
pub fn validate_runbook(raw: &RawRunbook) -> Result<ValidatedRunbook, ValidationErrors>;

pub struct ValidationErrors {
    pub errors: Vec<ValidationError>,
}

#[derive(Debug)]
pub enum ValidationError {
    UndefinedReference { kind: &'static str, name: String, referenced_in: String },
    UnreachablePhase { pipeline: String, phase: String },
    CycleWithoutTermination { pipeline: String, phases: Vec<String> },
    InvalidDuration { field: String, value: String },
    InvalidEventPattern { pattern: String, reason: String },
}
```

**Milestone**: Validator catches all intentionally broken runbooks and passes all valid ones.

---

### Phase 4: Template Engine & Input Sources

**Goal**: Implement Jinja2-style templates and shell command output parsing.

**Files**:
- `crates/core/src/runbook/template.rs`
- `crates/core/src/runbook/template_tests.rs`
- `crates/core/src/runbook/input.rs`
- `crates/core/src/runbook/input_tests.rs`

**Template features**:
```rust
pub struct TemplateEngine {
    env: minijinja::Environment<'static>,
}

impl TemplateEngine {
    pub fn new() -> Self;

    /// Render a template string with the given context
    pub fn render(&self, template: &str, context: &Context) -> Result<String, TemplateError>;

    /// Render a template file
    pub fn render_file(&self, path: &Path, context: &Context) -> Result<String, TemplateError>;
}

pub type Context = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Number(i64),
    Bool(bool),
    List(Vec<Value>),
    Object(HashMap<String, Value>),
    Null,
}
```

**Template syntax** (Jinja2 subset):
- Variable interpolation: `{name}`, `{bug.id}`, `{config.labels}`
- Conditionals: `{% if condition %}...{% endif %}`
- Loops: `{% for item in items %}...{% endfor %}`
- Filters: `{name | upper}`, `{list | join(", ")}`
- Default values: `{name | default("unknown")}`

**Input source parsing**:
```rust
pub enum InputFormat {
    Json,
    Lines,      // One item per line
    Csv,        // CSV with header
    KeyValue,   // key=value pairs
}

pub fn parse_input(output: &str, format: InputFormat) -> Result<Vec<Value>, InputError>;

/// Execute command and parse output
pub async fn fetch_input(
    command: &str,
    format: InputFormat,
) -> Result<Vec<Value>, InputError>;
```

**JSON parsing example**:
```rust
// source = "wk list -l bug -s todo --json"
// Returns Vec of issue objects with id, title, description, labels, etc.
```

**Milestone**: Templates render correctly for all example runbook prompts.

---

### Phase 5: Loader & Cross-Runbook References

**Goal**: Convert validated runbooks into runtime types and support cross-runbook references.

**Files**:
- `crates/core/src/runbook/loader.rs`
- `crates/core/src/runbook/loader_tests.rs`
- `crates/core/src/runbook/mod.rs`

**Loader types** (runtime representations):
```rust
pub struct Runbook {
    pub name: String,
    pub commands: HashMap<String, Command>,
    pub workers: HashMap<String, Worker>,
    pub queues: HashMap<String, Queue>,
    pub pipelines: HashMap<String, PipelineDef>,
    pub tasks: HashMap<String, TaskDef>,
    pub guards: HashMap<String, GuardDef>,
    pub strategies: HashMap<String, StrategyDef>,
    pub locks: HashMap<String, LockDef>,
    pub semaphores: HashMap<String, SemaphoreDef>,
    pub config: HashMap<String, Value>,
    pub functions: HashMap<String, String>,
}

pub struct PipelineDef {
    pub name: String,
    pub inputs: Vec<String>,
    pub defaults: HashMap<String, String>,
    pub phases: Vec<PhaseDef>,
}

pub struct PhaseDef {
    pub name: String,
    pub action: PhaseAction,
    pub pre_guards: Vec<String>,
    pub post_guards: Vec<String>,
    pub lock: Option<String>,
    pub semaphore: Option<String>,
    pub next: PhaseNext,
    pub on_fail: FailAction,
}

pub enum PhaseAction {
    Run { command: String },
    Task { name: String },
    Strategy { name: String },
    None,
}

pub enum PhaseNext {
    Phase(String),
    Done,
}

pub enum FailAction {
    Escalate,
    GotoPhase(String),
    UseStrategy(String),
    Retry { max: u32, interval: Duration },
}
```

**Cross-runbook references**:
```rust
pub struct RunbookRegistry {
    runbooks: HashMap<String, Runbook>,
}

impl RunbookRegistry {
    pub fn load_directory(&mut self, dir: &Path) -> Result<(), LoadError>;

    /// Resolve reference like "build.task.planning" or "common.guard.plan_exists"
    pub fn resolve<T>(&self, reference: &str) -> Option<&T>;
}
```

**Reference syntax**:
- Same runbook: `task.planning`, `guard.plan_exists`
- Cross-runbook: `common.task.planning`, `shared.guard.file_exists`

**Milestone**: Loader produces runtime types that match existing `PhaseConfig` structures.

---

### Phase 6: Dynamic Pipelines & Integration

**Goal**: Replace hardcoded pipelines with runbook-driven execution.

**Files**:
- `crates/core/src/pipelines/dynamic.rs`
- `crates/core/src/pipelines/mod.rs` (update)
- `crates/core/src/engine/mod.rs` (integration)
- Remove: `crates/core/src/pipelines/build.rs`, `crates/core/src/pipelines/bugfix.rs`

**Dynamic pipeline creation**:
```rust
impl Pipeline {
    /// Create pipeline from runbook definition
    pub fn from_runbook(
        id: &str,
        def: &PipelineDef,
        inputs: HashMap<String, String>,
        clock: &impl Clock,
    ) -> Pipeline;
}

impl PhaseDef {
    /// Convert to runtime PhaseConfig
    pub fn to_config(&self, template: &TemplateEngine, context: &Context) -> PhaseConfig;
}
```

**Engine integration**:
```rust
pub struct Engine<A: Adapters> {
    // ... existing fields
    registry: RunbookRegistry,
    template: TemplateEngine,
}

impl<A: Adapters> Engine<A> {
    pub fn load_runbooks(&mut self, dir: &Path) -> Result<(), LoadError>;

    pub fn create_pipeline(
        &self,
        runbook: &str,
        pipeline: &str,
        inputs: HashMap<String, String>,
    ) -> Result<Pipeline, CreateError>;
}
```

**Migration steps**:
1. Create `runbooks/` directory with `build.toml`, `bugfix.toml`
2. Update `Engine` to load runbooks on startup
3. Update `oj pipeline create` to use runbook definitions
4. Remove hardcoded `build.rs`, `bugfix.rs` after verification
5. Update all tests to use runbook-based pipelines

**Milestone**: All existing pipeline tests pass with runbook-driven pipelines.

## 5. Key Implementation Details

### Strategy Rollback Semantics

Rollback commands run with the checkpoint value available:
```rust
// Strategy captures checkpoint before first attempt
let checkpoint = run_command(&strategy.checkpoint)?; // "abc123"

// On failure, rollback has access to checkpoint
let rollback_cmd = template.render(
    &attempt.rollback,
    &context!{ checkpoint => checkpoint }
)?; // "git reset --hard abc123"
```

### Template Context Building

Context is built hierarchically:
```rust
fn build_context(pipeline: &Pipeline, def: &PipelineDef) -> Context {
    let mut ctx = Context::new();

    // Pipeline inputs
    for (k, v) in &pipeline.inputs {
        ctx.insert(k.clone(), Value::String(v.clone()));
    }

    // Pipeline defaults (can use inputs via templates)
    for (k, template) in &def.defaults {
        let value = engine.render(template, &ctx)?;
        ctx.insert(k.clone(), Value::String(value));
    }

    // Runtime values
    ctx.insert("phase".into(), Value::String(pipeline.phase.to_string()));

    ctx
}
```

### Guard Integration with Runbooks

Guards defined in runbooks map to existing `GuardCondition`:
```rust
fn load_guard(raw: &RawGuard) -> GuardDef {
    // [guard.plan_exists]
    // condition = "test -f plans/{name}.md"
    // → CustomCheck with templated command
    GuardDef {
        condition: GuardCondition::CustomCheck {
            command: raw.condition.clone(),
            description: raw.name.clone(),
        },
        timeout: raw.timeout,
        on_timeout: raw.on_timeout.clone(),
        wake_on: raw.wake_on.clone(),
    }
}
```

### Phase Transition with Strategy

When a phase uses a strategy:
```rust
[[pipeline.build.phase]]
name = "merge"
strategy = "merge"
lock = "main_branch"
next = "done"
```

The engine:
1. Acquires the lock
2. Creates a `Strategy` instance from the definition
3. Transitions the pipeline to `Phase::Blocked` with the strategy
4. Strategy runs attempts until success or exhaustion
5. On success, pipeline transitions to `next`
6. On exhaust, pipeline handles `on_fail` action

### Input Source Queue Population

Queues with `source` field pull items dynamically:
```rust
[queue.bugs]
source = "wk list -l bug -s todo --json"
filter = "not has_label('assigned')"
```

The engine:
1. Executes `source` command
2. Parses JSON output into `Vec<Value>`
3. Applies filter expression to each item
4. Adds matching items to queue

## 6. Verification Plan

### Unit Tests

Each module has comprehensive unit tests:

**Strategy tests** (`strategy_tests.rs`):
- All state transitions covered
- Property tests for transition determinism
- Timeout handling
- Rollback sequences

**Parser tests** (`parser_tests.rs`):
- Parse all example runbooks successfully
- Handle malformed TOML gracefully
- Preserve all fields correctly

**Validator tests** (`validator_tests.rs`):
- Catch undefined references
- Detect unreachable phases
- Validate duration formats
- Cross-runbook reference resolution

**Template tests** (`template_tests.rs`):
- Variable interpolation
- Nested object access (`{bug.id}`)
- Conditionals and loops
- Filter functions
- Error on undefined variables

**Loader tests** (`loader_tests.rs`):
- Correct mapping to runtime types
- Default value handling
- Cross-runbook references

### Integration Tests

**Pipeline integration** (`tests/runbook_pipeline.rs`):
```rust
#[tokio::test]
async fn build_pipeline_from_runbook() {
    let registry = load_test_runbooks();
    let engine = Engine::new(FakeAdapters::new(), registry);

    let pipeline = engine.create_pipeline(
        "build",
        "build",
        hashmap!{ "name" => "auth", "prompt" => "Add auth" },
    ).unwrap();

    assert_eq!(pipeline.phase, Phase::Init);
    // ... test full pipeline lifecycle
}
```

**Strategy integration** (`tests/strategy_integration.rs`):
```rust
#[tokio::test]
async fn strategy_tries_approaches_in_order() {
    // First attempt fails, second succeeds
}

#[tokio::test]
async fn strategy_rolls_back_on_failure() {
    // Verify rollback command runs with checkpoint
}

#[tokio::test]
async fn strategy_escalates_on_exhaust() {
    // All attempts fail, verify escalation
}
```

### Regression Tests

Ensure existing functionality unchanged:
- All existing `pipeline_tests.rs` tests pass
- All existing `task_tests.rs` tests pass
- All existing `guard_tests.rs` tests pass
- Full `make check` passes

### Example Runbook Validation

Create a test that validates all example runbooks:
```rust
#[test]
fn all_example_runbooks_are_valid() {
    let dir = Path::new("docs/10-example-runbooks");
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension() == Some("toml".as_ref()) {
            let content = fs::read_to_string(&path).unwrap();
            let raw = parse_runbook(&content).expect(&format!("Parse {}", path.display()));
            validate_runbook(&raw).expect(&format!("Validate {}", path.display()));
        }
    }
}
```

### Pre-commit Verification

Before committing each phase:
```bash
./checks/lint.sh
make check   # fmt, clippy, test, build, audit, deny
```

# GitHub CI/CD Implementation Plan

## Overview

Implement comprehensive GitHub Actions CI/CD for the Otter Jobs (oj) Rust workspace. This includes automated testing across multiple platforms and Rust versions, code quality checks (formatting, linting, security auditing), test coverage tracking toward the 90%+ goal (Epic 9), and release automation for binary distribution and crates.io publishing.

The project is a Rust workspace with two crates:
- **oj** (CLI binary) - Command-line interface
- **oj-core** (library) - Core state machines and engine

## Project Structure

```
.github/
├── workflows/
│   ├── ci.yml              # Main CI workflow (build, test, lint)
│   ├── security.yml        # Dependency audit and security checks
│   ├── coverage.yml        # Code coverage tracking
│   └── release.yml         # Release automation
├── dependabot.yml          # Automated dependency updates
└── CODEOWNERS              # Code review requirements

# Supporting configuration files (workspace root)
rustfmt.toml                # Formatting standards
clippy.toml                 # Clippy configuration
deny.toml                   # Dependency checking rules
```

## Dependencies

### GitHub Actions

| Action | Purpose |
|--------|---------|
| `actions/checkout@v4` | Repository checkout |
| `dtolnay/rust-toolchain@stable` | Rust toolchain installation |
| `Swatinem/rust-cache@v2` | Cargo dependency caching |
| `taiki-e/install-action@v2` | Install cargo tools (cargo-deny, cargo-tarpaulin) |
| `codecov/codecov-action@v4` | Coverage upload to Codecov |
| `softprops/action-gh-release@v2` | GitHub release creation |

### Cargo Tools

| Tool | Purpose |
|------|---------|
| `cargo-deny` | Dependency license/security checking |
| `cargo-tarpaulin` | Code coverage (Linux) |
| `cargo-audit` | Security vulnerability scanning |

## Implementation Phases

### Phase 1: Core CI Workflow

**Goal:** Establish basic build and test automation on every push/PR.

**Files to create:**
- `.github/workflows/ci.yml`
- `rustfmt.toml`

**Workflow: ci.yml**

```yaml
name: CI

on:
  push:
    branches: [main, "feature/**"]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests
        run: cargo test --all --verbose
      - name: Run integration tests (single-threaded)
        run: cargo test --all --verbose -- --test-threads=1 --ignored
        continue-on-error: true

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --all
```

**rustfmt.toml:**

```toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
```

**Verification:**
- Push to feature branch
- Verify all jobs pass
- Intentionally break formatting to confirm check fails

---

### Phase 2: Security and Dependency Checks

**Goal:** Automate security vulnerability scanning and dependency policy enforcement.

**Files to create:**
- `.github/workflows/security.yml`
- `deny.toml`
- `.github/dependabot.yml`

**Workflow: security.yml**

```yaml
name: Security

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  schedule:
    - cron: "0 0 * * 0"  # Weekly on Sunday

jobs:
  audit:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-audit
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-audit
      - name: Run audit
        run: cargo audit

  deny:
    name: Dependency Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-deny
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-deny
      - name: Check dependencies
        run: cargo deny check
```

**deny.toml:**

```toml
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "Unicode-DFS-2016",
]
copyleft = "warn"

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

**dependabot.yml:**

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      rust-dependencies:
        patterns:
          - "*"
    commit-message:
      prefix: "deps"

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    commit-message:
      prefix: "ci"
```

**Verification:**
- Run `cargo deny check` locally
- Verify weekly schedule trigger works (check Actions tab)
- Confirm Dependabot creates PRs for outdated deps

---

### Phase 3: Code Coverage

**Goal:** Track test coverage and enforce minimum thresholds, supporting the 90%+ coverage goal from Epic 9.

**Files to create:**
- `.github/workflows/coverage.yml`
- `codecov.yml` (optional, for Codecov configuration)

**Workflow: coverage.yml**

```yaml
name: Coverage

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Install cargo-tarpaulin
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-tarpaulin

      - name: Generate coverage
        run: |
          cargo tarpaulin \
            --workspace \
            --out xml \
            --out html \
            --skip-clean \
            --timeout 300 \
            --exclude-files "*/tests/*" \
            --exclude-files "*/main.rs"

      - name: Upload to Codecov
        uses: codecov/codecov-action@v4
        with:
          files: cobertura.xml
          fail_ci_if_error: false
        env:
          CODECOV_TOKEN: ${{ secrets.CODECOV_TOKEN }}

      - name: Upload HTML report
        uses: actions/upload-artifact@v4
        with:
          name: coverage-report
          path: tarpaulin-report.html
          retention-days: 14
```

**Codecov configuration (codecov.yml in repo root):**

```yaml
coverage:
  status:
    project:
      default:
        target: 80%
        threshold: 2%
    patch:
      default:
        target: 80%

comment:
  layout: "reach,diff,flags,files"
  behavior: default
```

**Verification:**
- Sign up for Codecov and add `CODECOV_TOKEN` secret
- Open PR and verify coverage comment appears
- Check coverage report artifact is uploaded

---

### Phase 4: Release Automation

**Goal:** Automate binary builds and GitHub releases on version tags.

**Files to create:**
- `.github/workflows/release.yml`

**Workflow: release.yml**

```yaml
name: Release

on:
  push:
    tags:
      - "v[0-9]+.*"

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build-release:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            archive: tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: tar.gz

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }} -p oj

      - name: Package binary
        shell: bash
        run: |
          cd target/${{ matrix.target }}/release
          tar czvf ../../../oj-${{ github.ref_name }}-${{ matrix.target }}.${{ matrix.archive }} oj
          cd ../../..
          sha256sum oj-${{ github.ref_name }}-${{ matrix.target }}.${{ matrix.archive }} > oj-${{ github.ref_name }}-${{ matrix.target }}.${{ matrix.archive }}.sha256

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: oj-${{ matrix.target }}
          path: |
            oj-${{ github.ref_name }}-${{ matrix.target }}.*

  create-release:
    name: Create Release
    needs: build-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          draft: true
          generate_release_notes: true
          files: artifacts/*
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Verification:**
- Create and push a test tag: `git tag v0.1.0-test && git push origin v0.1.0-test`
- Verify release job runs and creates draft release
- Check binaries are attached with checksums
- Delete test tag: `git tag -d v0.1.0-test && git push origin :v0.1.0-test`

---

### Phase 5: Branch Protection and CODEOWNERS

**Goal:** Enforce CI checks and code review requirements.

**Files to create:**
- `.github/CODEOWNERS`

**CODEOWNERS:**

```
# Default owners for everything
* @kestred

# Core state machines require careful review
/crates/core/src/*.rs @kestred
/crates/core/src/engine/ @kestred
```

**Branch Protection Rules (manual setup in GitHub):**

Configure via Settings > Branches > Add rule for `main`:

1. **Require a pull request before merging**
   - Require approvals: 1 (optional for solo projects)

2. **Require status checks to pass before merging**
   - Required checks:
     - `Format`
     - `Clippy`
     - `Test (ubuntu-latest)`
     - `Test (macos-latest)`
     - `Build`
     - `Dependency Check`

3. **Require branches to be up to date before merging**

4. **Do not allow bypassing the above settings**

**Verification:**
- Create a PR that fails formatting
- Verify merge is blocked
- Fix formatting and confirm merge is allowed

---

### Phase 6: Documentation (Optional)

**Goal:** Auto-generate and publish Rust documentation.

**Workflow: docs.yml (optional)**

```yaml
name: Docs

on:
  push:
    branches: [main]

permissions:
  contents: read
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Build documentation
        run: cargo doc --no-deps --workspace

      - name: Add redirect
        run: echo '<meta http-equiv="refresh" content="0; url=oj_core/">' > target/doc/index.html

      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: target/doc

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

**Verification:**
- Enable GitHub Pages in repo settings (Source: GitHub Actions)
- Push to main and verify docs are published
- Check documentation renders correctly

## Key Implementation Details

### Caching Strategy

The `Swatinem/rust-cache@v2` action caches:
- `~/.cargo/registry/index/`
- `~/.cargo/registry/cache/`
- `~/.cargo/git/db/`
- `./target/`

Cache key is based on `Cargo.lock` hash, ensuring cache invalidation on dependency changes.

### Test Threading

Some integration tests (tmux, git adapters) require single-threaded execution:

```yaml
# Run normal tests in parallel
cargo test --all --verbose

# Run integration tests single-threaded
cargo test --all -- --test-threads=1 --ignored
```

### Platform Matrix

| Platform | Rationale |
|----------|-----------|
| `ubuntu-latest` | Primary CI, fastest, most tools available |
| `macos-latest` | Production target (project uses osascript for notifications) |
| Windows | Excluded - project uses Unix-specific features (tmux) |

### Proptest Considerations

Property-based tests may generate many test cases. The coverage workflow uses `--timeout 300` to accommodate longer test runs.

### Release Versioning

Tags must follow semantic versioning pattern: `v[0-9]+.*`

Examples:
- `v0.1.0` - Initial release
- `v0.1.1` - Patch release
- `v1.0.0` - Major release

## Verification Plan

### Phase 1 Verification
- [ ] Push to feature branch triggers CI
- [ ] All jobs (fmt, clippy, test, build) pass
- [ ] Formatting failure blocks merge
- [ ] Clippy warnings cause failure

### Phase 2 Verification
- [ ] `cargo deny check` passes locally
- [ ] Security workflow runs on schedule
- [ ] Dependabot creates PRs for updates

### Phase 3 Verification
- [ ] Coverage report generates successfully
- [ ] Codecov comment appears on PRs
- [ ] HTML artifact is downloadable

### Phase 4 Verification
- [ ] Tag push triggers release workflow
- [ ] Binaries built for all targets
- [ ] Draft release created with artifacts
- [ ] Checksums included

### Phase 5 Verification
- [ ] Branch protection blocks direct pushes
- [ ] Required checks must pass before merge
- [ ] CODEOWNERS assigns reviewers

### Phase 6 Verification (Optional)
- [ ] Documentation builds on push to main
- [ ] GitHub Pages serves docs
- [ ] All crate docs are accessible

## File Summary

| File | Phase | Priority |
|------|-------|----------|
| `.github/workflows/ci.yml` | 1 | Required |
| `rustfmt.toml` | 1 | Required |
| `.github/workflows/security.yml` | 2 | Required |
| `deny.toml` | 2 | Required |
| `.github/dependabot.yml` | 2 | Required |
| `.github/workflows/coverage.yml` | 3 | Required |
| `codecov.yml` | 3 | Optional |
| `.github/workflows/release.yml` | 4 | Required |
| `.github/CODEOWNERS` | 5 | Optional |
| `.github/workflows/docs.yml` | 6 | Optional |

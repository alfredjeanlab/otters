#!/bin/bash
# scripts/migrate-from-bash.sh
#
# Migrate from bash scripts (feature, bugfix, mergeq) to oj
#
# This script:
# 1. Checks for running bash-based processes
# 2. Migrates any existing queue items
# 3. Backs up old state
# 4. Initializes oj state directory
#
# Usage:
#   ./scripts/migrate-from-bash.sh [--dry-run]

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=true
    echo -e "${YELLOW}[DRY RUN]${NC} No changes will be made"
    echo ""
fi

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

echo "=== Migrating from Bash Scripts to oj ==="
echo ""

# Check for running bash-based processes
info "Checking for running bash processes..."
RUNNING_PROCESSES=$(pgrep -f "mergeq|feature-daemon|bugfix-daemon" 2>/dev/null || true)

if [[ -n "$RUNNING_PROCESSES" ]]; then
    error "Found running bash processes. Stop them first:"
    echo "  pkill -f mergeq"
    echo "  pkill -f feature-daemon"
    echo "  pkill -f bugfix-daemon"
    echo ""
    echo "Process IDs: $RUNNING_PROCESSES"
    exit 1
fi

info "No running bash processes found."
echo ""

# Define paths
BASH_STATE_DIR="${HOME}/.feature-state"
OJ_STATE_DIR="${PWD}/.build"

# Check for existing bash state
if [[ -d "$BASH_STATE_DIR" ]]; then
    info "Found bash script state in $BASH_STATE_DIR"
    echo ""

    # List what we found
    if [[ -f "$BASH_STATE_DIR/merge-queue.json" ]]; then
        QUEUE_ITEMS=$(cat "$BASH_STATE_DIR/merge-queue.json" 2>/dev/null | jq length 2>/dev/null || echo "0")
        info "  Found merge queue with $QUEUE_ITEMS items"
    fi

    if [[ -d "$BASH_STATE_DIR/worktrees" ]]; then
        WORKTREE_COUNT=$(ls -1 "$BASH_STATE_DIR/worktrees" 2>/dev/null | wc -l | tr -d ' ')
        info "  Found $WORKTREE_COUNT worktree references"
    fi

    echo ""

    # Migrate queue items
    if [[ -f "$BASH_STATE_DIR/merge-queue.json" ]] && [[ "$QUEUE_ITEMS" -gt 0 ]]; then
        info "Migrating merge queue..."

        if [[ "$DRY_RUN" == "true" ]]; then
            echo "  Would run: oj queue import $BASH_STATE_DIR/merge-queue.json --format legacy"
        else
            if command -v oj &> /dev/null; then
                oj queue import "$BASH_STATE_DIR/merge-queue.json" --format legacy 2>/dev/null || {
                    warn "Queue import failed (may not be supported yet)"
                    warn "Manual migration may be required"
                }
            else
                warn "oj binary not found, skipping queue migration"
            fi
        fi
    fi

    # Note about worktrees
    info "Worktrees are compatible, no migration needed"
    info "  Existing git worktrees will be detected automatically"

    # Backup and archive old state
    BACKUP_DIR="${BASH_STATE_DIR}.backup.$(date +%Y%m%d-%H%M%S)"
    info "Backing up old state to $BACKUP_DIR"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  Would run: mv $BASH_STATE_DIR $BACKUP_DIR"
    else
        mv "$BASH_STATE_DIR" "$BACKUP_DIR"
        info "  Backup created: $BACKUP_DIR"
    fi

    echo ""
else
    info "No existing bash state found in $BASH_STATE_DIR"
    echo ""
fi

# Initialize oj state
info "Initializing oj state directory..."

if [[ "$DRY_RUN" == "true" ]]; then
    echo "  Would create: $OJ_STATE_DIR"
    echo "  Would run: oj init"
else
    mkdir -p "$OJ_STATE_DIR"

    if command -v oj &> /dev/null; then
        oj init 2>/dev/null || true
        info "  State directory initialized"
    else
        warn "oj binary not found, creating empty state directory"
    fi
fi

echo ""

# Verify installation (if not dry run)
if [[ "$DRY_RUN" == "false" ]] && command -v oj &> /dev/null; then
    info "Verifying installation..."
    oj status 2>/dev/null || warn "oj status returned non-zero (may be expected on fresh install)"
fi

echo ""
echo "=== Migration Complete ==="
echo ""
echo "Next steps:"
echo "  1. Build oj: cargo build --release"
echo "  2. Add to PATH: export PATH=\"\$PATH:$(pwd)/target/release\""
echo "  3. Start the daemon: oj daemon start"
echo "  4. Run a test pipeline: oj run build --dry-run"
echo ""
echo "Shell aliases for backward compatibility:"
echo "  alias feature='oj run feature'"
echo "  alias bugfix='oj run bugfix'"
echo "  alias mergeq='oj queue'"
echo ""
echo "Add these to your shell profile (~/.bashrc or ~/.zshrc)"

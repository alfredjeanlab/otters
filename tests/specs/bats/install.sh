#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# Copyright (c) 2026 Alfred Jean LLC
#
# Download and install pinned versions of BATS testing libraries.
# Designed for reproducible builds - no homebrew dependency.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Pinned versions for reproducibility
BATS_CORE_VERSION="v1.13.0"
BATS_SUPPORT_VERSION="v0.3.0"
BATS_ASSERT_VERSION="v2.1.0"

# Download and extract a GitHub release tarball
# Usage: download_library <name> <version>
download_library() {
    local name="$1"
    local version="$2"
    local target_dir="$SCRIPT_DIR/$name"

    if [[ -d "$target_dir" ]]; then
        echo "  $name already installed"
        return 0
    fi

    local url="https://github.com/bats-core/$name/archive/refs/tags/$version.tar.gz"
    local temp_file
    temp_file=$(mktemp)
    local temp_dir
    temp_dir=$(mktemp -d)

    echo "  Downloading $name $version..."

    # Use curl or wget, whichever is available
    if command -v curl &>/dev/null; then
        curl -fsSL "$url" -o "$temp_file"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "$temp_file"
    else
        echo "Error: neither curl nor wget found" >&2
        exit 1
    fi

    # Extract to temp directory
    tar -xzf "$temp_file" -C "$temp_dir"

    # Move extracted directory to target (strips version suffix from dir name)
    mv "$temp_dir"/"$name"-*/ "$target_dir"

    # Cleanup
    rm -f "$temp_file"
    rm -rf "$temp_dir"

    echo "  $name $version installed"
}

main() {
    echo "Installing BATS libraries to $SCRIPT_DIR"
    echo ""

    download_library "bats-core" "$BATS_CORE_VERSION"
    download_library "bats-support" "$BATS_SUPPORT_VERSION"
    download_library "bats-assert" "$BATS_ASSERT_VERSION"

    echo ""
    echo "BATS installation complete"
}

main "$@"

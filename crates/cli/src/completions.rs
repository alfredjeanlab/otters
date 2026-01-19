// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2026 Alfred Jean LLC

//! Shell completion generation for the oj CLI.
//!
//! Generates shell completions for bash, zsh, fish, and powershell.
//! Install completions using:
//!
//! ```bash
//! # Bash
//! oj completions bash > ~/.local/share/bash-completion/completions/oj
//!
//! # Zsh
//! oj completions zsh > ~/.zfunc/_oj
//!
//! # Fish
//! oj completions fish > ~/.config/fish/completions/oj.fish
//!
//! # PowerShell
//! oj completions powershell > $PROFILE.CurrentUserAllHosts
//! ```

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

/// Generate shell completions and write to stdout.
pub fn generate_completions<C: CommandFactory>(shell: Shell) {
    let mut cmd = C::command();
    generate(shell, &mut cmd, "oj", &mut io::stdout());
}

/// Arguments for the completions command.
#[derive(clap::Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

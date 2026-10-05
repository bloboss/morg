//! `morg completions <shell>` — emit a completion script to stdout.
//!
//! The script is generated from the clap definition in `cli.rs`, so it
//! always covers the current set of subcommands and flags. Install it
//! wherever the shell expects (e.g. a zsh `fpath` directory as `_morg`).

use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;

pub fn run(shell: Shell) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "morg", &mut std::io::stdout());
    Ok(())
}

mod cli;
mod collect;
mod commands;
mod config;
mod report;
mod util;

use std::path::PathBuf;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    let json = cli.format == "json";
    let cfg = config::load();

    // Helper: if files list is empty, fall back to config.root
    let default_files = |files: Vec<PathBuf>| -> Vec<PathBuf> {
        if files.is_empty() {
            vec![cfg.root.clone()]
        } else {
            files
        }
    };

    let result = match cli.command {
        Command::Init => config::init_config(),
        Command::Diary { no_edit } => commands::diary::run(&cfg, no_edit),
        Command::Tangle { files, output_dir } => {
            commands::tangle::run(&default_files(files), output_dir.as_deref())
        }
        Command::Time { files, project } => {
            commands::time::run(&default_files(files), project.as_deref())
        }
        Command::Todos { files } => commands::todos::run(&default_files(files), json),
        Command::Meals { files } => commands::meals::run(&default_files(files), json),
        Command::Agenda { files } => commands::agenda::run(&default_files(files), json),
        Command::Frontmatter { files } => commands::frontmatter::run(&default_files(files)),
        Command::Ical { files, output } => {
            commands::ical::run(&default_files(files), output.as_deref())
        }
        Command::Export {
            files,
            output,
            standalone,
        } => commands::export::run(&default_files(files), output.as_deref(), standalone),
        Command::Columns { files, columns } => {
            commands::columns::run(&default_files(files), &columns)
        }
        Command::Capture { template, input } => commands::capture::run(&template, &input),
        Command::Lint { files } => commands::lint::run(&default_files(files), json),
        Command::Refs { files } => commands::refs::run(&default_files(files)),
        Command::Refile {
            source,
            target,
            dry_run,
        } => commands::refile::run(&source, &target, dry_run),
        Command::Id { files, dry_run } => commands::id::run(&default_files(files), dry_run),
        Command::Search {
            files,
            query,
            tags_only,
            headings_only,
        } => commands::search::run(
            &default_files(files),
            &query,
            tags_only,
            headings_only,
            json,
        ),
        Command::Archive {
            files,
            suffix,
            dry_run,
        } => commands::archive::run(&default_files(files), &suffix, dry_run),
        Command::Watch {
            files,
            command,
            output_dir,
        } => commands::watch::run(&default_files(files), output_dir.as_deref(), &command),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

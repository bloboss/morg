use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::collect;
use crate::commands;

pub fn run(
    paths: &[PathBuf],
    output_dir: Option<&Path>,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve watch targets
    let watch_paths = collect::resolve_paths(paths);
    if watch_paths.is_empty() {
        return Err("No markdown files found to watch.".into());
    }

    // Determine which directories to watch
    let mut watch_dirs: Vec<PathBuf> = Vec::new();
    for path in &watch_paths {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if !watch_dirs.contains(&dir) {
            watch_dirs.push(dir);
        }
    }

    // Run the command once initially
    eprintln!(
        "morg watch: running `{command}` on {} file(s)...",
        watch_paths.len()
    );
    run_command(command, paths, output_dir);

    // Set up file watcher
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    for dir in &watch_dirs {
        watcher.watch(dir, RecursiveMode::Recursive)?;
        eprintln!("morg watch: watching {}", dir.display());
    }

    eprintln!("morg watch: waiting for changes (Ctrl+C to stop)...");

    // Debounce: wait for events to settle before re-running
    let debounce = Duration::from_millis(200);
    let mut last_run = Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(event)) => {
                // Only react to modify/create events on markdown files
                if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    continue;
                }

                let is_markdown = event.paths.iter().any(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("md" | "morg" | "markdown")
                    )
                });

                if !is_markdown {
                    continue;
                }

                // Debounce: skip if we ran very recently
                if last_run.elapsed() < debounce {
                    continue;
                }

                let changed: Vec<_> = event
                    .paths
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                eprintln!("\nmorg watch: change detected in {}", changed.join(", "));

                run_command(command, paths, output_dir);
                last_run = Instant::now();
            }
            Ok(Err(e)) => {
                eprintln!("morg watch: watcher error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No events — just loop
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("File watcher disconnected.".into());
            }
        }
    }
}

fn run_command(command: &str, paths: &[PathBuf], output_dir: Option<&Path>) {
    let result = match command {
        "tangle" => commands::tangle::run(paths, output_dir),
        "todos" => commands::todos::run(paths, false),
        "agenda" => commands::agenda::run(paths, false),
        "media" => commands::media::run(paths, false, None, None),
        "time" => commands::time::run(paths, None),
        "frontmatter" => commands::frontmatter::run(paths),
        other => {
            eprintln!("morg watch: unknown command `{other}`, running tangle");
            commands::tangle::run(paths, output_dir)
        }
    };

    if let Err(e) = result {
        eprintln!("morg watch: command error: {e}");
    }
}

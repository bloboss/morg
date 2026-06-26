use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "morg",
    about = "Markdown org-mode — tangle, track, and organize"
)]
pub struct Cli {
    /// Output format: text (default) or json
    #[arg(long, global = true, default_value = "text")]
    pub format: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize morg config directory and default config.toml
    Init,
    /// Open today's diary note (rotate, archive, carry todos)
    Diary {
        /// Don't open in $EDITOR, just print the path
        #[arg(long)]
        no_edit: bool,
    },
    /// Extract tagged code blocks and write them to files
    Tangle {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Output directory (default: relative to source file)
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    /// Generate time tracking report
    Time {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Filter by project (heading name)
        #[arg(long)]
        project: Option<String>,
    },
    /// List all TODOs across files
    Todos {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
    },
    /// Show deadlines, dates, and events chronologically
    Agenda {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
    },
    /// List #media items grouped into to-read/watch/listen lists
    Media {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Filter by status: todo, active, or done
        #[arg(long)]
        status: Option<String>,
        /// Filter by list: read, watch, listen, play, or other
        #[arg(long)]
        category: Option<String>,
    },
    /// List #purchase entries, grouped by category with totals
    Purchases {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
    },
    /// Aggregate YAML frontmatter across files
    Frontmatter {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
    },
    /// Export todos and agenda entries to iCalendar (.ics) format
    Ical {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Output .ics file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Export markdown to HTML
    Export {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Output HTML file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Generate standalone HTML with embedded CSS
        #[arg(long, default_value_t = true)]
        standalone: bool,
    },
    /// Show tabular column view of headings with properties
    Columns {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Columns to display (comma-separated: item,todo,priority,effort,deadline)
        #[arg(short, long, default_value = "item,todo,priority,effort,deadline")]
        columns: String,
    },
    /// Capture a new entry using a template
    Capture {
        /// Template name (from ~/.config/morg/capture.yaml)
        template: String,
        /// Content to capture
        input: String,
    },
    /// Validate and list cross-file references
    Refs {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
    },
    /// Search across files by tags, text, or heading content
    Search {
        /// Search query (tag name, text, or regex)
        query: String,
        /// Markdown files or directories to search
        files: Vec<PathBuf>,
        /// Search only in tags
        #[arg(long)]
        tags_only: bool,
        /// Search only in headings
        #[arg(long)]
        headings_only: bool,
    },
    /// Lint markdown files for common issues
    Lint {
        /// Markdown files or directories to lint
        files: Vec<PathBuf>,
    },
    /// Move a heading subtree to another file/location
    Refile {
        /// Source: file:line or file::heading-text
        source: String,
        /// Target: file or file::heading-text
        target: String,
        /// Dry run — show what would be moved without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Assign UUIDs to headings that lack an id property
    Id {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Dry run — show what would be changed
        #[arg(long)]
        dry_run: bool,
    },
    /// Move #archive subtrees from source files into archive files
    Archive {
        /// Markdown files or directories to process
        files: Vec<PathBuf>,
        /// Suffix for archive files (default: _archive)
        #[arg(long, default_value = "_archive")]
        suffix: String,
        /// Dry run — show what would be archived without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Watch files and re-run a command on changes
    Watch {
        /// Markdown files or directories to watch
        files: Vec<PathBuf>,
        /// Command to run on changes (tangle, todos, agenda, media, purchases, time, frontmatter)
        #[arg(short, long, default_value = "tangle")]
        command: String,
        /// Output directory for tangle command
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

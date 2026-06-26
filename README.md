# morg-mode

A markdown-idiomatic replacement for Emacs org-mode, built in Rust.

morg-mode extends standard markdown with a `#tag` system for metadata, time tracking, task management, literate programming, and personal knowledge management. It operates purely on source files and is designed for use with any editor.

[![CI](https://github.com/YOUR_USER/morg-mode/actions/workflows/ci.yml/badge.svg)](https://github.com/YOUR_USER/morg-mode/actions/workflows/ci.yml)

## Features

- **Tag system** -- `#todo`, `#deadline`, `#scheduled`, `#clock`, `#priority`, `#effort`, `#archive`, `#media`, and more. Tags are inline (`text #todo fix this`) or block-level (`#deadline 2026-04-10`).
- **Code tangling** -- Extract tagged code blocks into standalone files with `#tangle file=path`. Supports noweb references (`<<block-name>>`), indent preservation, and recursive expansion.
- **Time tracking** -- `#clock-in`/`#clock-out` pairs and `#clock 1h30m` durations, aggregated into per-heading reports.
- **Todo management** -- `#todo`/`#done` tags, checkbox lists, custom workflow sequences, priorities, and effort estimates. Aggregated across files.
- **Media tracking** -- `#media` tags for books, movies, albums, games, and more. Structured fields (creator, status, rating, year) aggregated into to-read/to-watch/to-listen lists.
- **Purchase tracking** -- `#purchase` tags with structured `price`, `category`, and `qty` attributes, aggregated into a per-category purchases list with totals.
- **Agenda** -- Deadlines, scheduled items, and events with recurring timestamps (`+1w`, `+1m`), warning periods (`-3d`), and time-of-day support.
- **Property drawers** -- Per-heading key-value metadata via `#properties`/`#end` blocks.
- **Diary** -- Daily note rotation with template stamping, date-based archiving, and automatic todo carry-over.
- **Capture** -- Quick entry via configurable YAML templates.
- **Literate programming** -- Named code blocks, noweb references, callout tangling.
- **Cross-file references** -- `id:` links validated against property drawer IDs.
- **Export** -- Standalone HTML with embedded CSS, tag badges, callout styling, footnotes.
- **iCalendar** -- Export todos and agenda to `.ics` with `RRULE` for recurring items.
- **Neovim plugin** -- Commands, keybindings, Telescope pickers, LuaSnip snippets, diagnostics.

## Installation

### From source

```sh
cargo install --path crates/morg-mode
```

This installs the `morg` binary to `~/.cargo/bin/`.

### Initialize config

```sh
morg init
```

Creates `~/.config/morg/config.toml` with documented defaults.

## Quick Start

```sh
# Open today's diary note (creates from template if needed)
morg diary

# List all TODOs across your morg directory
morg todos

# Show agenda (deadlines, scheduled, events)
morg agenda

# Tangle code blocks from a file
morg tangle notes/project.md

# Search across all files
morg search "parser"

# Lint for issues
morg lint notes/

# Export to HTML
morg export notes/project.md -o project.html
```

## Tag Syntax

Tags are prefixed with `#` (no space -- a space after `#` makes it a heading). Escape with `\#`.

```markdown
# Project Notes

#todo refactor the parser #priority A #effort 2h
#deadline 2026-04-15 +1w -3d
#scheduled 2026-04-10T09:00

## Time Log

#clock-in 2026-04-03T09:00
#clock-out 2026-04-03T10:30
#clock 1h30m
```

## CLI Commands

| Command | Description |
|---|---|
| `morg init` | Create default config |
| `morg diary` | Open/rotate daily note |
| `morg tangle` | Extract code blocks to files |
| `morg todos` | List TODOs (quickfix-friendly) |
| `morg agenda` | Chronological deadlines/events |
| `morg media` | To-read/watch/listen lists from `#media` tags |
| `morg purchases` | Aggregated purchase list with totals |
| `morg time` | Time tracking report |
| `morg search` | Full-text and tag search |
| `morg lint` | Validate documents |
| `morg export` | Markdown to HTML |
| `morg ical` | Export to iCalendar |
| `morg columns` | Tabular heading view |
| `morg capture` | Quick entry from template |
| `morg refs` | Validate cross-file references |
| `morg refile` | Move headings between files |
| `morg archive` | Extract archived subtrees |
| `morg id` | Assign UUIDs to headings |
| `morg watch` | File watcher for auto-tangle |
| `morg frontmatter` | Aggregate YAML frontmatter |

All commands accept `--format json` for machine-readable output. When no files are specified, commands default to the `root` directory from config.

## Configuration

`~/.config/morg/config.toml`:

```toml
# Root directory -- default search path for all commands
root = "~/.morg"

[diary]
directory = "~/.diary"
template = "~/.diary/daily_note.template"
today_file = "today.md"
archive_pattern = "{year}/{month}"
archive_filename = "{day}.md"
carry_todos = true

[capture]
templates_file = "~/.config/morg/capture.yaml"
```

## Neovim Plugin

See [`morg-mode-nvim/`](morg-mode-nvim/) for the Neovim integration:

- 14 `:Morg*` commands (async, never blocks)
- 5 Telescope pickers (todos, agenda, search, tags, headings)
- 30 LuaSnip snippets
- Buffer-local keybindings (checkbox toggle, tag insertion, heading promote/demote)
- Auto-lint on save (populates `vim.diagnostic`)
- `:checkhealth morg`

## Architecture

```
morg-mode/
  crates/
    morg-parser/         # Hand-written markdown+tag parser
      tokens.rs          # define_keywords! macro (single source of truth)
      lexer.rs           # Block tokenizer + inline tokenizer
      parser.rs          # Token-consuming recursive descent
      ast.rs             # 12 Block variants, 8 InlineSegment variants
      tags.rs            # Strongly-typed TagKind enum
    morg-mode/           # CLI binary (18 commands)
      config.rs          # TOML configuration
      commands/           # One module per command
  morg-mode-nvim/        # Neovim plugin (Lua)
  docs/                  # mdbook documentation
```

### Parser design

- **Keywords** defined once via `define_keywords!` macro -- adding a tag requires one line
- **Lexer** classifies lines into block tokens, with on-demand inline tokenization
- **Parser** consumes the token stream to build a typed AST
- **Lenient** -- parse errors are collected, not fatal. Partial documents still produce useful results.

## Documentation

Full documentation is built with [mdbook](https://rust-lang.github.io/mdBook/) from the `docs/` directory:

```sh
cd docs && mdbook serve
```

Includes syntax reference, feature comparison with org-mode, and detailed roadmaps.

## Development

```sh
# Run Rust tests (82 tests)
cargo test --workspace

# Run Neovim plugin tests (30 tests)
cd morg-mode-nvim
eval "$(luarocks path --bin --lua-version 5.1)" && busted .

# Build docs locally
cd docs && mdbook build
```

## License

MIT

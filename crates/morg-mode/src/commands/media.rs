use std::path::PathBuf;

use morg_parser::ast::Heading;
use morg_parser::tags::{MediaCategory, MediaKind, MediaStatus, TagKind};

use crate::collect::{self, TagContext};
use crate::report;

pub fn run(
    paths: &[PathBuf],
    json: bool,
    status_filter: Option<&str>,
    category_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status_filter = match status_filter {
        Some(s) => match MediaStatus::parse(s) {
            Some(st) => Some(st),
            None => {
                return Err(format!(
                    "unknown --status '{s}' (expected todo, active, or done)"
                )
                .into());
            }
        },
        None => None,
    };

    let category_filter = match category_filter {
        Some(c) => match parse_category(c) {
            Some(cat) => Some(cat),
            None => {
                return Err(format!(
                    "unknown --category '{c}' (expected read, watch, listen, play, or other)"
                )
                .into());
            }
        },
        None => None,
    };

    let parsed = collect::parse_files(paths);
    let mut entries: Vec<MediaEntry> = Vec::new();

    for pf in &parsed {
        collect::walk_tags(&pf.path, &pf.document, |ctx: TagContext<'_>| {
            if ctx.is_archived {
                return;
            }
            if let TagKind::Media {
                kind,
                title,
                creator,
                status,
                rating,
                year,
            } = &ctx.tag.kind
            {
                entries.push(MediaEntry {
                    category: kind.category(),
                    kind: kind.clone(),
                    title: title.clone().unwrap_or_default(),
                    creator: creator.clone(),
                    status: *status,
                    rating: *rating,
                    year: *year,
                    location: report::format_location(ctx.file, &ctx.tag.span),
                    heading: ctx.parent_heading.map(heading_text),
                });
            }
        });
    }

    entries.retain(|e| {
        status_filter.is_none_or(|s| e.status == s)
            && category_filter.is_none_or(|c| e.category == c)
    });

    // Sort by category (display order), then status (todo first), then title.
    entries.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then(a.status.cmp(&b.status))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    if json {
        let items: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                let (file, lnum) = parse_location(&e.location);
                serde_json::json!({
                    "category": e.category.label(),
                    "kind": e.kind.as_str(),
                    "title": e.title,
                    "creator": e.creator,
                    "status": e.status.as_str(),
                    "rating": e.rating,
                    "year": e.year,
                    "heading": e.heading,
                    "file": file,
                    "line": lnum,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("No media items found.");
        return Ok(());
    }

    let mut first = true;
    for category in MediaCategory::all() {
        let in_cat: Vec<&MediaEntry> =
            entries.iter().filter(|e| e.category == *category).collect();
        if in_cat.is_empty() {
            continue;
        }
        if !first {
            println!();
        }
        first = false;
        println!("{}", category.label());
        for entry in in_cat {
            println!("  {}", format_entry(entry));
        }
    }

    let total = entries.len();
    let todo = entries.iter().filter(|e| e.status == MediaStatus::Todo).count();
    let active = entries
        .iter()
        .filter(|e| e.status == MediaStatus::Active)
        .count();
    let done = entries.iter().filter(|e| e.status == MediaStatus::Done).count();
    println!("\n{todo} queued, {active} in progress, {done} done, {total} total");

    Ok(())
}

struct MediaEntry {
    category: MediaCategory,
    kind: MediaKind,
    title: String,
    creator: Option<String>,
    status: MediaStatus,
    rating: Option<u8>,
    year: Option<i32>,
    location: String,
    heading: Option<String>,
}

fn format_entry(entry: &MediaEntry) -> String {
    let mark = match entry.status {
        MediaStatus::Todo => "[ ]",
        MediaStatus::Active => "[~]",
        MediaStatus::Done => "[x]",
    };
    let title = if entry.title.is_empty() {
        "(untitled)".to_string()
    } else {
        entry.title.clone()
    };
    let creator = entry
        .creator
        .as_deref()
        .map(|c| format!(" — {c}"))
        .unwrap_or_default();
    let year = entry
        .year
        .map(|y| format!(" ({y})"))
        .unwrap_or_default();
    let rating = entry
        .rating
        .map(|r| format!(" ★{r}"))
        .unwrap_or_default();
    let heading = entry
        .heading
        .as_deref()
        .map(|h| format!(" ({h})"))
        .unwrap_or_default();
    format!(
        "{mark} {title}{creator}{year} [{kind}]{rating}{heading}  -- {loc}",
        kind = entry.kind.as_str(),
        loc = entry.location,
    )
}

fn parse_category(s: &str) -> Option<MediaCategory> {
    let normalized: String = s
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    match normalized.as_str() {
        "read" | "toread" | "reading" | "books" | "book" => Some(MediaCategory::Read),
        "watch" | "towatch" | "watching" | "movies" => Some(MediaCategory::Watch),
        "listen" | "tolisten" | "listening" | "music" => Some(MediaCategory::Listen),
        "play" | "toplay" | "playing" | "games" => Some(MediaCategory::Play),
        "other" => Some(MediaCategory::Other),
        _ => None,
    }
}

fn heading_text(h: &Heading) -> String {
    h.content.plain_text()
}

fn parse_location(loc: &str) -> (&str, u32) {
    if let Some((file, line)) = loc.rsplit_once(':') {
        (file, line.parse().unwrap_or(0))
    } else {
        (loc, 0)
    }
}

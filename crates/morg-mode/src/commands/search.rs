use std::path::PathBuf;

use morg_parser::ast::*;
use morg_parser::tags::TagKind;

use crate::collect;
use crate::report;

pub fn run(
    paths: &[PathBuf],
    query: &str,
    tags_only: bool,
    headings_only: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);
    let query_lower = query.to_lowercase();
    let mut results: Vec<SearchResult> = Vec::new();

    for pf in &parsed {
        let mut current_heading: Option<&Heading> = None;

        for block in &pf.document.children {
            search_block(
                block,
                &pf.path,
                &query_lower,
                tags_only,
                headings_only,
                &mut current_heading,
                &mut results,
            );
        }
    }

    if json {
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let (file, lnum) = parse_location(&r.location);
                serde_json::json!({
                    "file": file,
                    "line": lnum,
                    "kind": r.match_kind,
                    "text": r.matched_text,
                    "heading": r.heading,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("No matches for \"{query}\".");
        return Ok(());
    }

    for r in &results {
        let heading_ctx = r
            .heading
            .as_deref()
            .map(|h| format!(" ({h})"))
            .unwrap_or_default();
        println!(
            "{loc}{heading_ctx}  {kind}: {text}",
            loc = r.location,
            kind = r.match_kind,
            text = r.matched_text,
        );
    }

    println!("\n{} match(es) for \"{query}\".", results.len());
    Ok(())
}

struct SearchResult {
    location: String,
    heading: Option<String>,
    match_kind: &'static str,
    matched_text: String,
}

fn search_block<'a>(
    block: &'a Block,
    file: &std::path::Path,
    query: &str,
    tags_only: bool,
    headings_only: bool,
    current_heading: &mut Option<&'a Heading>,
    results: &mut Vec<SearchResult>,
) {
    match block {
        Block::Heading(h) => {
            *current_heading = Some(h);
            let plain = h.content.plain_text();
            let loc = report::format_location(file, &h.span);

            if !tags_only && plain.to_lowercase().contains(query) {
                results.push(SearchResult {
                    location: loc.clone(),
                    heading: None,
                    match_kind: "heading",
                    matched_text: plain.clone(),
                });
            }

            if !headings_only {
                search_tags_in_content(&h.content, file, &h.span, query, *current_heading, results);
            }
        }
        Block::Paragraph(p) if !headings_only => {
            let loc = report::format_location(file, &p.span);

            if !tags_only {
                let plain = p.content.plain_text();
                if plain.to_lowercase().contains(query) {
                    results.push(SearchResult {
                        location: loc.clone(),
                        heading: current_heading.map(|h| h.content.plain_text()),
                        match_kind: "text",
                        matched_text: truncate(&plain, 80),
                    });
                }
            }

            search_tags_in_content(&p.content, file, &p.span, query, *current_heading, results);
        }
        Block::BlockTag(tag) if !headings_only => {
            let tag_name = tag_name_str(&tag.kind);
            if tag_name.to_lowercase().contains(query) {
                results.push(SearchResult {
                    location: report::format_location(file, &tag.span),
                    heading: current_heading.map(|h| h.content.plain_text()),
                    match_kind: "tag",
                    matched_text: format!("#{tag_name}"),
                });
            }
        }
        Block::List(list) if !headings_only => {
            for item in &list.items {
                let plain = item.content.plain_text();
                if !tags_only && plain.to_lowercase().contains(query) {
                    results.push(SearchResult {
                        location: report::format_location(file, &item.span),
                        heading: current_heading.map(|h| h.content.plain_text()),
                        match_kind: "list-item",
                        matched_text: truncate(&plain, 80),
                    });
                }
                search_tags_in_content(
                    &item.content,
                    file,
                    &item.span,
                    query,
                    *current_heading,
                    results,
                );
                for child in &item.children {
                    search_block(
                        child,
                        file,
                        query,
                        tags_only,
                        headings_only,
                        current_heading,
                        results,
                    );
                }
            }
        }
        Block::Callout(c) => {
            for child in &c.content {
                search_block(
                    child,
                    file,
                    query,
                    tags_only,
                    headings_only,
                    current_heading,
                    results,
                );
            }
        }
        _ => {}
    }
}

fn search_tags_in_content(
    content: &InlineContent,
    file: &std::path::Path,
    span: &morg_parser::span::Span,
    query: &str,
    current_heading: Option<&Heading>,
    results: &mut Vec<SearchResult>,
) {
    for tag in content.tags() {
        let name = tag_name_str(&tag.kind);
        if name.to_lowercase().contains(query) {
            results.push(SearchResult {
                location: report::format_location(file, span),
                heading: current_heading.map(|h| h.content.plain_text()),
                match_kind: "tag",
                matched_text: format!("#{name}"),
            });
        }
    }
}

fn tag_name_str(kind: &TagKind) -> String {
    match kind {
        TagKind::Todo { .. } => "todo".to_string(),
        TagKind::Done { .. } => "done".to_string(),
        TagKind::Deadline { .. } => "deadline".to_string(),
        TagKind::Scheduled { .. } => "scheduled".to_string(),
        TagKind::Date { .. } => "date".to_string(),
        TagKind::Event { .. } => "event".to_string(),
        TagKind::ClockIn { .. } => "clock-in".to_string(),
        TagKind::ClockOut { .. } => "clock-out".to_string(),
        TagKind::Clock(_) => "clock".to_string(),
        TagKind::Tangle => "tangle".to_string(),
        TagKind::Priority { level } => format!("priority {level}"),
        TagKind::Effort { minutes } => format!("effort {minutes}m"),
        TagKind::Closed { .. } => "closed".to_string(),
        TagKind::Archive => "archive".to_string(),
        TagKind::Progress => "progress".to_string(),
        TagKind::Media { kind, title, .. } => match title {
            Some(t) => format!("media {kind} {t}"),
            None => format!("media {kind}"),
        },
        TagKind::CustomState { name, .. } => name.to_lowercase(),
        TagKind::Unknown { name, .. } => name.clone(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn parse_location(loc: &str) -> (&str, u32) {
    if let Some((file, line)) = loc.rsplit_once(':') {
        (file, line.parse().unwrap_or(0))
    } else {
        (loc, 0)
    }
}

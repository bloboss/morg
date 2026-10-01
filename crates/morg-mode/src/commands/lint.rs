use std::path::PathBuf;

use morg_parser::ast::*;
use morg_parser::tags::TagKind;

use crate::collect;
use crate::report;

pub fn run(paths: &[PathBuf], json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);
    let mut warnings: Vec<LintWarning> = Vec::new();

    for pf in &parsed {
        // Parser errors are already reported; collect lint warnings
        lint_document(&pf.document, &pf.path, &mut warnings);
    }

    if json {
        let items: Vec<serde_json::Value> = warnings
            .iter()
            .map(|w| {
                let (file, lnum) = parse_location(&w.location);
                serde_json::json!({
                    "file": file,
                    "line": lnum,
                    "severity": w.severity,
                    "message": w.message,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    if warnings.is_empty() {
        println!("No issues found.");
        return Ok(());
    }

    for w in &warnings {
        println!(
            "{loc}  [{severity}] {msg}",
            loc = w.location,
            severity = w.severity,
            msg = w.message
        );
    }

    let error_count = warnings.iter().filter(|w| w.severity == "error").count();
    let warn_count = warnings.iter().filter(|w| w.severity == "warn").count();
    println!("\n{error_count} error(s), {warn_count} warning(s).");

    if error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

struct LintWarning {
    location: String,
    severity: &'static str,
    message: String,
}

fn lint_document(doc: &Document, file: &std::path::Path, warnings: &mut Vec<LintWarning>) {
    let mut prev_heading_level: Option<u8> = None;

    for block in &doc.children {
        lint_block(block, file, &mut prev_heading_level, warnings);
    }
}

fn lint_block(
    block: &Block,
    file: &std::path::Path,
    prev_heading_level: &mut Option<u8>,
    warnings: &mut Vec<LintWarning>,
) {
    match block {
        Block::Heading(h) => {
            // Check for heading level jumps (e.g. # → ### skipping ##)
            if let Some(prev) = *prev_heading_level
                && h.level > prev + 1
            {
                warnings.push(LintWarning {
                    location: report::format_location(file, &h.span),
                    severity: "warn",
                    message: format!(
                        "heading level jumps from {} to {} (skipped level {})",
                        prev,
                        h.level,
                        prev + 1
                    ),
                });
            }
            *prev_heading_level = Some(h.level);

            // Check for empty headings
            if h.content.plain_text().is_empty() {
                warnings.push(LintWarning {
                    location: report::format_location(file, &h.span),
                    severity: "warn",
                    message: "empty heading".to_string(),
                });
            }

            // Check property drawer for duplicate keys
            if let Some(ref props) = h.properties {
                let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
                for key in props.entries.keys() {
                    if !seen.insert(key.as_str()) {
                        warnings.push(LintWarning {
                            location: report::format_location(file, &props.span),
                            severity: "warn",
                            message: format!("duplicate property key: {key}"),
                        });
                    }
                }
            }

            // Lint inline content
            lint_inline_content(&h.content, file, &h.span, warnings);
        }
        Block::Paragraph(p) => {
            lint_inline_content(&p.content, file, &p.span, warnings);
        }
        Block::BlockTag(tag) => {
            lint_tag(tag, file, warnings);
        }
        Block::CodeBlock(cb) => {
            // Check for tangle without file attribute
            let has_tangle = cb.tags.iter().any(|t| matches!(t.kind, TagKind::Tangle));
            if has_tangle && !cb.attributes.contains_key("file") {
                warnings.push(LintWarning {
                    location: report::format_location(file, &cb.span),
                    severity: "error",
                    message: "#tangle tag without file= attribute".to_string(),
                });
            }

            // Check for code block without language
            if cb.lang.is_none() && has_tangle {
                warnings.push(LintWarning {
                    location: report::format_location(file, &cb.span),
                    severity: "warn",
                    message: "tangled code block without language specifier".to_string(),
                });
            }
        }
        Block::Callout(c) => {
            let has_tangle = c.tags.iter().any(|t| matches!(t.kind, TagKind::Tangle));
            if has_tangle && !c.attributes.contains_key("file") {
                warnings.push(LintWarning {
                    location: report::format_location(file, &c.span),
                    severity: "error",
                    message: "#tangle tag on callout without file= attribute".to_string(),
                });
            }
            for child in &c.content {
                lint_block(child, file, prev_heading_level, warnings);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                lint_inline_content(&item.content, file, &item.span, warnings);
                for child in &item.children {
                    lint_block(child, file, prev_heading_level, warnings);
                }
            }
        }
        _ => {}
    }
}

fn lint_inline_content(
    content: &InlineContent,
    file: &std::path::Path,
    span: &morg_parser::span::Span,
    warnings: &mut Vec<LintWarning>,
) {
    for tag in content.tags() {
        lint_tag(tag, file, warnings);
    }

    // Check for broken links (empty URL)
    for seg in &content.segments {
        if let InlineKind::Link(link) = &seg.kind
            && link.url.is_empty()
            && link.text.is_empty()
        {
            warnings.push(LintWarning {
                location: report::format_location(file, span),
                severity: "warn",
                message: "empty link".to_string(),
            });
        }
    }
}

fn lint_tag(tag: &morg_parser::tags::Tag, file: &std::path::Path, warnings: &mut Vec<LintWarning>) {
    match &tag.kind {
        TagKind::Deadline { date, .. } | TagKind::Scheduled { date, .. } => {
            // Warn about past deadlines/scheduled dates
            let today = chrono::Local::now().date_naive();
            if date.date() < today {
                warnings.push(LintWarning {
                    location: report::format_location(file, &tag.span),
                    severity: "warn",
                    message: format!("past date: {date}"),
                });
            }
        }
        _ => {}
    }
}

fn parse_location(loc: &str) -> (&str, u32) {
    if let Some((file, line)) = loc.rsplit_once(':') {
        (file, line.parse().unwrap_or(0))
    } else {
        (loc, 0)
    }
}

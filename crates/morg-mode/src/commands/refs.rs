use std::collections::HashMap;
use std::path::PathBuf;

use morg_parser::ast::{Block, InlineKind, InlineSegment};

use crate::collect;

pub fn run(paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);

    // Build ID index: id -> (file, heading_text)
    let mut id_index: HashMap<String, (PathBuf, String)> = HashMap::new();
    // Collect all id: links
    let mut id_refs: Vec<(String, PathBuf, u32)> = Vec::new(); // (id, file, line)

    for pf in &parsed {
        collect_ids_and_refs(&pf.document.children, &pf.path, &mut id_index, &mut id_refs);
    }

    if id_index.is_empty() && id_refs.is_empty() {
        println!("No IDs or cross-references found.");
        return Ok(());
    }

    // Report IDs
    println!("IDs ({}):", id_index.len());
    let mut ids: Vec<_> = id_index.iter().collect();
    ids.sort_by_key(|(_, (path, _))| path.clone());
    for (id, (path, heading)) in &ids {
        println!("  {id}  {heading}  -- {}", path.display());
    }

    // Validate references
    if !id_refs.is_empty() {
        println!("\nReferences ({}):", id_refs.len());
        let mut broken = 0;
        for (id, file, line) in &id_refs {
            let status = if id_index.contains_key(id) {
                "OK"
            } else {
                broken += 1;
                "BROKEN"
            };
            println!("  [{status}] id:{id}  -- {}:{line}", file.display());
        }
        if broken > 0 {
            eprintln!("\n{broken} broken reference(s) found.");
        }
    }

    Ok(())
}

fn collect_ids_and_refs(
    blocks: &[Block],
    file: &std::path::Path,
    id_index: &mut HashMap<String, (PathBuf, String)>,
    id_refs: &mut Vec<(String, PathBuf, u32)>,
) {
    for block in blocks {
        match block {
            Block::Heading(h) => {
                // Check for id in property drawer
                if let Some(ref props) = h.properties
                    && let Some(id) = props.entries.get("id")
                {
                    id_index.insert(id.clone(), (file.to_path_buf(), h.content.plain_text()));
                }
                // Check for id: links in heading content
                collect_link_refs(&h.content.segments, file, h.span.line, id_refs);
            }
            Block::Paragraph(p) => {
                collect_link_refs(&p.content.segments, file, p.span.line, id_refs);
            }
            Block::Callout(c) => {
                collect_ids_and_refs(&c.content, file, id_index, id_refs);
            }
            Block::List(l) => {
                for item in &l.items {
                    collect_link_refs(&item.content.segments, file, item.span.line, id_refs);
                }
            }
            _ => {}
        }
    }
}

fn collect_link_refs(
    segments: &[InlineSegment],
    file: &std::path::Path,
    line: u32,
    id_refs: &mut Vec<(String, PathBuf, u32)>,
) {
    for seg in segments {
        match &seg.kind {
            InlineKind::Link(link) => {
                if let Some(id) = link.url.strip_prefix("id:") {
                    id_refs.push((id.to_string(), file.to_path_buf(), line));
                }
            }
            InlineKind::Bold(inner)
            | InlineKind::Italic(inner)
            | InlineKind::Strikethrough(inner) => {
                collect_link_refs(&inner.segments, file, line, id_refs);
            }
            _ => {}
        }
    }
}

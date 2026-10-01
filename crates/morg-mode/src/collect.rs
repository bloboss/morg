use std::path::{Path, PathBuf};

use morg_parser::ast::*;
use morg_parser::error::ParseError;
use morg_parser::parser::{ParseResult, parse_document};
use morg_parser::tags::{Tag, TagKind};

pub struct ParsedFile {
    pub path: PathBuf,
    pub document: Document,
    #[allow(dead_code)]
    pub errors: Vec<ParseError>,
}

pub fn resolve_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            collect_markdown_files(path, &mut files);
        } else {
            eprintln!(
                "warning: skipping {}: not a file or directory",
                path.display()
            );
        }
    }
    files.sort();
    files
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("warning: cannot read directory {}", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, out);
        } else if is_markdown(&path) {
            out.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "morg" | "markdown")
    )
}

pub fn parse_files(paths: &[PathBuf]) -> Vec<ParsedFile> {
    let files = resolve_paths(paths);
    files
        .into_iter()
        .filter_map(|path| {
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("warning: cannot read {}: {e}", path.display());
                    return None;
                }
            };
            let ParseResult { document, errors } = parse_document(&source);
            for err in &errors {
                eprintln!("{}:{}", path.display(), err);
            }
            Some(ParsedFile {
                path,
                document,
                errors,
            })
        })
        .collect()
}

/// Context for a tag found during walking
pub struct TagContext<'a> {
    pub file: &'a Path,
    pub tag: &'a Tag,
    pub parent_heading: Option<&'a Heading>,
    pub inherited_tags: Vec<String>,
    pub is_archived: bool,
}

/// Extract file-level tags from frontmatter `tags:` field.
pub fn file_tags(document: &Document) -> Vec<String> {
    let Some(ref fm) = document.frontmatter else {
        return Vec::new();
    };
    let Some(tags) = fm.data.as_mapping_get("tags") else {
        return Vec::new();
    };
    if let Some(seq) = tags.as_sequence() {
        seq.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    } else if let Some(s) = tags.as_str() {
        s.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

/// A custom TODO state: name → is_done
#[derive(Debug, Clone)]
pub struct TodoSequence {
    pub states: Vec<(String, bool)>, // (name, is_done)
}

/// Extract todo_sequences from frontmatter.
/// Format: `todo_sequences: [["TODO", "NEXT", "WAIT", "|", "DONE", "CANCELLED"]]`
pub fn todo_sequences(document: &Document) -> Vec<TodoSequence> {
    let Some(ref fm) = document.frontmatter else {
        return Vec::new();
    };
    let Some(seqs) = fm
        .data
        .as_mapping_get("todo_sequences")
        .and_then(|v| v.as_sequence())
    else {
        return Vec::new();
    };

    seqs.iter()
        .filter_map(|seq| {
            let items = seq.as_sequence()?;
            let mut states = Vec::new();
            let mut past_separator = false;
            for item in items {
                let s = item.as_str()?;
                if s == "|" {
                    past_separator = true;
                    continue;
                }
                states.push((s.to_uppercase(), past_separator));
            }
            Some(TodoSequence { states })
        })
        .collect()
}

/// Check if a tag name matches a custom todo sequence state.
/// Returns `Some((name, is_done))` if it matches.
pub fn match_custom_state(name: &str, sequences: &[TodoSequence]) -> Option<(String, bool)> {
    let upper = name.to_uppercase();
    for seq in sequences {
        for (state_name, is_done) in &seq.states {
            if *state_name == upper {
                return Some((state_name.clone(), *is_done));
            }
        }
    }
    None
}

/// Walk all blocks in a document, yielding tags with their context.
/// Supports tag inheritance (parent heading tags propagate to children)
/// and file-level tags from frontmatter.
pub fn walk_tags<'a>(
    file: &'a Path,
    document: &'a Document,
    mut visitor: impl FnMut(TagContext<'a>),
) {
    let ftags = file_tags(document);
    let mut heading_stack: Vec<(u8, Vec<String>, bool)> = Vec::new(); // (level, tags, archived)
    let mut current_heading: Option<&'a Heading> = None;

    for block in &document.children {
        walk_block_with_inheritance(
            file,
            block,
            &mut current_heading,
            &mut heading_stack,
            &ftags,
            &mut visitor,
        );
    }
}

fn walk_block_with_inheritance<'a>(
    file: &'a Path,
    block: &'a Block,
    current_heading: &mut Option<&'a Heading>,
    heading_stack: &mut Vec<(u8, Vec<String>, bool)>,
    file_tags: &[String],
    visitor: &mut impl FnMut(TagContext<'a>),
) {
    match block {
        Block::Heading(h) => {
            // Pop headings from stack that are same level or deeper
            while heading_stack
                .last()
                .is_some_and(|(lvl, _, _)| *lvl >= h.level)
            {
                heading_stack.pop();
            }

            // Collect this heading's own tags
            let own_tags: Vec<String> = h
                .content
                .tags()
                .iter()
                .filter_map(|t| match &t.kind {
                    TagKind::Unknown { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();

            let is_archived = h
                .content
                .tags()
                .iter()
                .any(|t| matches!(t.kind, TagKind::Archive));

            heading_stack.push((h.level, own_tags, is_archived));
            *current_heading = Some(h);

            let (inherited, archived) = compute_inherited(heading_stack, file_tags);

            for tag in h.content.tags() {
                visitor(TagContext {
                    file,
                    tag,
                    parent_heading: *current_heading,
                    inherited_tags: inherited.clone(),
                    is_archived: archived,
                });
            }
        }
        Block::Paragraph(p) => {
            let (inherited, archived) = compute_inherited(heading_stack, file_tags);
            for tag in p.content.tags() {
                visitor(TagContext {
                    file,
                    tag,
                    parent_heading: *current_heading,
                    inherited_tags: inherited.clone(),
                    is_archived: archived,
                });
            }
        }
        Block::BlockTag(tag) => {
            let (inherited, archived) = compute_inherited(heading_stack, file_tags);
            visitor(TagContext {
                file,
                tag,
                parent_heading: *current_heading,
                inherited_tags: inherited,
                is_archived: archived,
            });
        }
        Block::CodeBlock(cb) => {
            let (inherited, archived) = compute_inherited(heading_stack, file_tags);
            for tag in &cb.tags {
                visitor(TagContext {
                    file,
                    tag,
                    parent_heading: *current_heading,
                    inherited_tags: inherited.clone(),
                    is_archived: archived,
                });
            }
        }
        Block::Callout(c) => {
            for child in &c.content {
                walk_block_with_inheritance(
                    file,
                    child,
                    current_heading,
                    heading_stack,
                    file_tags,
                    visitor,
                );
            }
        }
        Block::Table(t) => {
            let (inherited, archived) = compute_inherited(heading_stack, file_tags);
            for cell in &t.headers {
                for tag in cell.tags() {
                    visitor(TagContext {
                        file,
                        tag,
                        parent_heading: *current_heading,
                        inherited_tags: inherited.clone(),
                        is_archived: archived,
                    });
                }
            }
            for row in &t.rows {
                for cell in row {
                    for tag in cell.tags() {
                        visitor(TagContext {
                            file,
                            tag,
                            parent_heading: *current_heading,
                            inherited_tags: inherited.clone(),
                            is_archived: archived,
                        });
                    }
                }
            }
        }
        Block::List(list) => {
            let (inherited, archived) = compute_inherited(heading_stack, file_tags);
            for item in &list.items {
                for tag in item.content.tags() {
                    visitor(TagContext {
                        file,
                        tag,
                        parent_heading: *current_heading,
                        inherited_tags: inherited.clone(),
                        is_archived: archived,
                    });
                }
                for child in &item.children {
                    walk_block_with_inheritance(
                        file,
                        child,
                        current_heading,
                        heading_stack,
                        file_tags,
                        visitor,
                    );
                }
            }
        }
        Block::BlankLine(_)
        | Block::HtmlBlock(_)
        | Block::HorizontalRule(_)
        | Block::Comment(_)
        | Block::FootnoteDefinition(_) => {}
    }
}

/// Compute the full set of inherited tags and archive status from the heading stack.
fn compute_inherited(
    heading_stack: &[(u8, Vec<String>, bool)],
    file_tags: &[String],
) -> (Vec<String>, bool) {
    let mut tags: Vec<String> = file_tags.to_vec();
    let mut archived = false;

    for (_, heading_tags, is_archived) in heading_stack {
        tags.extend(heading_tags.iter().cloned());
        if *is_archived {
            archived = true;
        }
    }

    tags.sort();
    tags.dedup();
    (tags, archived)
}

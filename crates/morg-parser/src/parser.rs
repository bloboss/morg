//! Token-consuming parser for morg-mode.
//!
//! Consumes the `Lexer` token stream to produce the same AST as `parser.rs`.
//! Block-level tokens drive the structure; `lexer::tokenize_inline` is called
//! on demand for text content.

use std::collections::HashMap;

use crate::ast::*;
use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::{self, Lexer};
use crate::span::Span;
use crate::tags::{self, Tag};
use crate::tokens::Token;

pub struct ParseResult {
    pub document: Document,
    pub errors: Vec<ParseError>,
}

pub fn parse_document(source: &str) -> ParseResult {
    let mut lex = Lexer::new(source);
    let mut errors = Vec::new();

    let frontmatter = parse_frontmatter(&mut lex, &mut errors);
    let mut children = Vec::new();

    while !lex.is_eof() {
        match parse_block(&mut lex, &mut errors) {
            Some(block) => children.push(block),
            None => {
                lex.skip_to_next_line();
            }
        }
    }

    ParseResult {
        document: Document {
            frontmatter,
            children,
        },
        errors,
    }
}

// ---------------------------------------------------------------------------
// Frontmatter
// ---------------------------------------------------------------------------

fn parse_frontmatter(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> Option<Frontmatter> {
    let first = lex.peek();
    if first.span.line != 1 || !matches!(first.kind, Token::FrontmatterDelim) {
        return None;
    }

    let source = lex.source();
    let open_span = lex.advance().span;
    skip_newline(lex);

    loop {
        if lex.is_eof() {
            errors.push(ParseError {
                kind: ParseErrorKind::UnclosedFrontmatter,
                span: open_span,
                message: "frontmatter opened but never closed with ---".to_string(),
            });
            return None;
        }

        let tok = lex.peek();
        if matches!(tok.kind, Token::FrontmatterDelim) {
            let close_span = lex.advance().span;
            skip_newline(lex);

            // Slice the YAML text straight from the source (between the
            // newline after the opening `---` and the newline before the
            // closing one) so parsed markers line up with file offsets.
            let content_start = (open_span.end + 1).min(close_span.start);
            let content_end = close_span.start.saturating_sub(1).max(content_start);
            let raw = source[content_start..content_end].to_string();
            let span = open_span.merge(close_span);

            match load_frontmatter_yaml(&raw, content_start, open_span.line + 1) {
                Ok((data, entries)) => {
                    return Some(Frontmatter {
                        raw,
                        data,
                        entries,
                        span,
                    });
                }
                Err(e) => {
                    errors.push(ParseError {
                        kind: ParseErrorKind::InvalidYaml,
                        span,
                        message: format!("invalid YAML in frontmatter: {e}"),
                    });
                    return None;
                }
            }
        }

        lex.skip_to_next_line();
    }
}

/// Parse frontmatter YAML with `saphyr`, returning the plain value tree plus
/// per-entry spans for the top-level mapping.
///
/// `base_offset` is the absolute byte offset of `raw`'s first byte in the
/// source; `base_line` its 1-based line. Saphyr markers are char-indexed
/// into `raw`, so they are mapped back to byte offsets before being
/// absolutized.
fn load_frontmatter_yaml(
    raw: &str,
    base_offset: usize,
    base_line: u32,
) -> Result<(saphyr::YamlOwned, Vec<FrontmatterEntry>), saphyr::ScanError> {
    use saphyr::{LoadableYamlNode, MarkedYamlOwned, ScalarOwned, YamlDataOwned, YamlOwned};

    let mut docs = MarkedYamlOwned::load_from_str(raw)?;
    if docs.is_empty() {
        // Empty or comment-only frontmatter — same as YAML `null`.
        return Ok((YamlOwned::Value(ScalarOwned::Null), Vec::new()));
    }
    let marked = docs.remove(0);

    let cx = YamlSpanCx {
        raw,
        // Char index (saphyr markers) → byte offset in `raw`.
        char_to_byte: raw
            .char_indices()
            .map(|(b, _)| b)
            .chain(std::iter::once(raw.len()))
            .collect(),
        base_offset,
        base_line,
    };

    let mut entries = Vec::new();
    if let YamlDataOwned::Mapping(map) = &marked.data {
        for (k, v) in map {
            let Some((key, key_span)) = cx.key(k) else {
                continue; // non-string key — no typed entry for it
            };
            let value = cx.node(v);

            entries.push(FrontmatterEntry {
                key,
                key_span,
                value_span: value.span,
                value,
            });
        }
    }

    Ok((marked_to_plain(marked), entries))
}

/// Maps saphyr's char-indexed markers on `raw` back to [`Span`]s with
/// absolute byte offsets into the document source.
struct YamlSpanCx<'a> {
    raw: &'a str,
    char_to_byte: Vec<usize>,
    /// Absolute byte offset of `raw`'s first byte in the source.
    base_offset: usize,
    /// 1-based line of `raw`'s first line in the source.
    base_line: u32,
}

impl YamlSpanCx<'_> {
    fn to_byte(&self, char_idx: usize) -> usize {
        self.char_to_byte
            .get(char_idx)
            .copied()
            .unwrap_or(self.raw.len())
    }

    fn abs_span(&self, start_b: usize, end_b: usize) -> Span {
        let prefix = &self.raw[..start_b];
        let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
        Span::new(
            self.base_offset + start_b,
            self.base_offset + end_b,
            self.base_line + prefix.matches('\n').count() as u32,
            (start_b - line_start) as u32 + 1,
        )
    }

    /// The string key and its span, or `None` for a non-string key.
    fn key(&self, k: &saphyr::MarkedYamlOwned) -> Option<(String, Span)> {
        let key = k.data.as_str()?;
        let span = self.abs_span(
            self.to_byte(k.span.start.index()),
            self.to_byte(k.span.end.index()),
        );
        Some((key.to_string(), span))
    }

    /// Build the span-annotated node tree for a marked YAML value.
    fn node(&self, v: &saphyr::MarkedYamlOwned) -> FrontmatterNode {
        use saphyr::YamlDataOwned;

        // The value's end marker may extend past the value text (e.g.
        // over the newline after a block sequence); trim it back.
        let start = self.to_byte(v.span.start.index());
        let mut end = self.to_byte(v.span.end.index()).max(start);
        // For flow collections the end marker instead points AT the closing
        // delimiter, leaving it outside the exclusive range; take it back in
        // so the span covers the full `[...]`/`{...}` markup.
        for (open, close, is_kind) in [
            ('[', ']', matches!(v.data, YamlDataOwned::Sequence(_))),
            ('{', '}', matches!(v.data, YamlDataOwned::Mapping(_))),
        ] {
            if is_kind && self.raw[start..].starts_with(open) && self.raw[end..].starts_with(close)
            {
                end += close.len_utf8();
            }
        }
        let end = start + self.raw[start..end].trim_end().len();
        let span = self.abs_span(start, end);

        let kind = match &v.data {
            YamlDataOwned::Sequence(items) => {
                FrontmatterNodeKind::Seq(items.iter().map(|item| self.node(item)).collect())
            }
            YamlDataOwned::Mapping(map) => FrontmatterNodeKind::Map(
                map.iter()
                    .filter_map(|(k, inner)| {
                        let (key, key_span) = self.key(k)?;
                        Some(FrontmatterMapEntry {
                            key,
                            key_span,
                            value: self.node(inner),
                        })
                    })
                    .collect(),
            ),
            // A tagged node keeps the outer span (tag markup included) but
            // takes its shape from the inner value.
            YamlDataOwned::Tagged(_, inner) => self.node(inner).kind,
            _ => FrontmatterNodeKind::Scalar,
        };

        FrontmatterNode { span, kind }
    }
}

/// Strip span annotations from a `MarkedYamlOwned` tree, yielding the plain
/// `YamlOwned` stored on the AST.
fn marked_to_plain(node: saphyr::MarkedYamlOwned) -> saphyr::YamlOwned {
    use saphyr::{YamlDataOwned, YamlOwned};
    match node.data {
        YamlDataOwned::Representation(s, style, tag) => YamlOwned::Representation(s, style, tag),
        YamlDataOwned::Value(scalar) => YamlOwned::Value(scalar),
        YamlDataOwned::Sequence(seq) => {
            YamlOwned::Sequence(seq.into_iter().map(marked_to_plain).collect())
        }
        YamlDataOwned::Mapping(map) => YamlOwned::Mapping(
            map.into_iter()
                .map(|(k, v)| (marked_to_plain(k), marked_to_plain(v)))
                .collect(),
        ),
        YamlDataOwned::Tagged(tag, inner) => {
            YamlOwned::Tagged(tag, Box::new(marked_to_plain(*inner)))
        }
        YamlDataOwned::Alias(id) => YamlOwned::Alias(id),
        YamlDataOwned::BadValue => YamlOwned::BadValue,
    }
}

// ---------------------------------------------------------------------------
// Block dispatcher
// ---------------------------------------------------------------------------

fn parse_block(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> Option<Block> {
    let tok = lex.peek();

    match &tok.kind {
        Token::BlankLine => {
            let span = lex.advance().span;
            skip_newline(lex);
            Some(Block::BlankLine(span))
        }
        Token::Heading { level } => {
            let level = *level;
            parse_heading(lex, level, errors)
        }
        Token::FencedCodeOpen { .. } | Token::FencedCodeClose { .. } => {
            Some(parse_code_block(lex, errors))
        }
        Token::CalloutStart { .. } => Some(parse_callout(lex, errors)),
        Token::ListMarker { .. } => Some(parse_list(lex)),
        Token::TableRow => Some(parse_table(lex)),
        Token::HtmlOpen { .. } => Some(parse_html_block(lex, errors)),
        Token::HorizontalRule => {
            let span = lex.advance().span;
            skip_newline(lex);
            Some(Block::HorizontalRule(span))
        }
        Token::LineComment => Some(parse_line_comment(lex)),
        Token::BlockCommentOpen => Some(parse_block_comment(lex)),
        Token::FootnoteDefStart { .. } => Some(parse_footnote_def(lex)),
        Token::FrontmatterDelim => {
            // --- not at line 1 — treat as paragraph text
            let span = lex.advance().span;
            skip_newline(lex);
            Some(Block::Paragraph(Paragraph {
                content: InlineContent::plain("---", span),
                span,
            }))
        }
        Token::Tag(_) | Token::UnknownTag { .. } => Some(parse_block_tag(lex)),
        Token::Text(_) | Token::RawLine(_) => parse_paragraph(lex),
        Token::PropertiesOpen
        | Token::PropertiesClose
        | Token::BlockCommentClose
        | Token::HtmlClose { .. }
        | Token::BlockquoteContinuation => {
            // Stray structural tokens — treat as text paragraph
            parse_paragraph(lex)
        }
        _ => {
            // Skip unknown tokens
            lex.skip_to_next_line();
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Heading + property drawer
// ---------------------------------------------------------------------------

fn parse_heading(lex: &mut Lexer<'_>, level: u8, errors: &mut Vec<ParseError>) -> Option<Block> {
    let head_span = lex.advance().span; // consume Heading token
    let raw = consume_raw_line(lex);
    skip_newline(lex);

    let text = raw.trim_start();
    let content_start = text.find(' ').map(|i| i + 1).unwrap_or(text.len());
    let content_text = &text[content_start..];
    let content = build_inline_content(content_text, sub_span(head_span, &raw, content_text));

    // Look ahead for property drawer
    let saved = lex.position();
    skip_blank_lines(lex);

    let properties = if matches!(lex.peek().kind, Token::PropertiesOpen) {
        Some(parse_property_drawer(lex, errors))
    } else {
        lex.set_position(saved);
        None
    };

    Some(Block::Heading(Heading {
        level,
        content,
        properties,
        span: head_span,
    }))
}

fn parse_property_drawer(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> PropertyDrawer {
    let open_span = lex.advance().span; // consume PropertiesOpen
    skip_newline(lex);

    let mut entries = HashMap::new();
    let mut last_span = open_span;

    loop {
        if lex.is_eof() {
            errors.push(ParseError {
                kind: ParseErrorKind::UnexpectedToken,
                span: open_span,
                message: "#properties block opened but never closed with #end".to_string(),
            });
            break;
        }

        match &lex.peek().kind {
            Token::PropertiesClose => {
                last_span = lex.advance().span;
                skip_newline(lex);
                break;
            }
            Token::BlankLine => {
                lex.advance();
                skip_newline(lex);
            }
            _ => {
                let line_span = lex.peek().span;
                let raw = extract_raw_line(lex);
                last_span = line_span;
                let trimmed = raw.trim();
                if let Some((key, value)) = trimmed.split_once('=') {
                    entries.insert(key.trim().to_string(), value.trim().to_string());
                } else {
                    errors.push(ParseError {
                        kind: ParseErrorKind::UnexpectedToken,
                        span: line_span,
                        message: format!("invalid property line: {trimmed}"),
                    });
                }
            }
        }
    }

    PropertyDrawer {
        entries,
        span: open_span.merge(last_span),
    }
}

// ---------------------------------------------------------------------------
// Code block
// ---------------------------------------------------------------------------

fn parse_code_block(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> Block {
    let tok = lex.advance();
    let open_span = tok.span;

    let (fence_char, fence_len, info_string) = match &tok.kind {
        Token::FencedCodeOpen {
            info,
            fence_char,
            fence_len,
        } => (*fence_char, *fence_len, info.clone()),
        Token::FencedCodeClose {
            fence_char,
            fence_len,
        } => (*fence_char, *fence_len, String::new()),
        _ => unreachable!(),
    };
    skip_newline(lex);
    let info_str = &info_string;

    let (lang, code_tags, attributes) = parse_code_info(info_str, open_span);
    let mut body_lines: Vec<String> = Vec::new();

    loop {
        if lex.is_eof() {
            errors.push(ParseError {
                kind: ParseErrorKind::UnclosedCodeFence,
                span: open_span,
                message: "code fence opened but never closed".to_string(),
            });
            break;
        }

        let is_close = matches!(
            &lex.peek().kind,
            Token::FencedCodeClose { fence_char: fc, fence_len: fl }
                if *fc == fence_char && *fl >= fence_len
        );

        if is_close {
            let close_span = lex.advance().span;
            skip_newline(lex);
            return Block::CodeBlock(CodeBlock {
                lang,
                tags: code_tags,
                attributes,
                body: body_lines.join("\n"),
                span: open_span.merge(close_span),
            });
        }

        let raw = extract_raw_line(lex);
        body_lines.push(raw);
    }

    Block::CodeBlock(CodeBlock {
        lang,
        tags: code_tags,
        attributes,
        body: body_lines.join("\n"),
        span: open_span,
    })
}

fn parse_code_info(info: &str, span: Span) -> (Option<String>, Vec<Tag>, HashMap<String, String>) {
    let parts: Vec<&str> = info.split_whitespace().collect();
    if parts.is_empty() {
        return (None, Vec::new(), HashMap::new());
    }
    let lang = parts
        .first()
        .filter(|p| !p.starts_with('#') && !p.contains('='))
        .map(|p| p.to_string());
    let meta_start = if lang.is_some() { 1 } else { 0 };
    let meta_str = parts[meta_start..].join(" ");
    let (tag_list, attrs) = parse_metadata(&meta_str, span);
    (lang, tag_list, attrs)
}

fn parse_metadata(info: &str, span: Span) -> (Vec<Tag>, HashMap<String, String>) {
    let mut tag_list = Vec::new();
    let mut attrs = HashMap::new();
    for part in info.split_whitespace() {
        if part.starts_with('#') && part.len() > 1 {
            tag_list.push(tags::parse_tag(&part[1..], None, span));
        } else if let Some((key, value)) = part.split_once('=') {
            attrs.insert(key.to_string(), value.to_string());
        }
    }
    (tag_list, attrs)
}

// ---------------------------------------------------------------------------
// Callout
// ---------------------------------------------------------------------------

fn parse_callout(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> Block {
    let tok = lex.advance();
    let open_span = tok.span;

    let (kind, metadata) = match &tok.kind {
        Token::CalloutStart { kind, metadata } => (kind.clone(), metadata.clone()),
        _ => unreachable!(),
    };

    let (callout_tags, attributes) = match metadata.as_deref() {
        Some(meta) => parse_metadata(meta, open_span),
        None => (Vec::new(), HashMap::new()),
    };

    // Get raw line text to extract content after [!type][metadata]
    let mut content_lines: Vec<String> = Vec::new();
    let raw = consume_raw_line(lex);
    skip_newline(lex);

    // Extract content after the [!type] (and optional [metadata]) on the first line
    let first_text = raw.trim_start();
    if let Some(rest) = first_text.strip_prefix('>') {
        let rest = rest.trim_start();
        if let Some(after_type) = rest.find(']') {
            let mut after = &rest[after_type + 1..];
            let trimmed_after = after.trim_start();
            if trimmed_after.starts_with('[')
                && let Some(meta_end) = trimmed_after.find(']')
            {
                after = &trimmed_after[meta_end + 1..];
            }
            let after = after.trim();
            if !after.is_empty() {
                content_lines.push(after.to_string());
            }
        }
    }

    let mut last_span = open_span;

    // Collect continuation lines
    loop {
        if matches!(lex.peek().kind, Token::BlockquoteContinuation) {
            last_span = lex.advance().span;
            let raw = consume_raw_line(lex);
            skip_newline(lex);
            let text = raw.trim_start();
            let stripped = text.strip_prefix('>').unwrap_or(text);
            content_lines.push(stripped.trim_start().to_string());
        } else {
            break;
        }
    }

    let inner_source = content_lines.join("\n");
    let inner_result = parse_document(&inner_source);
    errors.extend(inner_result.errors);

    Block::Callout(Callout {
        kind,
        tags: callout_tags,
        attributes,
        content: inner_result.document.children,
        span: open_span.merge(last_span),
    })
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

fn parse_list(lex: &mut Lexer<'_>) -> Block {
    let first_span = lex.peek().span;
    let list_kind = match &lex.peek().kind {
        Token::ListMarker { ordered, .. } => {
            if *ordered {
                ListKind::Ordered
            } else {
                ListKind::Unordered
            }
        }
        _ => unreachable!(),
    };

    // Collect all list items with their indents as a flat sequence
    let mut flat_items: Vec<ListItem> = Vec::new();
    let mut last_span = first_span;

    while matches!(lex.peek().kind, Token::ListMarker { .. }) {
        let tok = lex.advance();
        let item_span = tok.span;
        let indent = match &tok.kind {
            Token::ListMarker { indent, .. } => *indent,
            _ => 0,
        };

        let raw = consume_raw_line(lex);
        skip_newline(lex);
        last_span = item_span;

        let (checkbox, content_text) = parse_list_item_content(&raw);
        let (term_text, desc) = if let Some((term, desc_text)) = content_text.split_once(" :: ") {
            (
                term,
                Some(build_inline_content(
                    desc_text,
                    sub_span(item_span, &raw, desc_text),
                )),
            )
        } else {
            (content_text, None)
        };

        let content = build_inline_content(term_text, sub_span(item_span, &raw, term_text));

        flat_items.push(ListItem {
            checkbox,
            content,
            description: desc,
            children: Vec::new(),
            indent,
            span: item_span,
        });
    }

    // Build nested structure from flat items based on indent levels
    let items = nest_list_items(flat_items);

    Block::List(List {
        kind: list_kind,
        items,
        span: first_span.merge(last_span),
    })
}

/// Convert a flat sequence of list items (with indent levels) into a nested tree.
/// Items with greater indent become children of the preceding item with lesser indent.
fn nest_list_items(flat: Vec<ListItem>) -> Vec<ListItem> {
    if flat.is_empty() {
        return flat;
    }

    let mut result: Vec<ListItem> = Vec::new();
    let mut i = 0;

    while i < flat.len() {
        let mut item = flat[i].clone();
        i += 1;

        // Collect children: subsequent items with indent > this item's indent
        let mut child_items: Vec<ListItem> = Vec::new();

        while i < flat.len() && flat[i].indent > item.indent {
            child_items.push(flat[i].clone());
            i += 1;
        }

        if !child_items.is_empty() {
            // Determine child list kind from the first child
            let child_kind = if child_items.iter().any(|c| c.indent > 0) {
                // Mixed — use unordered as default
                ListKind::Unordered
            } else {
                ListKind::Unordered
            };

            let nested_children = nest_list_items(child_items);
            let child_span = if let Some(last) = nested_children.last() {
                item.span.merge(last.span)
            } else {
                item.span
            };

            item.children.push(Block::List(List {
                kind: child_kind,
                items: nested_children,
                span: child_span,
            }));
        }

        result.push(item);
    }

    result
}

fn parse_list_item_content(text: &str) -> (Option<Checkbox>, &str) {
    let trimmed = text.trim_start();
    let after_marker =
        if trimmed.starts_with("- ") || trimmed.starts_with("+ ") || trimmed.starts_with("* ") {
            &trimmed[2..]
        } else {
            let digits_end = trimmed.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
            if trimmed[digits_end..].starts_with(". ") {
                &trimmed[digits_end + 2..]
            } else {
                trimmed
            }
        };

    if let Some(after) = after_marker.strip_prefix("[ ] ") {
        (Some(Checkbox::Unchecked), after)
    } else if after_marker.starts_with("[x] ") || after_marker.starts_with("[X] ") {
        (Some(Checkbox::Checked), &after_marker[4..])
    } else {
        (None, after_marker)
    }
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

fn parse_table(lex: &mut Lexer<'_>) -> Block {
    let first_span = lex.peek().span;
    lex.advance(); // consume TableRow
    let first_raw = consume_raw_line(lex);
    skip_newline(lex);

    let headers = parse_table_row_content(&first_raw, first_span);
    let mut alignments = Vec::new();
    let mut rows: Vec<Vec<InlineContent>> = Vec::new();
    let mut last_span = first_span;

    // Check for separator
    if matches!(lex.peek().kind, Token::TableRow) {
        let sep_span = lex.peek().span;
        let saved = lex.position();
        lex.advance();
        let sep_raw = consume_raw_line(lex);
        if let Some(aligns) = try_parse_separator(&sep_raw) {
            alignments = aligns;
            last_span = sep_span;
            skip_newline(lex);
        } else {
            lex.set_position(saved);
        }
    }

    // Data rows
    while matches!(lex.peek().kind, Token::TableRow) {
        let row_span = lex.peek().span;
        lex.advance();
        let raw = consume_raw_line(lex);
        skip_newline(lex);
        rows.push(parse_table_row_content(&raw, row_span));
        last_span = row_span;
    }

    if alignments.is_empty() && !headers.is_empty() {
        alignments = vec![Alignment::None; headers.len()];
    }

    Block::Table(Table {
        headers,
        alignments,
        rows,
        span: first_span.merge(last_span),
    })
}

fn parse_table_row_content(line: &str, span: Span) -> Vec<InlineContent> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner
        .split('|')
        .map(|cell| {
            let cell = cell.trim();
            build_inline_content(cell, sub_span(span, line, cell))
        })
        .collect()
}

fn try_parse_separator(line: &str) -> Option<Vec<Alignment>> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let cells: Vec<&str> = inner.split('|').collect();
    let mut alignments = Vec::new();
    for cell in cells {
        let cell = cell.trim();
        if cell.is_empty() {
            return None;
        }
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let middle = if left { &cell[1..] } else { cell };
        let middle = if right {
            &middle[..middle.len() - 1]
        } else {
            middle
        };
        if middle.is_empty() || !middle.chars().all(|c| c == '-') {
            return None;
        }
        alignments.push(match (left, right) {
            (true, true) => Alignment::Center,
            (true, false) => Alignment::Left,
            (false, true) => Alignment::Right,
            (false, false) => Alignment::None,
        });
    }
    Some(alignments)
}

// ---------------------------------------------------------------------------
// HTML block
// ---------------------------------------------------------------------------

fn parse_html_block(lex: &mut Lexer<'_>, errors: &mut Vec<ParseError>) -> Block {
    let tok = lex.advance();
    let open_span = tok.span;
    let open_tag = match &tok.kind {
        Token::HtmlOpen { tag } => tag.clone(),
        _ => unreachable!(),
    };

    let raw = consume_raw_line(lex);
    skip_newline(lex);

    let mut raw_lines = vec![raw.clone()];
    let mut last_span = open_span;

    let trimmed = raw.trim();
    let is_self_closing = trimmed.ends_with("/>")
        || is_void_element(&open_tag)
        || trimmed.contains(&format!("</{open_tag}"));

    if !is_self_closing {
        loop {
            if lex.is_eof() {
                errors.push(ParseError {
                    kind: ParseErrorKind::UnclosedHtmlBlock,
                    span: open_span,
                    message: format!("HTML block <{open_tag}> never closed"),
                });
                break;
            }
            if matches!(lex.peek().kind, Token::BlankLine) {
                break;
            }
            let is_close = matches!(&lex.peek().kind, Token::HtmlClose { tag } if *tag == open_tag);
            let line_span = lex.peek().span;
            let line_raw = extract_raw_line(lex);
            raw_lines.push(line_raw);
            last_span = line_span;
            if is_close {
                break;
            }
        }
    }

    Block::HtmlBlock(HtmlBlock {
        raw: raw_lines.join("\n"),
        span: open_span.merge(last_span),
    })
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

fn parse_line_comment(lex: &mut Lexer<'_>) -> Block {
    let span = lex.advance().span; // LineComment
    let raw = consume_raw_line(lex);
    skip_newline(lex);
    let text = raw
        .trim_start()
        .strip_prefix("//")
        .unwrap_or(&raw)
        .trim()
        .to_string();
    Block::Comment(Comment { text, span })
}

fn parse_block_comment(lex: &mut Lexer<'_>) -> Block {
    let open_span = lex.advance().span;
    let first_raw = consume_raw_line(lex);
    skip_newline(lex);

    let mut text_lines = vec![
        first_raw
            .trim_start()
            .strip_prefix("/*")
            .unwrap_or("")
            .trim()
            .to_string(),
    ];
    let mut last_span = open_span;

    loop {
        if lex.is_eof() {
            break;
        }
        if matches!(lex.peek().kind, Token::BlockCommentClose) {
            last_span = lex.advance().span;
            let raw = consume_raw_line(lex);
            skip_newline(lex);
            let trimmed = raw.trim();
            let without = trimmed.strip_suffix("*/").unwrap_or(trimmed);
            if !without.trim().is_empty() {
                text_lines.push(without.trim().to_string());
            }
            break;
        }
        last_span = lex.peek().span;
        let raw = extract_raw_line(lex);
        text_lines.push(raw);
    }

    Block::Comment(Comment {
        text: text_lines.join("\n"),
        span: open_span.merge(last_span),
    })
}

// ---------------------------------------------------------------------------
// Footnote definition
// ---------------------------------------------------------------------------

fn parse_footnote_def(lex: &mut Lexer<'_>) -> Block {
    let tok = lex.advance();
    let span = tok.span;
    let label = match &tok.kind {
        Token::FootnoteDefStart { label } => label.clone(),
        _ => unreachable!(),
    };
    let raw = consume_raw_line(lex);
    skip_newline(lex);

    let prefix = format!("[^{label}]: ");
    let content = match raw.trim().strip_prefix(&prefix) {
        Some(content_text) => {
            build_inline_content(content_text, sub_span(span, &raw, content_text))
        }
        None => InlineContent::empty(),
    };

    Block::FootnoteDefinition(FootnoteDefinition {
        label,
        content,
        span,
    })
}

// ---------------------------------------------------------------------------
// Block tag
// ---------------------------------------------------------------------------

fn parse_block_tag(lex: &mut Lexer<'_>) -> Block {
    let tok = lex.advance();
    let span = tok.span;

    let name = match &tok.kind {
        Token::Tag(kw) => kw.as_str().to_string(),
        Token::UnknownTag { name } => name.clone(),
        _ => unreachable!(),
    };

    // Consume optional argument
    let arg_string = if matches!(lex.peek().kind, Token::TagArg(_)) {
        let arg_tok = lex.advance();
        match &arg_tok.kind {
            Token::TagArg(a) => Some(a.clone()),
            _ => None,
        }
    } else {
        None
    };
    skip_newline(lex);

    Block::BlockTag(tags::parse_tag(&name, arg_string.as_deref(), span))
}

// ---------------------------------------------------------------------------
// Paragraph
// ---------------------------------------------------------------------------

fn parse_paragraph(lex: &mut Lexer<'_>) -> Option<Block> {
    let first_span = lex.peek().span;
    let mut text_lines: Vec<String> = Vec::new();
    let mut last_span = first_span;

    while matches!(
        &lex.peek().kind,
        Token::Text(_)
            | Token::RawLine(_)
            | Token::BlockquoteContinuation
            | Token::HtmlClose { .. }
            | Token::PropertiesOpen
            | Token::PropertiesClose
            | Token::BlockCommentClose
    ) {
        last_span = lex.peek().span;
        let raw = extract_raw_line(lex);
        text_lines.push(raw);
    }

    if text_lines.is_empty() {
        return None;
    }

    let full_text = text_lines.join("\n");
    let span = first_span.merge(last_span);
    let content = build_inline_content(&full_text, span);

    Some(Block::Paragraph(Paragraph { content, span }))
}

// ---------------------------------------------------------------------------
// Inline content builder (uses lexer::tokenize_inline)
// ---------------------------------------------------------------------------

/// Span for a subslice `sub` of a line's raw text `raw`, where `outer` is the
/// span of the full line (`outer.start` = byte offset of `raw[0]` in the
/// source, `outer.col` = its byte column). `sub` must be a subslice of `raw`.
fn sub_span(outer: Span, raw: &str, sub: &str) -> Span {
    let delta = sub.as_ptr() as usize - raw.as_ptr() as usize;
    debug_assert!(
        delta + sub.len() <= raw.len(),
        "sub is not a subslice of raw"
    );
    Span::new(
        outer.start + delta,
        outer.start + delta + sub.len(),
        outer.line,
        outer.col + delta as u32,
    )
}

fn build_inline_content(text: &str, span: Span) -> InlineContent {
    let tokens = lexer::tokenize_inline(text, span);
    tokens_to_inline_content(&tokens)
}

/// Build an inline segment for a delimited run (bold/italic/strikethrough).
/// `open_span` is the opening delimiter's span; the closing delimiter (or, if
/// unclosed, the last inner token) bounds the segment's end.
fn delimited_span(open_span: Span, inner: &[crate::tokens::Spanned], close: Option<Span>) -> Span {
    match close {
        Some(close_span) => open_span.merge(close_span),
        None => match inner.last() {
            Some(last) => open_span.merge(last.span),
            None => open_span,
        },
    }
}

fn tokens_to_inline_content(tokens: &[crate::tokens::Spanned]) -> InlineContent {
    let mut segments: Vec<InlineSegment> = Vec::new();
    let mut i = 0;

    let mut push = |kind: InlineKind, span: Span| {
        segments.push(InlineSegment { kind, span });
    };

    while i < tokens.len() {
        let tok_span = tokens[i].span;
        match &tokens[i].kind {
            Token::Text(t) => {
                push(InlineKind::Text(t.clone()), tok_span);
                i += 1;
            }
            Token::InlineCode(c) => {
                push(InlineKind::Code(c.clone()), tok_span);
                i += 1;
            }
            Token::BoldDelim => {
                // Collect inner tokens until matching BoldDelim
                i += 1;
                let inner_end = find_matching_delim(&tokens[i..], Token::BoldDelim);
                let inner_toks = &tokens[i..i + inner_end];
                let close = tokens.get(i + inner_end).map(|t| t.span);
                let inner = tokens_to_inline_content(inner_toks);
                push(
                    InlineKind::Bold(inner),
                    delimited_span(tok_span, inner_toks, close),
                );
                i += inner_end + 1; // skip closing delim
            }
            Token::ItalicDelim => {
                i += 1;
                let inner_end = find_matching_delim(&tokens[i..], Token::ItalicDelim);
                let inner_toks = &tokens[i..i + inner_end];
                let close = tokens.get(i + inner_end).map(|t| t.span);
                let inner = tokens_to_inline_content(inner_toks);
                push(
                    InlineKind::Italic(inner),
                    delimited_span(tok_span, inner_toks, close),
                );
                i += inner_end + 1;
            }
            Token::StrikethroughDelim => {
                i += 1;
                let inner_end = find_matching_delim(&tokens[i..], Token::StrikethroughDelim);
                let inner_toks = &tokens[i..i + inner_end];
                let close = tokens.get(i + inner_end).map(|t| t.span);
                let inner = tokens_to_inline_content(inner_toks);
                push(
                    InlineKind::Strikethrough(inner),
                    delimited_span(tok_span, inner_toks, close),
                );
                i += inner_end + 1;
            }
            Token::Link {
                text,
                url,
                title,
                meta,
            } => {
                let (link_tags, attrs) = match meta.as_deref() {
                    Some(m) => parse_metadata(m, tok_span),
                    None => (Vec::new(), HashMap::new()),
                };
                push(
                    InlineKind::Link(Link {
                        text: text.clone(),
                        url: url.clone(),
                        title: title.clone(),
                        tags: link_tags,
                        attributes: attrs,
                    }),
                    tok_span,
                );
                i += 1;
            }
            Token::FootnoteRef { label } => {
                push(InlineKind::FootnoteRef(label.clone()), tok_span);
                i += 1;
            }
            Token::Cite { key, locator } => {
                push(
                    InlineKind::Cite {
                        key: key.clone(),
                        locator: locator.clone(),
                    },
                    tok_span,
                );
                i += 1;
            }
            Token::Tag(kw) => {
                let (arg, span) = take_tag_arg(tokens, &mut i, tok_span);
                let tag = tags::parse_tag(kw.as_str(), arg, span);
                push(InlineKind::Tag(tag), span);
                i += 1;
            }
            Token::UnknownTag { name } => {
                let (arg, span) = take_tag_arg(tokens, &mut i, tok_span);
                let tag = tags::parse_tag(name, arg, span);
                push(InlineKind::Tag(tag), span);
                i += 1;
            }
            Token::TagArg(a) => {
                // Stray tag arg without a preceding tag — treat as text
                push(InlineKind::Text(a.clone()), tok_span);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    InlineContent { segments }
}

/// If the token after `*i` is a `TagArg`, consume it and return the argument
/// text plus the tag span extended to cover the argument.
fn take_tag_arg<'a>(
    tokens: &'a [crate::tokens::Spanned],
    i: &mut usize,
    tag_span: Span,
) -> (Option<&'a str>, Span) {
    if let Some(next) = tokens.get(*i + 1)
        && let Token::TagArg(a) = &next.kind
    {
        *i += 1;
        (Some(a.as_str()), tag_span.merge(next.span))
    } else {
        (None, tag_span)
    }
}

fn find_matching_delim(tokens: &[crate::tokens::Spanned], delim: Token) -> usize {
    for (idx, tok) in tokens.iter().enumerate() {
        if tok.kind == delim {
            return idx;
        }
    }
    tokens.len() // no match found — consume everything
}

// ---------------------------------------------------------------------------
// Token stream helpers
// ---------------------------------------------------------------------------

/// Consume all tokens until the next Newline/Eof, collecting RawLine content.
/// Returns the raw text of the line.
fn extract_raw_line(lex: &mut Lexer<'_>) -> String {
    let mut raw = String::new();
    loop {
        match &lex.peek().kind {
            Token::Newline | Token::Eof => {
                if matches!(lex.peek().kind, Token::Newline) {
                    lex.advance();
                }
                break;
            }
            Token::RawLine(text) => {
                raw = text.clone();
                lex.advance();
            }
            _ => {
                lex.advance();
            }
        }
    }
    raw
}

/// Consume and return the RawLine text if present (without advancing past Newline).
fn consume_raw_line(lex: &mut Lexer<'_>) -> String {
    if let Token::RawLine(text) = &lex.peek().kind {
        let t = text.clone();
        lex.advance();
        t
    } else {
        String::new()
    }
}

fn skip_newline(lex: &mut Lexer<'_>) {
    if matches!(lex.peek().kind, Token::Newline) {
        lex.advance();
    }
}

fn skip_blank_lines(lex: &mut Lexer<'_>) {
    while matches!(lex.peek().kind, Token::BlankLine) {
        lex.advance();
        skip_newline(lex);
    }
}

// ===========================================================================
// Tests — mirror key tests from parser.rs against parser_v2
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::TagKind;

    #[test]
    fn test_v2_full_document() {
        let src = "---\ntitle: Test\n---\n\n# Heading\n\nSome text with #todo inline tag.\n\n```rust #tangle file=main.rs\nfn main() {}\n```\n\n#deadline 2026-04-10\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let doc = &result.document;
        assert!(doc.frontmatter.is_some());

        let has_heading = doc
            .children
            .iter()
            .any(|b| matches!(b, Block::Heading(h) if h.level == 1));
        assert!(has_heading, "should have heading");

        let code_block = doc.children.iter().find_map(|b| match b {
            Block::CodeBlock(cb) => Some(cb),
            _ => None,
        });
        assert!(code_block.is_some(), "should have code block");
        let cb = code_block.unwrap();
        assert_eq!(cb.lang.as_deref(), Some("rust"));
        assert!(cb.tags.iter().any(|t| matches!(t.kind, TagKind::Tangle)));
        assert_eq!(
            cb.attributes.get("file").map(|s| s.as_str()),
            Some("main.rs")
        );
        assert_eq!(cb.body, "fn main() {}");

        let has_deadline = doc.children.iter().any(|b| {
            matches!(
                b,
                Block::BlockTag(Tag {
                    kind: TagKind::Deadline { .. },
                    ..
                })
            )
        });
        assert!(has_deadline, "should have deadline");
    }

    #[test]
    fn test_v2_inline_tags() {
        let src = "some text #todo fix this\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let para = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .unwrap();

        let tags: Vec<_> = para.content.tags();
        assert_eq!(tags.len(), 1);
        assert!(matches!(tags[0].kind, TagKind::Todo { .. }));
    }

    #[test]
    fn test_v2_bold_italic() {
        let src = "**bold** and *italic* text\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let para = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .unwrap();

        assert!(
            para.content
                .segments
                .iter()
                .any(|s| matches!(s.kind, InlineKind::Bold(_)))
        );
        assert!(
            para.content
                .segments
                .iter()
                .any(|s| matches!(s.kind, InlineKind::Italic(_)))
        );
    }

    #[test]
    fn test_v2_link() {
        let src = "[click](https://example.com)\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let para = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .unwrap();

        assert!(
            para.content
                .segments
                .iter()
                .any(|s| matches!(s.kind, InlineKind::Link(_)))
        );
    }

    #[test]
    fn test_v2_table() {
        let src = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let table = result.document.children.iter().find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        });
        assert!(table.is_some());
        let t = table.unwrap();
        assert_eq!(t.headers.len(), 2);
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn test_v2_callout() {
        let src = "> [!note]\n> This is a note.\n> With two lines.\n\nRegular text.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let callout = result.document.children.iter().find_map(|b| match b {
            Block::Callout(c) => Some(c),
            _ => None,
        });
        assert!(callout.is_some());
        assert_eq!(callout.unwrap().kind, "note");
    }

    #[test]
    fn test_v2_list_with_checkboxes() {
        let src = "- [ ] Unchecked task\n- [x] Checked task\n- Regular item\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let list = result.document.children.iter().find_map(|b| match b {
            Block::List(l) => Some(l),
            _ => None,
        });
        let l = list.unwrap();
        assert_eq!(l.items.len(), 3);
        assert_eq!(l.items[0].checkbox, Some(Checkbox::Unchecked));
        assert_eq!(l.items[1].checkbox, Some(Checkbox::Checked));
        assert_eq!(l.items[2].checkbox, None);
    }

    #[test]
    fn test_v2_property_drawer() {
        let src = "## My Task\n\n#properties\nid = abc-123\neffort = 2h30m\n#end\n\nContent.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let h = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Heading(h) => Some(h),
                _ => None,
            })
            .unwrap();
        assert!(h.properties.is_some());
        let props = h.properties.as_ref().unwrap();
        assert_eq!(props.entries.get("id").map(|s| s.as_str()), Some("abc-123"));
    }

    #[test]
    fn test_v2_clock_tags() {
        let src = "#clock-in 2026-04-03T09:00\n#clock-out 2026-04-03T10:30\n#clock 1h30m\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let tags: Vec<_> = result
            .document
            .children
            .iter()
            .filter_map(|b| match b {
                Block::BlockTag(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tags.len(), 3);
        assert!(matches!(tags[0].kind, TagKind::ClockIn { .. }));
        assert!(matches!(tags[1].kind, TagKind::ClockOut { .. }));
        assert!(matches!(tags[2].kind, TagKind::Clock(_)));
    }

    #[test]
    fn test_v2_comments() {
        let src = "// this is a comment\n\nText.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let has_comment = result
            .document
            .children
            .iter()
            .any(|b| matches!(b, Block::Comment(_)));
        assert!(has_comment);
    }

    #[test]
    fn test_v2_horizontal_rule() {
        let src = "Text above.\n\n***\n\nText below.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let has_hr = result
            .document
            .children
            .iter()
            .any(|b| matches!(b, Block::HorizontalRule(_)));
        assert!(has_hr);
    }

    #[test]
    fn test_v2_footnote() {
        let src = "[^1]: This is a footnote.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let has_fn = result
            .document
            .children
            .iter()
            .any(|b| matches!(b, Block::FootnoteDefinition(_)));
        assert!(has_fn);
    }

    #[test]
    fn test_v2_html_block() {
        let src = "<div class=\"container\">\n  <p>Hello</p>\n</div>\n";
        let result = parse_document(src);

        let html = result.document.children.iter().find_map(|b| match b {
            Block::HtmlBlock(h) => Some(h),
            _ => None,
        });
        assert!(html.is_some());
        assert!(html.unwrap().raw.contains("<div"));
    }

    #[test]
    fn test_v2_unclosed_fence_recovery() {
        let src = "```rust\nfn main() {}\n\n# Next heading\n";
        let result = parse_document(src);
        assert!(!result.errors.is_empty());
        assert!(!result.document.children.is_empty());
    }

    #[test]
    fn test_v2_nested_list() {
        let src =
            "- Parent one\n  - Child A\n  - Child B\n- Parent two\n  - Child C\n    - Grandchild\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let list = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::List(l) => Some(l),
                _ => None,
            })
            .expect("should have a list");

        // Two top-level items
        assert_eq!(list.items.len(), 2, "should have 2 top-level items");

        // First parent has 2 children
        assert_eq!(list.items[0].content.plain_text(), "Parent one");
        assert_eq!(
            list.items[0].children.len(),
            1,
            "parent one should have 1 child block (nested list)"
        );
        if let Block::List(child_list) = &list.items[0].children[0] {
            assert_eq!(child_list.items.len(), 2, "child list should have 2 items");
            assert_eq!(child_list.items[0].content.plain_text(), "Child A");
            assert_eq!(child_list.items[1].content.plain_text(), "Child B");
        } else {
            panic!("expected nested list");
        }

        // Second parent has 1 child with its own grandchild
        assert_eq!(list.items[1].content.plain_text(), "Parent two");
        if let Block::List(child_list) = &list.items[1].children[0] {
            assert_eq!(child_list.items.len(), 1);
            assert_eq!(child_list.items[0].content.plain_text(), "Child C");
            // Child C has a grandchild
            assert_eq!(child_list.items[0].children.len(), 1);
            if let Block::List(grandchild_list) = &child_list.items[0].children[0] {
                assert_eq!(grandchild_list.items.len(), 1);
                assert_eq!(grandchild_list.items[0].content.plain_text(), "Grandchild");
            } else {
                panic!("expected grandchild list");
            }
        } else {
            panic!("expected nested list for parent two");
        }
    }

    /// Helper: the first paragraph of a parsed document.
    fn first_paragraph(result: &ParseResult) -> &Paragraph {
        result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .expect("document should contain a paragraph")
    }

    fn slice<'a>(src: &'a str, span: &Span) -> &'a str {
        &src[span.start..span.end]
    }

    #[test]
    fn test_inline_spans_slice_source() {
        // Fixture with bold, inline code, a link, and a #tag on one line.
        let src = "Intro **bold** with `code` and [click](https://example.com) plus #todo fix it\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let para = first_paragraph(&result);
        let segs = &para.content.segments;

        type KindCheck = fn(&InlineKind) -> bool;
        let expected: &[(&str, KindCheck)] = &[
            ("Intro ", |k| matches!(k, InlineKind::Text(_))),
            ("**bold**", |k| matches!(k, InlineKind::Bold(_))),
            (" with ", |k| matches!(k, InlineKind::Text(_))),
            ("`code`", |k| matches!(k, InlineKind::Code(_))),
            (" and ", |k| matches!(k, InlineKind::Text(_))),
            ("[click](https://example.com)", |k| {
                matches!(k, InlineKind::Link(_))
            }),
            (" plus ", |k| matches!(k, InlineKind::Text(_))),
            ("#todo fix it", |k| matches!(k, InlineKind::Tag(_))),
        ];

        assert_eq!(segs.len(), expected.len(), "segments: {segs:#?}");
        for (seg, (text, kind_ok)) in segs.iter().zip(expected) {
            assert!(kind_ok(&seg.kind), "unexpected kind for {text:?}: {seg:#?}");
            assert_eq!(slice(src, &seg.span), *text);
            assert_eq!(seg.span.line, 1);
            // Byte column is 1-based: start offset on line 1 is col - 1.
            assert_eq!(seg.span.col as usize, seg.span.start + 1);
        }

        // Inner segment of the bold run points at "bold" itself.
        if let InlineKind::Bold(inner) = &segs[1].kind {
            assert_eq!(slice(src, &inner.segments[0].span), "bold");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn test_inline_spans_multibyte_and_offset_lines() {
        // Heading + blank line push the paragraph to line 3; "café" has a
        // two-byte 'é' so byte offsets and byte columns diverge from chars.
        let src = "# Título\n\ncafé **gras** et `α` fin\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        // Heading content span slices to the text after "# ".
        let heading = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Heading(h) => Some(h),
                _ => None,
            })
            .unwrap();
        assert_eq!(slice(src, &heading.content.segments[0].span), "Título");
        assert_eq!(heading.content.segments[0].span.line, 1);
        assert_eq!(heading.content.segments[0].span.col, 3);

        let para = first_paragraph(&result);
        let segs = &para.content.segments;
        assert_eq!(slice(src, &segs[0].span), "café ");
        assert_eq!(slice(src, &segs[1].span), "**gras**");
        assert_eq!(slice(src, &segs[2].span), " et ");
        assert_eq!(slice(src, &segs[3].span), "`α`");
        assert_eq!(slice(src, &segs[4].span), " fin");
        for seg in segs {
            assert_eq!(seg.span.line, 3);
        }
        // "café " is 6 bytes, so the bold run starts at byte column 7.
        assert_eq!(segs[1].span.col, 7);
        assert!(matches!(&segs[0].kind, InlineKind::Text(t) if t == "café "));
    }

    #[test]
    fn test_inline_spans_multiline_paragraph() {
        let src = "first line\nsecond **b** line\n";
        let result = parse_document(src);
        let para = first_paragraph(&result);

        let bold = para
            .content
            .segments
            .iter()
            .find(|s| matches!(s.kind, InlineKind::Bold(_)))
            .unwrap();
        assert_eq!(slice(src, &bold.span), "**b**");
        assert_eq!(bold.span.line, 2);
        assert_eq!(bold.span.col, 8);
    }

    #[test]
    fn test_inline_spans_list_item_and_footnote_ref() {
        let src = "- [ ] task with `code` and [^1]\n";
        let result = parse_document(src);
        let list = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::List(l) => Some(l),
                _ => None,
            })
            .unwrap();

        let segs = &list.items[0].content.segments;
        assert_eq!(slice(src, &segs[0].span), "task with ");
        assert_eq!(slice(src, &segs[1].span), "`code`");
        assert_eq!(slice(src, &segs[3].span), "[^1]");
        assert!(matches!(segs[3].kind, InlineKind::FootnoteRef(_)));
    }

    #[test]
    fn test_cite_segment_in_paragraph() {
        let src = "Evidence from [@paszke_pytorch_2019] and [@martin_adapting_2021-1, p. 4].\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let para = first_paragraph(&result);
        let cites: Vec<_> = para
            .content
            .segments
            .iter()
            .filter(|s| matches!(s.kind, InlineKind::Cite { .. }))
            .collect();
        assert_eq!(cites.len(), 2, "segments: {:#?}", para.content.segments);

        assert!(matches!(
            &cites[0].kind,
            InlineKind::Cite { key, locator: None } if key == "paszke_pytorch_2019"
        ));
        assert_eq!(slice(src, &cites[0].span), "[@paszke_pytorch_2019]");

        assert!(matches!(
            &cites[1].kind,
            InlineKind::Cite { key, locator: Some(loc) }
                if key == "martin_adapting_2021-1" && loc == "p. 4"
        ));
        assert_eq!(
            slice(src, &cites[1].span),
            "[@martin_adapting_2021-1, p. 4]"
        );
    }

    #[test]
    fn test_cite_segment_multibyte_spans() {
        // Multi-byte text around the cite; the paragraph sits on line 3.
        let src = "# Título\n\ncafé [@key_2020, § 2–3] après\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let para = first_paragraph(&result);
        let cite = para
            .content
            .segments
            .iter()
            .find(|s| matches!(s.kind, InlineKind::Cite { .. }))
            .expect("should have a cite segment");
        assert_eq!(slice(src, &cite.span), "[@key_2020, § 2–3]");
        assert_eq!(cite.span.line, 3);
        // "café " is 6 bytes, so the cite starts at byte column 7.
        assert_eq!(cite.span.col, 7);
        assert!(matches!(
            &cite.kind,
            InlineKind::Cite { key, locator: Some(loc) } if key == "key_2020" && loc == "§ 2–3"
        ));
    }

    #[test]
    fn test_cite_plain_text_fallbacks_parse_as_text() {
        let src = "bad [@] and [@unclosed\n";
        let result = parse_document(src);
        let para = first_paragraph(&result);
        assert!(
            para.content
                .segments
                .iter()
                .all(|s| !matches!(s.kind, InlineKind::Cite { .. })),
            "no cite expected: {:#?}",
            para.content.segments
        );
        assert_eq!(para.content.plain_text(), "bad [@] and [@unclosed");
    }

    #[test]
    fn test_frontmatter_entry_spans_slice_source() {
        let src = "---\ntitle: Test note\ntags:\n  - alpha\n  - beta\ncount: 3\n---\n\nBody.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();
        assert_eq!(
            fm.raw,
            "title: Test note\ntags:\n  - alpha\n  - beta\ncount: 3"
        );

        let keys: Vec<&str> = fm.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["title", "tags", "count"]);

        let title = fm.entry("title").unwrap();
        assert_eq!(slice(src, &title.key_span), "title");
        assert_eq!(slice(src, &title.value_span), "Test note");
        assert_eq!(title.key_span.line, 2);
        assert_eq!(title.key_span.col, 1);

        let tags = fm.entry("tags").unwrap();
        assert_eq!(slice(src, &tags.key_span), "tags");
        assert_eq!(slice(src, &tags.value_span), "- alpha\n  - beta");
        assert_eq!(tags.key_span.line, 3);

        let count = fm.entry("count").unwrap();
        assert_eq!(slice(src, &count.key_span), "count");
        assert_eq!(slice(src, &count.value_span), "3");
        assert_eq!(count.key_span.line, 6);

        // The typed data is still a plain value tree.
        assert_eq!(
            fm.data.as_mapping_get("title").and_then(|v| v.as_str()),
            Some("Test note")
        );
        assert_eq!(
            fm.data.as_mapping_get("count").and_then(|v| v.as_integer()),
            Some(3)
        );
        assert_eq!(
            fm.data
                .as_mapping_get("tags")
                .and_then(|v| v.as_sequence())
                .map(|s| s.len()),
            Some(2)
        );
    }

    #[test]
    fn test_frontmatter_entry_spans_multibyte() {
        // Multi-byte chars in keys and values: saphyr markers are
        // char-indexed, so byte-exact slicing exercises the conversion.
        let src = "---\ntítulo: Café crème\nétiquettes:\n  - détail\nn: 1\n---\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();

        let titulo = fm.entry("título").unwrap();
        assert_eq!(slice(src, &titulo.key_span), "título");
        assert_eq!(slice(src, &titulo.value_span), "Café crème");
        assert_eq!(titulo.key_span.line, 2);
        assert_eq!(titulo.key_span.col, 1);

        let etiquettes = fm.entry("étiquettes").unwrap();
        assert_eq!(slice(src, &etiquettes.key_span), "étiquettes");
        assert_eq!(slice(src, &etiquettes.value_span), "- détail");
        assert_eq!(etiquettes.key_span.line, 3);

        let n = fm.entry("n").unwrap();
        assert_eq!(slice(src, &n.key_span), "n");
        assert_eq!(slice(src, &n.value_span), "1");
        assert_eq!(n.key_span.line, 5);
    }

    #[test]
    fn test_frontmatter_quoted_and_empty_values() {
        let src = "---\nquoted: \"hello world\"\nempty:\n---\nBody.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();

        let quoted = fm.entry("quoted").unwrap();
        // The value span covers the source markup, quotes included.
        assert_eq!(slice(src, &quoted.value_span), "\"hello world\"");
        assert_eq!(
            fm.data.as_mapping_get("quoted").and_then(|v| v.as_str()),
            Some("hello world")
        );

        let empty = fm.entry("empty").unwrap();
        assert_eq!(slice(src, &empty.key_span), "empty");
        assert_eq!(slice(src, &empty.value_span), "");
    }

    #[test]
    fn test_frontmatter_nested_nodes_relations_shape() {
        // Block sequence of flow mappings — the `relations:` doc pattern.
        let src = "---\nrelations:\n  - { to: squares, stance: supports, op: analyze }\n  - { to: circles#c2, stance: opposes }\n---\nBody.\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();
        let relations = fm.entry("relations").unwrap();
        // The flat span and the node span are the same thing.
        assert_eq!(relations.value.span, relations.value_span);

        let items = relations.value.items().expect("relations is a sequence");
        assert_eq!(items.len(), 2);

        // Item spans cover the flow mapping's source markup, braces included.
        assert_eq!(
            slice(src, &items[0].span),
            "{ to: squares, stance: supports, op: analyze }"
        );
        assert_eq!(items[0].span.line, 3);
        assert_eq!(
            slice(src, &items[1].span),
            "{ to: circles#c2, stance: opposes }"
        );
        assert_eq!(items[1].span.line, 4);

        // Each inner key/value pair slices the source exactly.
        let FrontmatterNodeKind::Map(pairs) = &items[0].kind else {
            panic!("expected a mapping, got {:?}", items[0].kind);
        };
        let expected = [("to", "squares"), ("stance", "supports"), ("op", "analyze")];
        assert_eq!(pairs.len(), expected.len());
        for (pair, (key, value)) in pairs.iter().zip(expected) {
            assert_eq!(pair.key, key);
            assert_eq!(slice(src, &pair.key_span), key);
            assert_eq!(slice(src, &pair.value.span), value);
            assert!(matches!(pair.value.kind, FrontmatterNodeKind::Scalar));
        }

        let to = items[1].entry("to").unwrap();
        assert_eq!(slice(src, &to.key_span), "to");
        assert_eq!(slice(src, &to.value.span), "circles#c2");
        let stance = items[1].entry("stance").unwrap();
        assert_eq!(slice(src, &stance.value.span), "opposes");
        assert!(items[1].entry("op").is_none());
    }

    #[test]
    fn test_frontmatter_nested_nodes_block_sequence_items() {
        let src = "---\naliases:\n  - first alias\n  - \"quoted one\"\n  - last\n---\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();
        let items = fm.entry("aliases").unwrap().value.items().unwrap();
        assert_eq!(items.len(), 3);
        // Item spans cover the item text (markup included), not the `- `.
        assert_eq!(slice(src, &items[0].span), "first alias");
        assert_eq!(items[0].span.line, 3);
        assert_eq!(items[0].span.col, 5);
        assert_eq!(slice(src, &items[1].span), "\"quoted one\"");
        assert_eq!(slice(src, &items[2].span), "last");
        assert_eq!(items[2].span.line, 5);
    }

    #[test]
    fn test_frontmatter_nested_nodes_multibyte() {
        // Multi-byte content in nested values: markers are char-indexed,
        // byte-exact slicing exercises the conversion at every depth.
        let src = "---\nrelations:\n  - { à: café crème, cible: carrés }\nétiquettes: [détail, plus—loin]\n---\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();
        let items = fm.entry("relations").unwrap().value.items().unwrap();
        assert_eq!(
            slice(src, &items[0].span),
            "{ à: café crème, cible: carrés }"
        );
        let a = items[0].entry("à").unwrap();
        assert_eq!(slice(src, &a.key_span), "à");
        assert_eq!(slice(src, &a.value.span), "café crème");
        let cible = items[0].entry("cible").unwrap();
        assert_eq!(slice(src, &cible.value.span), "carrés");

        // Flow sequence with multi-byte scalars.
        let etiquettes = fm.entry("étiquettes").unwrap();
        assert_eq!(slice(src, &etiquettes.value.span), "[détail, plus—loin]");
        let tags = etiquettes.value.items().unwrap();
        assert_eq!(slice(src, &tags[0].span), "détail");
        assert_eq!(slice(src, &tags[1].span), "plus—loin");
    }

    #[test]
    fn test_frontmatter_nested_nodes_deep_nesting() {
        let src =
            "---\nouter:\n  middle:\n    - inner: [1, 2]\n      other:\n        deep: yes\n---\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();
        let outer = &fm.entry("outer").unwrap().value;
        assert_eq!(
            slice(src, &outer.span),
            "middle:\n    - inner: [1, 2]\n      other:\n        deep: yes"
        );

        let middle = outer.entry("middle").unwrap();
        assert_eq!(slice(src, &middle.key_span), "middle");
        let items = middle.value.items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            slice(src, &items[0].span),
            "inner: [1, 2]\n      other:\n        deep: yes"
        );

        let inner = items[0].entry("inner").unwrap();
        assert_eq!(slice(src, &inner.value.span), "[1, 2]");
        let nums = inner.value.items().unwrap();
        assert_eq!(slice(src, &nums[0].span), "1");
        assert_eq!(slice(src, &nums[1].span), "2");

        let deep = items[0]
            .entry("other")
            .unwrap()
            .value
            .entry("deep")
            .unwrap();
        assert_eq!(slice(src, &deep.key_span), "deep");
        assert_eq!(slice(src, &deep.value.span), "yes");
        assert_eq!(deep.value.span.line, 6);
        assert_eq!(deep.value.span.col, 15);
    }

    #[test]
    fn test_frontmatter_nested_nodes_empty_and_edge_values() {
        let src =
            "---\nscalar: plain\nempty:\nmap:\n  present: 1\n  absent:\nseq: []\nflow: {}\n---\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let fm = result.document.frontmatter.as_ref().unwrap();

        // A scalar entry is a Scalar leaf whose node span equals value_span.
        let scalar = fm.entry("scalar").unwrap();
        assert!(matches!(scalar.value.kind, FrontmatterNodeKind::Scalar));
        assert_eq!(scalar.value.span, scalar.value_span);
        assert!(scalar.value.items().is_none());
        assert!(scalar.value.entry("anything").is_none());

        // An empty top-level value is an empty-span Scalar.
        let empty = fm.entry("empty").unwrap();
        assert!(matches!(empty.value.kind, FrontmatterNodeKind::Scalar));
        assert_eq!(slice(src, &empty.value.span), "");

        // An empty nested mapping value too.
        let absent = fm.entry("map").unwrap().value.entry("absent").unwrap();
        assert_eq!(slice(src, &absent.key_span), "absent");
        assert!(matches!(absent.value.kind, FrontmatterNodeKind::Scalar));
        assert_eq!(slice(src, &absent.value.span), "");

        // Empty flow collections keep their kind with no children.
        let seq = fm.entry("seq").unwrap();
        assert_eq!(seq.value.items(), Some(&[][..]));
        assert_eq!(slice(src, &seq.value.span), "[]");
        let flow = fm.entry("flow").unwrap();
        assert!(matches!(&flow.value.kind, FrontmatterNodeKind::Map(m) if m.is_empty()));
        assert_eq!(slice(src, &flow.value.span), "{}");
    }

    #[test]
    fn test_frontmatter_empty_and_invalid() {
        // Empty frontmatter parses to a null value with no entries.
        let result = parse_document("---\n---\nBody.\n");
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        let fm = result.document.frontmatter.as_ref().unwrap();
        assert!(fm.raw.is_empty());
        assert!(fm.entries.is_empty());
        assert!(fm.data.is_null());

        // Invalid YAML is reported and drops the frontmatter, as before.
        let result = parse_document("---\na: [unclosed\n---\nBody.\n");
        assert!(result.document.frontmatter.is_none());
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e.kind, ParseErrorKind::InvalidYaml))
        );
    }

    #[test]
    fn test_anchor_trailing_on_blocks() {
        // Trailing #anchor on a heading, a paragraph, and a list item.
        let src = "# Methods #anchor methods\n\nA cited claim. #anchor claim-1\n\n- evidence item #anchor ev_2021-1\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let anchor_name = |content: &InlineContent| -> Option<String> {
            content.tags().iter().find_map(|t| match &t.kind {
                TagKind::Anchor { name } => Some(name.clone()),
                _ => None,
            })
        };

        let heading = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::Heading(h) => Some(h),
                _ => None,
            })
            .unwrap();
        assert_eq!(anchor_name(&heading.content).as_deref(), Some("methods"));

        let para = first_paragraph(&result);
        assert_eq!(anchor_name(&para.content).as_deref(), Some("claim-1"));

        let list = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::List(l) => Some(l),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            anchor_name(&list.items[0].content).as_deref(),
            Some("ev_2021-1")
        );
    }

    #[test]
    fn test_anchor_span_slices_source() {
        // Multi-byte text before the anchor; the tag segment span (name +
        // argument) must slice the source exactly. Paragraph is on line 3.
        let src = "# Título\n\ncafé claim #anchor sec-1\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

        let para = first_paragraph(&result);
        let seg = para
            .content
            .segments
            .iter()
            .find(|s| matches!(s.kind, InlineKind::Tag(_)))
            .expect("should have a tag segment");
        assert_eq!(slice(src, &seg.span), "#anchor sec-1");
        assert_eq!(seg.span.line, 3);
        // "café claim " is 12 bytes, so the tag starts at byte column 13.
        assert_eq!(seg.span.col, 13);
        if let InlineKind::Tag(tag) = &seg.kind {
            assert!(matches!(&tag.kind, TagKind::Anchor { name } if name == "sec-1"));
            assert_eq!(slice(src, &tag.span), "#anchor sec-1");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn test_anchor_block_tag() {
        // An #anchor line on its own parses as a block tag.
        let src = "#anchor standalone-1\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());
        assert!(result.document.children.iter().any(|b| matches!(
            b,
            Block::BlockTag(Tag {
                kind: TagKind::Anchor { name },
                ..
            }) if name == "standalone-1"
        )));
    }

    #[test]
    fn test_v2_nested_checkbox_list() {
        let src = "- [ ] Parent task\n  - [x] Subtask done\n  - [ ] Subtask pending\n";
        let result = parse_document(src);
        assert!(result.errors.is_empty());

        let list = result
            .document
            .children
            .iter()
            .find_map(|b| match b {
                Block::List(l) => Some(l),
                _ => None,
            })
            .unwrap();

        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].checkbox, Some(Checkbox::Unchecked));

        if let Block::List(child_list) = &list.items[0].children[0] {
            assert_eq!(child_list.items.len(), 2);
            assert_eq!(child_list.items[0].checkbox, Some(Checkbox::Checked));
            assert_eq!(child_list.items[1].checkbox, Some(Checkbox::Unchecked));
        } else {
            panic!("expected nested list");
        }
    }
}

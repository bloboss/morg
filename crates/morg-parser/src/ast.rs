use std::collections::HashMap;

use crate::span::Span;
use crate::tags::Tag;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub frontmatter: Option<Frontmatter>,
    pub children: Vec<Block>,
}

/// YAML frontmatter delimited by `---` lines at the top of a document.
///
/// `raw` is the exact source text between the delimiters, so spans in
/// `entries` can be checked against it (or against the whole source — all
/// spans are absolute, like inline-segment spans).
#[derive(Debug, Clone, PartialEq)]
pub struct Frontmatter {
    pub raw: String,
    pub data: saphyr::YamlOwned,
    /// Per-entry spans for the top-level mapping, in document order.
    /// Empty when the document root is not a mapping.
    pub entries: Vec<FrontmatterEntry>,
    pub span: Span,
}

impl Frontmatter {
    /// The entry for a top-level key, if present.
    pub fn entry(&self, key: &str) -> Option<&FrontmatterEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
}

/// Source location of one top-level `key: value` frontmatter entry.
///
/// `key_span` covers the key text; `value_span` covers the value's source
/// text (including any quotes or block markers, trailing whitespace
/// trimmed). Both slice the original document source exactly. For an empty
/// value (`key:`), `value_span` is empty and sits just past the colon.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontmatterEntry {
    pub key: String,
    pub key_span: Span,
    pub value_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(Heading),
    Paragraph(Paragraph),
    CodeBlock(CodeBlock),
    BlankLine(Span),
    BlockTag(Tag),
    Callout(Callout),
    Table(Table),
    HtmlBlock(HtmlBlock),
    List(List),
    HorizontalRule(Span),
    Comment(Comment),
    FootnoteDefinition(FootnoteDefinition),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteDefinition {
    pub label: String,
    pub content: InlineContent,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub content: InlineContent,
    pub properties: Option<PropertyDrawer>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDrawer {
    pub entries: HashMap<String, String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub content: InlineContent,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    pub lang: Option<String>,
    pub tags: Vec<Tag>,
    pub attributes: HashMap<String, String>,
    pub body: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Callout {
    pub kind: String,
    pub tags: Vec<Tag>,
    pub attributes: HashMap<String, String>,
    pub content: Vec<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub headers: Vec<InlineContent>,
    pub alignments: Vec<Alignment>,
    pub rows: Vec<Vec<InlineContent>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HtmlBlock {
    pub raw: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub kind: ListKind,
    pub items: Vec<ListItem>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Unordered,
    Ordered,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub checkbox: Option<Checkbox>,
    pub content: InlineContent,
    pub description: Option<InlineContent>,
    pub children: Vec<Block>,
    pub indent: usize,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkbox {
    Unchecked,
    Checked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineContent {
    pub segments: Vec<InlineSegment>,
}

impl InlineContent {
    pub fn plain(text: &str, span: Span) -> Self {
        Self {
            segments: vec![InlineSegment {
                kind: InlineKind::Text(text.to_string()),
                span,
            }],
        }
    }

    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn tags(&self) -> Vec<&Tag> {
        let mut result = Vec::new();
        collect_tags_from_segments(&self.segments, &mut result);
        result
    }

    /// Extract plain text from inline content, stripping all markup.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        plain_text_segments(&self.segments, &mut out);
        out.trim().to_string()
    }
}

fn plain_text_segments(segments: &[InlineSegment], out: &mut String) {
    for seg in segments {
        match &seg.kind {
            InlineKind::Text(t) => out.push_str(t),
            InlineKind::Code(c) => out.push_str(c),
            InlineKind::Tag(_) => {}
            InlineKind::Bold(inner)
            | InlineKind::Italic(inner)
            | InlineKind::Strikethrough(inner) => {
                plain_text_segments(&inner.segments, out);
            }
            InlineKind::Link(link) => out.push_str(&link.text),
            InlineKind::FootnoteRef(label) => {
                out.push_str("[^");
                out.push_str(label);
                out.push(']');
            }
            InlineKind::Cite { key, locator } => {
                out.push_str("[@");
                out.push_str(key);
                if let Some(loc) = locator {
                    out.push_str(", ");
                    out.push_str(loc);
                }
                out.push(']');
            }
        }
    }
}

fn collect_tags_from_segments<'a>(segments: &'a [InlineSegment], out: &mut Vec<&'a Tag>) {
    for seg in segments {
        match &seg.kind {
            InlineKind::Tag(t) => out.push(t),
            InlineKind::Bold(inner)
            | InlineKind::Italic(inner)
            | InlineKind::Strikethrough(inner) => {
                collect_tags_from_segments(&inner.segments, out);
            }
            InlineKind::Link(link) => {
                for t in &link.tags {
                    out.push(t);
                }
            }
            InlineKind::Text(_)
            | InlineKind::Code(_)
            | InlineKind::FootnoteRef(_)
            | InlineKind::Cite { .. } => {}
        }
    }
}

/// A single piece of inline content together with its source location.
///
/// `span.start`/`span.end` are **byte offsets absolute within the source
/// file** passed to [`crate::parse_document`], so
/// `&source[seg.span.start..seg.span.end]` yields exactly the source text of
/// the segment (including its markup, e.g. `**bold**` or `` `code` ``).
/// `span.line` is 1-based; `span.col` is a 1-based *byte* column within that
/// line, matching block-level spans. Use [`crate::line_index::LineIndex`] to
/// convert offsets to UTF-8/UTF-16 columns.
///
/// Exception: content nested inside a callout body is re-parsed from a
/// reassembled buffer, so its offsets are relative to that buffer — the same
/// (pre-existing) convention as block spans inside callouts.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineSegment {
    pub kind: InlineKind,
    pub span: Span,
}

/// The kind of an [`InlineSegment`].
#[derive(Debug, Clone, PartialEq)]
pub enum InlineKind {
    Text(String),
    Tag(Tag),
    Bold(InlineContent),
    Italic(InlineContent),
    Strikethrough(InlineContent),
    Code(String),
    Link(Link),
    FootnoteRef(String),
    /// A Pandoc-style citation: `[@key]` or `[@key, p. 4]`.
    Cite {
        key: String,
        locator: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub title: Option<String>,
    pub tags: Vec<Tag>,
    pub attributes: HashMap<String, String>,
}

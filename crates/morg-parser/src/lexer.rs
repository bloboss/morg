//! Lexer for morg-mode source files.
//!
//! Two-phase design:
//! 1. **Block tokenizer** (`Lexer::new`) classifies each line into a block-level token
//!    plus a `RawLine` carrying the raw text. One token sequence per line, separated by `Newline`.
//! 2. **Inline tokenizer** (`tokenize_inline`) is called by the parser on demand to break
//!    raw text into inline tokens (bold, italic, tags, links, etc.). This is never called
//!    eagerly — the parser controls when inline parsing happens.

use crate::span::Span;
use crate::tokens::{Keyword, Spanned, Token};

// ===========================================================================
// Block-level lexer
// ===========================================================================

/// A lexer that produces block-level tokens from source text.
pub struct Lexer<'a> {
    source: &'a str,
    tokens: Vec<Spanned>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let tokens = tokenize_blocks(source);
        Self {
            source,
            tokens,
            pos: 0,
        }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn peek(&self) -> &Spanned {
        self.tokens.get(self.pos).unwrap_or(&EOF_TOKEN)
    }

    pub fn advance(&mut self) -> &Spanned {
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            tok
        } else {
            &EOF_TOKEN
        }
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek().kind, Token::Eof)
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Skip tokens until (and including) the next Newline or Eof.
    pub fn skip_to_next_line(&mut self) {
        while self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            if matches!(tok.kind, Token::Newline | Token::Eof) {
                return;
            }
        }
    }
}

static EOF_TOKEN: Spanned = Spanned {
    kind: Token::Eof,
    span: Span {
        start: 0,
        end: 0,
        line: 0,
        col: 0,
    },
};

/// Tokenize source into block-level tokens. Each line produces:
/// - One block-classification token (Heading, FencedCodeOpen, BlankLine, etc.)
/// - A `RawLine` token carrying the full line text
/// - A `Newline` token
fn tokenize_blocks(source: &str) -> Vec<Spanned> {
    let mut tokens = Vec::new();
    let mut byte_offset: usize = 0;

    for (line_idx, line_text) in source.split('\n').enumerate() {
        let line_number = (line_idx + 1) as u32;
        let span = Span::new(byte_offset, byte_offset + line_text.len(), line_number, 1);

        classify_line(line_text, span, &mut tokens);

        tokens.push(Spanned {
            kind: Token::Newline,
            span: Span::new(
                byte_offset + line_text.len(),
                byte_offset + line_text.len() + 1,
                line_number,
                (line_text.len() + 1) as u32,
            ),
        });

        byte_offset += line_text.len() + 1;
    }

    // Replace final Newline with Eof
    if let Some(last) = tokens.last_mut()
        && last.kind == Token::Newline
    {
        last.kind = Token::Eof;
    }

    tokens
}

/// Classify a single line into block-level token(s) + RawLine.
fn classify_line(text: &str, span: Span, out: &mut Vec<Spanned>) {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        out.push(Spanned {
            kind: Token::BlankLine,
            span,
        });
        return;
    }

    // Comments
    if trimmed.starts_with("//") {
        out.push(Spanned {
            kind: Token::LineComment,
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }
    if trimmed.starts_with("/*") {
        out.push(Spanned {
            kind: Token::BlockCommentOpen,
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }
    if trimmed.ends_with("*/") || trimmed == "*/" {
        out.push(Spanned {
            kind: Token::BlockCommentClose,
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Footnote definition
    if let Some(label) = try_footnote_def(trimmed) {
        out.push(Spanned {
            kind: Token::FootnoteDefStart { label },
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Frontmatter delimiter
    if trimmed == "---" {
        out.push(Spanned {
            kind: Token::FrontmatterDelim,
            span,
        });
        return;
    }

    // Horizontal rule
    if is_horizontal_rule(trimmed) {
        out.push(Spanned {
            kind: Token::HorizontalRule,
            span,
        });
        return;
    }

    // Code fence
    if let Some(tok) = try_code_fence(trimmed) {
        out.push(Spanned { kind: tok, span });
        return;
    }

    // HTML
    if let Some(tok) = try_html_line(trimmed) {
        out.push(Spanned { kind: tok, span });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Table row
    if trimmed.starts_with('|') {
        out.push(Spanned {
            kind: Token::TableRow,
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Callout
    if let Some((kind, metadata)) = try_callout(trimmed) {
        out.push(Spanned {
            kind: Token::CalloutStart { kind, metadata },
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Blockquote continuation
    if trimmed.starts_with('>') {
        out.push(Spanned {
            kind: Token::BlockquoteContinuation,
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // List item
    if let Some((indent, ordered)) = try_list_item(text) {
        out.push(Spanned {
            kind: Token::ListMarker { ordered, indent },
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Heading
    if let Some(level) = try_heading(text) {
        out.push(Spanned {
            kind: Token::Heading { level },
            span,
        });
        out.push(Spanned {
            kind: Token::RawLine(text.to_string()),
            span,
        });
        return;
    }

    // Properties markers
    if trimmed == "#properties" {
        out.push(Spanned {
            kind: Token::PropertiesOpen,
            span,
        });
        return;
    }
    if trimmed == "#end" {
        out.push(Spanned {
            kind: Token::PropertiesClose,
            span,
        });
        return;
    }

    // Block-level tag check
    if let Some(rest) = trimmed.strip_prefix('#')
        && !rest.is_empty()
        && !rest.starts_with(' ')
    {
        let first = rest.chars().next().unwrap();
        if first.is_alphanumeric() || first == '_' {
            let name_end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .unwrap_or(rest.len());
            let name = &rest[..name_end];
            let tag_tok = match Keyword::from_str(name) {
                Some(kw) => Token::Tag(kw),
                None => Token::UnknownTag {
                    name: name.to_string(),
                },
            };
            out.push(Spanned {
                kind: tag_tok,
                span,
            });
            let arg = rest[name_end..].trim();
            if !arg.is_empty() {
                out.push(Spanned {
                    kind: Token::TagArg(arg.to_string()),
                    span,
                });
            }
            return;
        }
    }

    // Plain text — emit as RawLine for parser to inline-tokenize
    out.push(Spanned {
        kind: Token::Text(String::new()),
        span,
    }); // marker: this is a text line
    out.push(Spanned {
        kind: Token::RawLine(text.to_string()),
        span,
    });
}

// ===========================================================================
// Inline tokenizer (called by parser on demand)
// ===========================================================================

/// Tokenize inline content from raw text. Called by the parser when it needs
/// to break a text line into inline segments (bold, italic, tags, links, etc.).
///
/// `base` describes where `text` sits in the original source file:
/// `base.start` must be the absolute byte offset of `text`'s first byte, and
/// `base.line`/`base.col` the (1-based) line and byte column of that byte.
/// Every returned token then carries a span whose `start`/`end` are absolute
/// byte offsets into the source file and whose `line`/`col` locate the
/// token's first byte (newlines inside `text` are tracked, so multi-line
/// paragraphs get per-line positions).
pub fn tokenize_inline(text: &str, base: Span) -> Vec<Spanned> {
    let mut out = Vec::new();
    tokenize_inline_into(text, base, &mut out);
    out
}

/// Incrementally maps byte indices within a text fragment to spans that are
/// absolute within the original source file. Indices must be visited in
/// non-decreasing order (tokens are emitted left to right).
struct SpanTracker<'t> {
    text: &'t [u8],
    base: Span,
    /// Byte index in `text` that `line`/`col` currently describe.
    scanned: usize,
    line: u32,
    col: u32,
}

impl<'t> SpanTracker<'t> {
    fn new(text: &'t str, base: Span) -> Self {
        Self {
            text: text.as_bytes(),
            base,
            scanned: 0,
            line: base.line,
            col: base.col,
        }
    }

    /// Span for `text[start..end]`, with absolute byte offsets and the
    /// line/col of `start`.
    fn span(&mut self, start: usize, end: usize) -> Span {
        while self.scanned < start {
            if self.text[self.scanned] == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.scanned += 1;
        }
        Span::new(
            self.base.start + start,
            self.base.start + end,
            self.line,
            self.col,
        )
    }
}

fn tokenize_inline_into(text: &str, base: Span, out: &mut Vec<Spanned>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut current_text = String::new();
    // Byte index in `text` where the pending text run began.
    let mut run_start = 0usize;
    let mut tracker = SpanTracker::new(text, base);

    fn flush(
        buf: &mut String,
        run_start: usize,
        run_end: usize,
        tracker: &mut SpanTracker<'_>,
        out: &mut Vec<Spanned>,
    ) {
        if !buf.is_empty() {
            out.push(Spanned {
                kind: Token::Text(std::mem::take(buf)),
                span: tracker.span(run_start, run_end),
            });
        }
    }

    while i < bytes.len() {
        let ch = bytes[i];

        // Backslash escape
        if ch == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'#' || next == b'[' || next == b'*' || next == b'~' || next == b'`' {
                if current_text.is_empty() {
                    run_start = i;
                }
                current_text.push(next as char);
                i += 2;
                continue;
            }
            if current_text.is_empty() {
                run_start = i;
            }
            current_text.push('\\');
            i += 1;
            continue;
        }

        // Inline code
        if ch == b'`'
            && let Some((code, end)) = scan_backtick_code(text, i)
        {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::InlineCode(code.to_string()),
                span: tracker.span(i, end),
            });
            i = end;
            continue;
        }

        // Bold **
        if ch == b'*' && peek(bytes, i + 1) == Some(b'*') {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::BoldDelim,
                span: tracker.span(i, i + 2),
            });
            i += 2;
            continue;
        }

        // Strikethrough ~~
        if ch == b'~' && peek(bytes, i + 1) == Some(b'~') {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::StrikethroughDelim,
                span: tracker.span(i, i + 2),
            });
            i += 2;
            continue;
        }

        // Italic * (not **)
        if ch == b'*' && peek(bytes, i + 1) != Some(b'*') {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::ItalicDelim,
                span: tracker.span(i, i + 1),
            });
            i += 1;
            continue;
        }

        // Footnote ref [^label]
        if ch == b'['
            && peek(bytes, i + 1) == Some(b'^')
            && let Some((label, end)) = try_footnote_ref(text, i)
        {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::FootnoteRef { label },
                span: tracker.span(i, end),
            });
            i = end;
            continue;
        }

        // Citation [@key] or [@key, locator]
        if ch == b'['
            && peek(bytes, i + 1) == Some(b'@')
            && let Some((key, locator, end)) = try_cite(text, i)
        {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: Token::Cite { key, locator },
                span: tracker.span(i, end),
            });
            i = end;
            continue;
        }

        // Link [text](url ...)
        if ch == b'['
            && let Some((link_tok, end)) = try_link(text, i)
        {
            flush(&mut current_text, run_start, i, &mut tracker, out);
            out.push(Spanned {
                kind: link_tok,
                span: tracker.span(i, end),
            });
            i = end;
            continue;
        }

        // Tag
        if ch == b'#' {
            if let Some(next) = text[i + 1..].chars().next()
                && (next.is_alphanumeric() || next == '_')
            {
                flush(&mut current_text, run_start, i, &mut tracker, out);
                let tag = tokenize_tag(text, i + 1);
                out.push(Spanned {
                    kind: tag.token,
                    span: tracker.span(i, tag.name_end),
                });
                if let Some((arg_tok, arg_start, arg_end)) = tag.arg {
                    out.push(Spanned {
                        kind: arg_tok,
                        span: tracker.span(arg_start, arg_end),
                    });
                }
                i = tag.end;
                continue;
            }
            if current_text.is_empty() {
                run_start = i;
            }
            current_text.push('#');
            i += 1;
            continue;
        }

        // Plain character — push the whole (possibly multi-byte) char.
        if current_text.is_empty() {
            run_start = i;
        }
        let c = text[i..].chars().next().unwrap();
        current_text.push(c);
        i += c.len_utf8();
    }

    flush(&mut current_text, run_start, i, &mut tracker, out);
}

// ===========================================================================
// Line-level classification helpers
// ===========================================================================

fn try_heading(text: &str) -> Option<u8> {
    let trimmed = text.trim_start();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed[hashes..];
        if rest.is_empty() || rest.starts_with(' ') {
            return Some(hashes as u8);
        }
    }
    None
}

fn try_code_fence(trimmed: &str) -> Option<Token> {
    let fence_char = trimmed.chars().next()?;
    if fence_char != '`' && fence_char != '~' {
        return None;
    }
    let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
    if fence_len < 3 {
        return None;
    }
    let rest = trimmed[fence_len..].trim();
    if rest.is_empty() {
        Some(Token::FencedCodeClose {
            fence_char,
            fence_len,
        })
    } else {
        Some(Token::FencedCodeOpen {
            info: rest.to_string(),
            fence_char,
            fence_len,
        })
    }
}

fn try_html_line(trimmed: &str) -> Option<Token> {
    if !trimmed.starts_with('<') {
        return None;
    }
    let rest = &trimmed[1..];
    let closing = rest.starts_with('/');
    let tag_start = if closing { &rest[1..] } else { rest };
    let tag_name: String = tag_start
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if tag_name.is_empty() {
        return None;
    }
    if closing {
        Some(Token::HtmlClose { tag: tag_name })
    } else {
        Some(Token::HtmlOpen { tag: tag_name })
    }
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    if first != '-' && first != '*' && first != '_' {
        return false;
    }
    trimmed.chars().all(|c| c == first || c == ' ')
}

fn try_callout(trimmed: &str) -> Option<(String, Option<String>)> {
    let rest = trimmed.strip_prefix('>')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("[!")?;
    let end = rest.find(']')?;
    let kind = &rest[..end];
    if kind.is_empty() {
        return None;
    }
    let after_type = &rest[end + 1..];
    let metadata = if let Some(meta_rest) = after_type.trim_start().strip_prefix('[') {
        meta_rest.find(']').and_then(|meta_end| {
            let meta = meta_rest[..meta_end].trim();
            if meta.is_empty() {
                None
            } else {
                Some(meta.to_string())
            }
        })
    } else {
        None
    };
    Some((kind.to_lowercase(), metadata))
}

fn try_list_item(text: &str) -> Option<(usize, bool)> {
    let indent = text.len() - text.trim_start().len();
    let trimmed = text.trim_start();
    if (trimmed.starts_with("- ") || trimmed.starts_with("+ ")) && trimmed.len() > 2 {
        return Some((indent, false));
    }
    if indent > 0 && trimmed.starts_with("* ") && trimmed.len() > 2 {
        return Some((indent, false));
    }
    let digits_end = trimmed.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    if digits_end > 0 && digits_end < trimmed.len() {
        let after = &trimmed[digits_end..];
        if after.starts_with(". ") && after.len() > 2 {
            return Some((indent, true));
        }
    }
    None
}

fn try_footnote_def(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("[^")?;
    let end = rest.find("]:")?;
    let label = &rest[..end];
    if label.is_empty() || label.contains(' ') {
        return None;
    }
    Some(label.to_string())
}

// ===========================================================================
// Inline helpers
// ===========================================================================

fn peek(bytes: &[u8], i: usize) -> Option<u8> {
    bytes.get(i).copied()
}

fn scan_backtick_code(text: &str, start: usize) -> Option<(&str, usize)> {
    let after = start + 1;
    if after >= text.len() {
        return None;
    }
    let end = text[after..].find('`')?;
    let code = &text[after..after + end];
    if code.is_empty() {
        return None;
    }
    Some((code, after + end + 1))
}

fn try_footnote_ref(text: &str, start: usize) -> Option<(String, usize)> {
    let rest = &text[start..];
    if !rest.starts_with("[^") {
        return None;
    }
    let after = &rest[2..];
    let end = after.find(']')?;
    let label = &after[..end];
    if label.is_empty() || label.contains(' ') {
        return None;
    }
    Some((label.to_string(), start + 2 + end + 1))
}

/// Scan a Pandoc-style citation `[@key]` / `[@key, locator]` whose `[` sits
/// at byte `start`. Returns the key, the optional locator (trimmed, `None`
/// when empty), and the byte index just past the closing `]`.
///
/// Returns `None` — so the caller falls through to link parsing or plain
/// text — when the key is empty or malformed, the bracket never closes on
/// this inline run, unexpected content follows the key, or the citation is
/// immediately followed by `(` (link syntax `[text](url)` takes precedence).
fn try_cite(text: &str, start: usize) -> Option<(String, Option<String>, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'[') || bytes.get(start + 1) != Some(&b'@') {
        return None;
    }

    // Key: ASCII alphanumerics plus `_`, with `-` allowed after the first
    // char (trailing disambiguation suffixes like `-1`).
    let key_start = start + 2;
    let first = *bytes.get(key_start)?;
    if !first.is_ascii_alphanumeric() && first != b'_' {
        return None;
    }
    let mut pos = key_start + 1;
    while pos < bytes.len()
        && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_' || bytes[pos] == b'-')
    {
        pos += 1;
    }
    let key_end = pos;

    // Optional locator after a comma; `]` must close on this inline run.
    let locator = match bytes.get(pos)? {
        b']' => None,
        b',' => {
            let loc_start = pos + 1;
            let close = text[loc_start..].find(']')? + loc_start;
            pos = close;
            let loc = text[loc_start..close].trim();
            if loc.is_empty() {
                None
            } else {
                Some(loc.to_string())
            }
        }
        _ => return None,
    };

    let end = pos + 1; // past the `]`
    if peek(bytes, end) == Some(b'(') {
        return None; // `[@key](url)` is a link, not a citation
    }
    Some((text[key_start..key_end].to_string(), locator, end))
}

fn try_link(text: &str, start: usize) -> Option<(Token, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(start).copied() != Some(b'[') {
        return None;
    }

    let mut depth = 0i32;
    let mut pos = start;
    let bracket_close;
    loop {
        if pos >= bytes.len() {
            return None;
        }
        if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            pos += 2;
            continue;
        }
        if bytes[pos] == b'[' {
            depth += 1;
        } else if bytes[pos] == b']' {
            depth -= 1;
            if depth == 0 {
                bracket_close = pos;
                break;
            }
        }
        pos += 1;
    }

    let link_text = &text[start + 1..bracket_close];
    pos = bracket_close + 1;
    if pos >= bytes.len() || bytes[pos] != b'(' {
        return None;
    }

    let mut paren_depth = 0i32;
    let paren_close;
    loop {
        if pos >= bytes.len() {
            return None;
        }
        if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            pos += 2;
            continue;
        }
        if bytes[pos] == b'(' {
            paren_depth += 1;
        } else if bytes[pos] == b')' {
            paren_depth -= 1;
            if paren_depth == 0 {
                paren_close = pos;
                break;
            }
        }
        pos += 1;
    }

    let paren_inner = text[bracket_close + 2..paren_close].trim();
    let (url, title, meta) = parse_link_paren(paren_inner);

    Some((
        Token::Link {
            text: link_text.to_string(),
            url,
            title,
            meta,
        },
        paren_close + 1,
    ))
}

fn parse_link_paren(inner: &str) -> (String, Option<String>, Option<String>) {
    let inner = inner.trim();
    let url_end = inner.find([' ', '"', '[']).unwrap_or(inner.len());
    let url = inner[..url_end].to_string();
    let rest = inner[url_end..].trim();

    if rest.is_empty() {
        return (url, None, None);
    }

    let (title, rest) = if let Some(after_quote) = rest.strip_prefix('"') {
        if let Some(end) = after_quote.find('"') {
            (
                Some(after_quote[..end].to_string()),
                after_quote[end + 1..].trim(),
            )
        } else {
            (None, rest)
        }
    } else {
        (None, rest)
    };

    let meta = if rest.starts_with('[') {
        rest.find(']').map(|end| rest[1..end].to_string())
    } else {
        None
    };

    (url, title, meta)
}

/// Result of scanning a `#name [arg]` inline tag.
struct ScannedTag {
    /// The tag token itself (`Tag` or `UnknownTag`).
    token: Token,
    /// Byte index (in the scanned text) just past the tag name.
    name_end: usize,
    /// Optional argument token with the byte range of its trimmed raw text.
    arg: Option<(Token, usize, usize)>,
    /// Byte index just past everything consumed by this tag.
    end: usize,
}

/// Scan a tag starting at `name_start` (the byte after `#`; the `#` itself is
/// at `name_start - 1`).
fn tokenize_tag(text: &str, name_start: usize) -> ScannedTag {
    let bytes = text.as_bytes();
    let mut pos = name_start;

    for c in text[name_start..].chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            pos += c.len_utf8();
        } else {
            break;
        }
    }

    let name_end = pos;
    let name = &text[name_start..name_end];
    let tok = match Keyword::from_str(name) {
        Some(kw) => Token::Tag(kw),
        None => Token::UnknownTag {
            name: name.to_string(),
        },
    };

    let mut arg = String::new();
    let arg_scan_start = if pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
        let scan_start = pos;
        while pos < bytes.len() {
            let c = bytes[pos];
            if c == b'#'
                && let Some(next) = text[pos + 1..].chars().next()
                && (next.is_alphanumeric() || next == '_')
            {
                break;
            }
            if c == b'\\' {
                if peek(bytes, pos + 1) == Some(b'#') {
                    arg.push('#');
                    pos += 2;
                    continue;
                }
                arg.push('\\');
                pos += 1;
                continue;
            }
            let ch = text[pos..].chars().next().unwrap();
            arg.push(ch);
            pos += ch.len_utf8();
        }
        Some(scan_start)
    } else {
        None
    };

    let arg_tok = {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            None
        } else {
            // Recover the raw byte range of the trimmed argument: escape
            // sequences only shift interior bytes, so trimming whitespace on
            // the raw text matches trimming on the built string.
            let scan_start = arg_scan_start.unwrap_or(pos);
            let raw = &text[scan_start..pos];
            let arg_start = scan_start + (raw.len() - raw.trim_start().len());
            let arg_end = scan_start + raw.trim_end().len();
            Some((Token::TagArg(trimmed.to_string()), arg_start, arg_end))
        }
    };

    ScannedTag {
        token: tok,
        name_end,
        arg: arg_tok,
        end: pos,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn block_tokens(source: &str) -> Vec<Token> {
        Lexer::new(source)
            .tokens
            .iter()
            .map(|s| s.kind.clone())
            .filter(|t| !matches!(t, Token::Newline | Token::Eof))
            .collect()
    }

    fn inline_tokens(text: &str) -> Vec<Token> {
        tokenize_inline(text, Span::empty(1, 1))
            .into_iter()
            .map(|s| s.kind)
            .collect()
    }

    // Block-level tests

    #[test]
    fn test_heading() {
        let tokens = block_tokens("# Hello world");
        assert!(matches!(&tokens[0], Token::Heading { level: 1 }));
        assert!(matches!(&tokens[1], Token::RawLine(_)));
    }

    #[test]
    fn test_code_fence() {
        let tokens = block_tokens("```rust #tangle file=main.rs");
        assert!(
            matches!(&tokens[0], Token::FencedCodeOpen { info, .. } if info == "rust #tangle file=main.rs")
        );
    }

    #[test]
    fn test_block_tag() {
        let tokens = block_tokens("#deadline 2026-04-10");
        assert!(matches!(&tokens[0], Token::Tag(Keyword::Deadline)));
        assert!(matches!(&tokens[1], Token::TagArg(a) if a == "2026-04-10"));
    }

    #[test]
    fn test_list_item() {
        let tokens = block_tokens("- [ ] Task item");
        assert!(matches!(
            &tokens[0],
            Token::ListMarker {
                ordered: false,
                indent: 0
            }
        ));
        assert!(matches!(&tokens[1], Token::RawLine(_)));
    }

    #[test]
    fn test_properties() {
        let tokens = block_tokens("#properties");
        assert!(matches!(&tokens[0], Token::PropertiesOpen));
    }

    #[test]
    fn test_comment() {
        let tokens = block_tokens("// this is a comment");
        assert!(matches!(&tokens[0], Token::LineComment));
        assert!(matches!(&tokens[1], Token::RawLine(_)));
    }

    #[test]
    fn test_horizontal_rule() {
        let tokens = block_tokens("***");
        assert!(matches!(&tokens[0], Token::HorizontalRule));
    }

    #[test]
    fn test_frontmatter() {
        let tokens = block_tokens("---");
        assert!(matches!(&tokens[0], Token::FrontmatterDelim));
    }

    #[test]
    fn test_blank_line() {
        let tokens = block_tokens("");
        assert!(matches!(&tokens[0], Token::BlankLine));
    }

    #[test]
    fn test_unknown_tag() {
        let tokens = block_tokens("#custom value");
        assert!(matches!(&tokens[0], Token::UnknownTag { name } if name == "custom"));
        assert!(matches!(&tokens[1], Token::TagArg(a) if a == "value"));
    }

    // Inline tokenizer tests

    #[test]
    fn test_inline_tag() {
        let tokens = inline_tokens("some text #todo fix this");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "some text "));
        assert!(matches!(&tokens[1], Token::Tag(Keyword::Todo)));
        assert!(matches!(&tokens[2], Token::TagArg(a) if a == "fix this"));
    }

    #[test]
    fn test_inline_tag_multibyte_name() {
        // Tag names may contain non-ASCII alphanumerics; the scanner must
        // advance whole chars, never landing inside a multi-byte sequence.
        let tokens = inline_tokens("note #café fix accents");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "note "));
        assert!(matches!(&tokens[1], Token::UnknownTag { name } if name == "café"));
        assert!(matches!(&tokens[2], Token::TagArg(a) if a == "fix accents"));

        let tokens = inline_tokens("#日本語タグ 引数はこちら");
        assert!(matches!(&tokens[0], Token::UnknownTag { name } if name == "日本語タグ"));
        assert!(matches!(&tokens[1], Token::TagArg(a) if a == "引数はこちら"));
    }

    #[test]
    fn test_inline_tag_multibyte_spans() {
        // Spans must slice the source exactly even around multi-byte chars.
        let src = "αβ #todo fíx это";
        let spanned = tokenize_inline(src, Span::new(0, src.len(), 1, 1));
        for s in &spanned {
            match &s.kind {
                Token::Tag(_) => assert_eq!(&src[s.span.start..s.span.end], "#todo"),
                Token::TagArg(a) => assert_eq!(&src[s.span.start..s.span.end], a.as_str()),
                Token::Text(t) => assert_eq!(&src[s.span.start..s.span.end], t.as_str()),
                _ => {}
            }
        }
    }

    #[test]
    fn test_inline_tag_arg_stops_at_next_multibyte_tag() {
        // A '#' followed by a multi-byte alphanumeric starts a new tag; the
        // old byte-cast check misread the first UTF-8 byte here.
        let tokens = inline_tokens("#todo done #über arg");
        assert!(matches!(&tokens[0], Token::Tag(Keyword::Todo)));
        assert!(matches!(&tokens[1], Token::TagArg(a) if a == "done"));
        assert!(matches!(&tokens[2], Token::UnknownTag { name } if name == "über"));
        assert!(matches!(&tokens[3], Token::TagArg(a) if a == "arg"));
    }

    #[test]
    fn test_inline_bold_italic() {
        let tokens = inline_tokens("**bold** and *italic*");
        assert!(matches!(&tokens[0], Token::BoldDelim));
        assert!(matches!(&tokens[1], Token::Text(t) if t == "bold"));
        assert!(matches!(&tokens[2], Token::BoldDelim));
        assert!(matches!(&tokens[3], Token::Text(t) if t == " and "));
        assert!(matches!(&tokens[4], Token::ItalicDelim));
        assert!(matches!(&tokens[5], Token::Text(t) if t == "italic"));
        assert!(matches!(&tokens[6], Token::ItalicDelim));
    }

    #[test]
    fn test_inline_code() {
        let tokens = inline_tokens("use `println!` here");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "use "));
        assert!(matches!(&tokens[1], Token::InlineCode(c) if c == "println!"));
        assert!(matches!(&tokens[2], Token::Text(t) if t == " here"));
    }

    #[test]
    fn test_inline_link() {
        let tokens = inline_tokens("[click](https://example.com)");
        assert!(
            matches!(&tokens[0], Token::Link { text, url, .. } if text == "click" && url == "https://example.com")
        );
    }

    #[test]
    fn test_inline_footnote_ref() {
        let tokens = inline_tokens("text[^1] more");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "text"));
        assert!(matches!(&tokens[1], Token::FootnoteRef { label } if label == "1"));
        assert!(matches!(&tokens[2], Token::Text(t) if t == " more"));
    }

    #[test]
    fn test_inline_cite_simple() {
        let tokens = inline_tokens("see [@paszke_pytorch_2019] for details");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "see "));
        assert!(matches!(
            &tokens[1],
            Token::Cite { key, locator: None } if key == "paszke_pytorch_2019"
        ));
        assert!(matches!(&tokens[2], Token::Text(t) if t == " for details"));
    }

    #[test]
    fn test_inline_cite_with_locator() {
        let tokens = inline_tokens("[@martin_adapting_2021-1, p. 4]");
        assert!(matches!(
            &tokens[0],
            Token::Cite { key, locator: Some(loc) }
                if key == "martin_adapting_2021-1" && loc == "p. 4"
        ));
    }

    #[test]
    fn test_inline_cite_empty_locator_is_none() {
        let tokens = inline_tokens("[@key, ]");
        assert!(matches!(
            &tokens[0],
            Token::Cite { key, locator: None } if key == "key"
        ));
    }

    #[test]
    fn test_inline_cite_fallback_to_text() {
        // Empty key
        let tokens = inline_tokens("[@] nothing");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "[@] nothing"));

        // Unclosed bracket
        let tokens = inline_tokens("see [@dangling_2020");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "see [@dangling_2020"));

        // Content after the key that is not `,` or `]`
        let tokens = inline_tokens("[@key extra]");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "[@key extra]"));

        // Unclosed after locator comma
        let tokens = inline_tokens("[@key, p. 4");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "[@key, p. 4"));
    }

    #[test]
    fn test_inline_cite_not_confused_with_link_or_footnote() {
        // `[@key](url)` is a link whose text happens to start with @
        let tokens = inline_tokens("[@key](https://example.com)");
        assert!(matches!(&tokens[0], Token::Link { text, .. } if text == "@key"));

        // Footnotes keep working
        let tokens = inline_tokens("x[^1] and [@real_key_2024]");
        assert!(matches!(&tokens[1], Token::FootnoteRef { label } if label == "1"));
        assert!(matches!(&tokens[3], Token::Cite { key, .. } if key == "real_key_2024"));
    }

    #[test]
    fn test_inline_cite_spans_slice_source_multibyte() {
        // Multi-byte chars before the cite and inside the locator; slicing
        // the source with the token span must yield the exact citation text.
        let src = "αβ [@kohler_2019, p. 4–5] fin";
        let spanned = tokenize_inline(src, Span::new(0, src.len(), 1, 1));
        let cite = spanned
            .iter()
            .find(|s| matches!(s.kind, Token::Cite { .. }))
            .expect("should lex a cite");
        assert_eq!(
            &src[cite.span.start..cite.span.end],
            "[@kohler_2019, p. 4–5]"
        );
        assert!(matches!(
            &cite.kind,
            Token::Cite { key, locator: Some(loc) } if key == "kohler_2019" && loc == "p. 4–5"
        ));
    }

    #[test]
    fn test_inline_cite_non_ascii_key_falls_back() {
        // 'ö' is outside the key charset (ASCII alphanumerics, `_`, `-`),
        // so this is not a citation; the whole run stays plain text.
        let src = "[@köhler_2019] text";
        let tokens = inline_tokens(src);
        assert!(matches!(&tokens[0], Token::Text(t) if t == src));
    }

    #[test]
    fn test_inline_escaped_hash() {
        let tokens = inline_tokens(r"price \#100");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "price #100"));
    }
}

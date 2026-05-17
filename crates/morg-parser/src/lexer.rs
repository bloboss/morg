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
pub fn tokenize_inline(text: &str, span: Span) -> Vec<Spanned> {
    let mut out = Vec::new();
    tokenize_inline_into(text, span, &mut out);
    out
}

fn tokenize_inline_into(text: &str, span: Span, out: &mut Vec<Spanned>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut current_text = String::new();

    while i < bytes.len() {
        let ch = bytes[i];

        // Backslash escape
        if ch == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'#' || next == b'[' || next == b'*' || next == b'~' || next == b'`' {
                current_text.push(next as char);
                i += 2;
                continue;
            }
            current_text.push('\\');
            i += 1;
            continue;
        }

        // Inline code
        if ch == b'`'
            && let Some((code, end)) = scan_backtick_code(text, i)
        {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: Token::InlineCode(code.to_string()),
                span,
            });
            i = end;
            continue;
        }

        // Bold **
        if ch == b'*' && peek(bytes, i + 1) == Some(b'*') {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: Token::BoldDelim,
                span,
            });
            i += 2;
            continue;
        }

        // Strikethrough ~~
        if ch == b'~' && peek(bytes, i + 1) == Some(b'~') {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: Token::StrikethroughDelim,
                span,
            });
            i += 2;
            continue;
        }

        // Italic * (not **)
        if ch == b'*' && peek(bytes, i + 1) != Some(b'*') {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: Token::ItalicDelim,
                span,
            });
            i += 1;
            continue;
        }

        // Footnote ref [^label]
        if ch == b'['
            && peek(bytes, i + 1) == Some(b'^')
            && let Some((label, end)) = try_footnote_ref(text, i)
        {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: Token::FootnoteRef { label },
                span,
            });
            i = end;
            continue;
        }

        // Link [text](url ...)
        if ch == b'['
            && let Some((link_tok, end)) = try_link(text, i)
        {
            flush_text_token(&mut current_text, span, out);
            out.push(Spanned {
                kind: link_tok,
                span,
            });
            i = end;
            continue;
        }

        // Tag
        if ch == b'#' {
            if let Some(next) = peek(bytes, i + 1)
                && ((next as char).is_alphanumeric() || next == b'_')
            {
                flush_text_token(&mut current_text, span, out);
                let (tok, arg_tok, end) = tokenize_tag(text, i + 1, span);
                out.push(Spanned { kind: tok, span });
                if let Some(at) = arg_tok {
                    out.push(Spanned { kind: at, span });
                }
                i = end;
                continue;
            }
            current_text.push('#');
            i += 1;
            continue;
        }

        current_text.push(ch as char);
        i += 1;
    }

    flush_text_token(&mut current_text, span, out);
}

fn flush_text_token(buf: &mut String, span: Span, out: &mut Vec<Spanned>) {
    if !buf.is_empty() {
        out.push(Spanned {
            kind: Token::Text(std::mem::take(buf)),
            span,
        });
    }
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

fn tokenize_tag(text: &str, name_start: usize, _span: Span) -> (Token, Option<Token>, usize) {
    let bytes = text.as_bytes();
    let mut pos = name_start;

    while pos < bytes.len() {
        let c = bytes[pos] as char;
        if c.is_alphanumeric() || c == '-' || c == '_' {
            pos += 1;
        } else {
            break;
        }
    }

    let name = &text[name_start..pos];
    let tok = match Keyword::from_str(name) {
        Some(kw) => Token::Tag(kw),
        None => Token::UnknownTag {
            name: name.to_string(),
        },
    };

    let mut arg = String::new();

    // Capture parenthetical immediately after tag name: #meal(chili) or #meal(chili) 2026-05-20
    if pos < bytes.len() && bytes[pos] == b'(' {
        if let Some(close_offset) = text[pos..].find(')') {
            let close_pos = pos + close_offset + 1;
            arg.push_str(&text[pos..close_pos]);
            pos = close_pos;
        }
    }

    if pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
        let space_start = pos;
        while pos < bytes.len() {
            let c = bytes[pos];
            if c == b'#'
                && let Some(next) = peek(bytes, pos + 1)
                && ((next as char).is_alphanumeric() || next == b'_')
            {
                break;
            }
            if c == b'\\' {
                if peek(bytes, pos + 1) == Some(b'#') {
                    pos += 2;
                    continue;
                }
                pos += 1;
                continue;
            }
            pos += 1;
        }
        let space_arg = text[space_start..pos].trim();
        if !space_arg.is_empty() {
            if !arg.is_empty() {
                arg.push(' ');
            }
            arg.push_str(space_arg);
        }
    }

    let arg_tok = {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Token::TagArg(trimmed.to_string()))
        }
    };

    (tok, arg_tok, pos)
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
    fn test_inline_escaped_hash() {
        let tokens = inline_tokens(r"price \#100");
        assert!(matches!(&tokens[0], Token::Text(t) if t == "price #100"));
    }
}

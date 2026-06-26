//! Token definitions for the morg-mode parser.
//!
//! Keywords are defined via `define_keywords!` — adding a new keyword
//! requires a single line here. The lexer and parser derive all their
//! keyword handling from this definition.

use crate::span::Span;

// ---------------------------------------------------------------------------
// Keyword definitions — single source of truth
// ---------------------------------------------------------------------------

macro_rules! define_keywords {
    ($($name:ident => $string:literal),* $(,)?) => {
        /// Known morg tag keywords.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Keyword {
            $($name,)*
        }

        impl Keyword {
            /// Look up a keyword by its string form. Returns `None` for unknown tags.
            #[allow(clippy::should_implement_trait)]
            pub fn from_str(s: &str) -> Option<Self> {
                match s {
                    $($string => Some(Self::$name),)*
                    _ => None,
                }
            }

            /// The canonical string form of this keyword.
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$name => $string,)*
                }
            }

            /// All defined keywords.
            pub fn all() -> &'static [Keyword] {
                &[$(Self::$name,)*]
            }
        }

        impl std::fmt::Display for Keyword {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

define_keywords! {
    // TODO system
    Todo        => "todo",
    Done        => "done",

    // Planning / timestamps
    Deadline    => "deadline",
    Scheduled   => "scheduled",
    Date        => "date",
    Event       => "event",

    // Time tracking
    ClockIn     => "clock-in",
    ClockOut    => "clock-out",
    Clock       => "clock",

    // Code / tangling
    Tangle      => "tangle",

    // Metadata
    Priority    => "priority",
    Effort      => "effort",
    Closed      => "closed",
    Archive     => "archive",
    Progress    => "progress",
    Purchase    => "purchase",

    // Media tracking
    Media       => "media",

    // Structure
    Properties  => "properties",
    End         => "end",
}

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// A token with source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub kind: Token,
    pub span: Span,
}

/// The kinds of tokens the lexer produces.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ---- Block structure ----
    /// `# ` through `###### ` — level 1..=6
    Heading { level: u8 },
    /// Opening ``` or ~~~ with optional info string
    FencedCodeOpen {
        info: String,
        fence_char: char,
        fence_len: usize,
    },
    /// Closing ``` or ~~~
    FencedCodeClose { fence_char: char, fence_len: usize },
    /// `---` frontmatter delimiter
    FrontmatterDelim,
    /// `***`, `___`, `----` etc.
    HorizontalRule,
    /// List item marker: `- `, `+ `, `* ` (indented), `N. `
    ListMarker { ordered: bool, indent: usize },
    /// A line that starts with `|`
    TableRow,
    /// `> [!type]` with optional `[metadata]`
    CalloutStart {
        kind: String,
        metadata: Option<String>,
    },
    /// `> ` continuation of a blockquote
    BlockquoteContinuation,
    /// `<tag ...>` opening HTML tag
    HtmlOpen { tag: String },
    /// `</tag>` closing HTML tag
    HtmlClose { tag: String },
    /// `#properties`
    PropertiesOpen,
    /// `#end`
    PropertiesClose,
    /// `[^label]: ` footnote definition start
    FootnoteDefStart { label: String },
    /// `//` single-line comment
    LineComment,
    /// `/*` block comment open
    BlockCommentOpen,
    /// `*/` block comment close
    BlockCommentClose,

    // ---- Tags ----
    /// A known `#keyword` tag
    Tag(Keyword),
    /// An unknown `#name` tag
    UnknownTag { name: String },

    // ---- Inline content ----
    /// Plain text (no markup)
    Text(String),
    /// `**` bold delimiter (open or close)
    BoldDelim,
    /// `*` italic delimiter (open or close, when not part of `**`)
    ItalicDelim,
    /// `~~` strikethrough delimiter
    StrikethroughDelim,
    /// `` ` `` backtick-delimited code span content
    InlineCode(String),
    /// `[text](url)` or `[text](url "title" [metadata])`
    Link {
        text: String,
        url: String,
        title: Option<String>,
        meta: Option<String>,
    },
    /// `[^label]` footnote reference
    FootnoteRef { label: String },
    /// `key=value` attribute
    Attribute { key: String, value: String },
    /// Tag argument text (after a tag, before the next tag)
    TagArg(String),

    // ---- Raw content ----
    /// A line of raw text inside a code block or HTML block
    RawLine(String),
    /// `key = value` inside a property drawer
    PropertyLine { key: String, value: String },

    // ---- Control ----
    /// End of a line
    Newline,
    /// Empty / whitespace-only line
    BlankLine,
    /// End of input
    Eof,
}

impl Token {
    /// Whether this token starts a new block-level construct.
    pub fn is_block_start(&self) -> bool {
        matches!(
            self,
            Token::Heading { .. }
                | Token::FencedCodeOpen { .. }
                | Token::FrontmatterDelim
                | Token::HorizontalRule
                | Token::ListMarker { .. }
                | Token::TableRow
                | Token::CalloutStart { .. }
                | Token::HtmlOpen { .. }
                | Token::PropertiesOpen
                | Token::FootnoteDefStart { .. }
                | Token::LineComment
                | Token::BlockCommentOpen
                | Token::BlankLine
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_roundtrip() {
        for kw in Keyword::all() {
            let s = kw.as_str();
            assert_eq!(Keyword::from_str(s), Some(*kw), "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_unknown_keyword() {
        assert_eq!(Keyword::from_str("nonexistent"), None);
    }

    #[test]
    fn test_all_keywords_non_empty() {
        assert!(!Keyword::all().is_empty());
        for kw in Keyword::all() {
            assert!(!kw.as_str().is_empty());
        }
    }

    #[test]
    fn test_keyword_display() {
        assert_eq!(format!("{}", Keyword::Todo), "todo");
        assert_eq!(format!("{}", Keyword::ClockIn), "clock-in");
        assert_eq!(format!("{}", Keyword::Properties), "properties");
    }
}

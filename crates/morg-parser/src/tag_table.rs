//! User-defined tag interpretation and argument extent (plan §10, T1 + T2).
//!
//! A [`TagTable`] holds compiled user tag declarations, typically parsed from
//! a `[tags]` config section by a consumer crate. Interpretation (T1) changes
//! nothing about lexing — custom tags still lex as `UnknownTag` plus the
//! greedy argument rule — it only upgrades those tags to [`TagKind::Custom`]
//! at parse level via [`parse_document_with`]. A declared [`ArgShape`] (T2)
//! additionally changes where an *inline* argument ends, which is why the
//! inline lexer consults the table too
//! ([`tokenize_inline_with`](crate::lexer::tokenize_inline_with)).
//!
//! Declarations arrive as plain Rust data ([`TagDeclaration`]) so that config
//! layers (TOML, YAML, …) stay out of this crate. Each declaration carries at
//! most one interpretation rule, plus an optional extent shape:
//!
//! - `pattern`: a regex (the `regex` crate — deliberately: no lookaround, no
//!   catastrophic backtracking) with named capture groups; compiled once when
//!   the table is built.
//! - `kind`: a shorthand reusing a built-in argument parser
//!   (`duration` | `date` | `timestamp` | `slug`).
//! - `shape`: where the argument *ends* at inline positions — a closed
//!   vocabulary ([`ArgShape`]), never a regex at the lexer. A shape may stand
//!   alone (a plain declaration) or compose with `pattern`/`kind` (the shape
//!   bounds the extent; the rule then interprets the captured text).
//!
//! [`TagKind::Custom`]: crate::tags::TagKind::Custom
//! [`parse_document_with`]: crate::parser::parse_document_with

use std::collections::HashMap;

use regex::Regex;

use crate::tokens::Keyword;

/// A single user tag declaration, as plain data (one `[tags.<name>]` entry).
///
/// At most one of `pattern` and `kind` may be set; at least one of `pattern`,
/// `kind`, and `shape` must be. [`TagTable::build`] rejects everything else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagDeclaration {
    /// The tag name (without the leading `#`).
    pub name: String,
    /// Regex with named capture groups, matched against the raw argument.
    pub pattern: Option<String>,
    /// Shorthand reusing a built-in argument parser.
    pub kind: Option<CustomArgKind>,
    /// Argument extent shape (T2). `None` means [`ArgShape::Greedy`].
    pub shape: Option<ArgShape>,
}

/// The closed argument-extent vocabulary (plan §10.1(3)). Shapes change where
/// an **inline** tag's argument ends; block-level tags always keep the
/// whole-line extent (there the shape only structures/validates the
/// argument). A shape that fails to match falls back to the greedy rule and
/// flags `shape_mismatch` — never an error, never a different document shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ArgShape {
    /// The universal default: to the next `#` that starts a tag, or EOL.
    #[default]
    Greedy,
    /// A `"..."` string right after the name; `\"` escapes the quote. The
    /// tag ends at the closing quote and the rest of the line is prose.
    Quoted,
    /// One whitespace-delimited word.
    Word,
    /// A run of `key=value` pairs (values optionally quoted), ending before
    /// the first token that is not one.
    Kv,
    /// Up to (excluding) the first of `.,;:!?` or EOL.
    UntilPunct,
}

impl ArgShape {
    /// Look up a shape by its config string. Returns `None` for unknown
    /// words.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "greedy" => Some(Self::Greedy),
            "quoted" => Some(Self::Quoted),
            "word" => Some(Self::Word),
            "kv" => Some(Self::Kv),
            "until-punct" => Some(Self::UntilPunct),
            _ => None,
        }
    }

    /// The canonical config string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::Quoted => "quoted",
            Self::Word => "word",
            Self::Kv => "kv",
            Self::UntilPunct => "until-punct",
        }
    }
}

impl std::fmt::Display for ArgShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `kind` shorthands: each reuses an existing built-in argument parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CustomArgKind {
    /// `1h30m`-style durations — the `#effort`/`#clock` parser.
    Duration,
    /// A calendar date — the `#deadline`/`#scheduled` timestamp parser,
    /// canonicalized to the date component.
    Date,
    /// A date or date+time — the `#deadline`/`#scheduled` timestamp parser,
    /// keeping the time when present.
    Timestamp,
    /// A slug-shaped name — the `#anchor` parser.
    Slug,
}

impl CustomArgKind {
    /// Look up a shorthand by its config string. Returns `None` for unknown
    /// words.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "duration" => Some(Self::Duration),
            "date" => Some(Self::Date),
            "timestamp" => Some(Self::Timestamp),
            "slug" => Some(Self::Slug),
            _ => None,
        }
    }

    /// The canonical config string, also used as the single field's group
    /// name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Duration => "duration",
            Self::Date => "date",
            Self::Timestamp => "timestamp",
            Self::Slug => "slug",
        }
    }
}

impl std::fmt::Display for CustomArgKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a [`TagTable`] could not be built. Every variant names the offending
/// tag.
#[derive(Debug, Clone, PartialEq)]
pub enum TagTableError {
    /// The declared name cannot lex as a tag name (ASCII or Unicode
    /// alphanumerics, `-`, `_`; must not be empty or start with `-`).
    InvalidName { tag: String },
    /// The name collides with a built-in keyword (`#deadline`, `#todo`, …).
    BuiltinCollision { tag: String },
    /// The same name was declared twice.
    DuplicateDeclaration { tag: String },
    /// Both `pattern` and `kind` were given — they are mutually exclusive.
    ConflictingRule { tag: String },
    /// None of `pattern`, `kind`, and `shape` was given.
    MissingRule { tag: String },
    /// The `pattern` regex failed to compile.
    InvalidPattern {
        tag: String,
        error: Box<regex::Error>,
    },
}

impl std::fmt::Display for TagTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { tag } => {
                write!(f, "tag '{tag}': name cannot appear as a #tag")
            }
            Self::BuiltinCollision { tag } => {
                write!(f, "tag '{tag}': collides with the built-in #{tag} keyword")
            }
            Self::DuplicateDeclaration { tag } => {
                write!(f, "tag '{tag}': declared more than once")
            }
            Self::ConflictingRule { tag } => {
                write!(
                    f,
                    "tag '{tag}': 'pattern' and 'kind' are mutually exclusive"
                )
            }
            Self::MissingRule { tag } => {
                write!(f, "tag '{tag}': declare 'pattern', 'kind', or 'shape'")
            }
            Self::InvalidPattern { tag, error } => {
                write!(f, "tag '{tag}': invalid pattern: {error}")
            }
        }
    }
}

impl std::error::Error for TagTableError {}

/// The compiled interpretation rule for one custom tag.
#[derive(Debug, Clone)]
pub enum CustomRule {
    /// Regex with named capture groups, compiled at table build.
    Pattern(Regex),
    /// Built-in argument parser shorthand.
    Kind(CustomArgKind),
}

/// One compiled declaration: the argument-extent shape plus the optional
/// interpretation rule.
#[derive(Debug, Clone)]
struct CompiledTag {
    shape: ArgShape,
    rule: Option<CustomRule>,
}

/// Compiled user tag declarations, consulted when the parser types a tag
/// whose name is not a built-in keyword.
///
/// Build one with [`TagTable::build`] and pass it to
/// [`parse_document_with`](crate::parser::parse_document_with). The empty
/// table ([`TagTable::empty`] / [`Default`]) reproduces
/// [`parse_document`](crate::parser::parse_document) exactly.
#[derive(Debug, Clone, Default)]
pub struct TagTable {
    rules: HashMap<String, CompiledTag>,
}

impl TagTable {
    /// A table with no declarations. Parsing with it is identical to parsing
    /// without one.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Compile declarations into a table. Regexes are compiled once here;
    /// any invalid declaration aborts the build with the tag's name.
    pub fn build(
        declarations: impl IntoIterator<Item = TagDeclaration>,
    ) -> Result<Self, TagTableError> {
        let mut rules = HashMap::new();

        for decl in declarations {
            let tag = decl.name;
            if !is_valid_tag_name(&tag) {
                return Err(TagTableError::InvalidName { tag });
            }
            if Keyword::from_str(&tag).is_some() {
                return Err(TagTableError::BuiltinCollision { tag });
            }
            let rule = match (decl.pattern, decl.kind) {
                (Some(_), Some(_)) => return Err(TagTableError::ConflictingRule { tag }),
                // A shape alone is a valid (plain) declaration.
                (None, None) if decl.shape.is_none() => {
                    return Err(TagTableError::MissingRule { tag });
                }
                (None, None) => None,
                (Some(pattern), None) => match Regex::new(&pattern) {
                    Ok(re) => Some(CustomRule::Pattern(re)),
                    Err(error) => {
                        return Err(TagTableError::InvalidPattern {
                            tag,
                            error: Box::new(error),
                        });
                    }
                },
                (None, Some(kind)) => Some(CustomRule::Kind(kind)),
            };
            let compiled = CompiledTag {
                shape: decl.shape.unwrap_or_default(),
                rule,
            };
            if rules.insert(tag.clone(), compiled).is_some() {
                return Err(TagTableError::DuplicateDeclaration { tag });
            }
        }

        Ok(Self { rules })
    }

    /// Whether the table has no declarations.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Number of declared tags.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// The compiled interpretation rule for `name`. `None` both when `name`
    /// is not declared and when the declaration is shape-only; distinguish
    /// with [`TagTable::contains`].
    pub fn get(&self, name: &str) -> Option<&CustomRule> {
        self.rules.get(name)?.rule.as_ref()
    }

    /// The declared argument-extent shape for `name`.
    /// [`ArgShape::Greedy`] for undeclared names — the universal default.
    pub fn shape(&self, name: &str) -> ArgShape {
        self.rules
            .get(name)
            .map(|c| c.shape)
            .unwrap_or(ArgShape::Greedy)
    }

    /// Whether `name` is declared.
    pub fn contains(&self, name: &str) -> bool {
        self.rules.contains_key(name)
    }

    /// Declared tag names, in arbitrary order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rules.keys().map(String::as_str)
    }

    /// The field (column) names `name` can produce: the regex's named capture
    /// groups in pattern order, or the single kind name (empty for a
    /// shape-only declaration). `None` when `name` is not declared.
    pub fn group_names(&self, name: &str) -> Option<Vec<String>> {
        match &self.rules.get(name)?.rule {
            Some(CustomRule::Pattern(re)) => {
                Some(re.capture_names().flatten().map(str::to_string).collect())
            }
            Some(CustomRule::Kind(kind)) => Some(vec![kind.as_str().to_string()]),
            None => Some(Vec::new()),
        }
    }
}

/// Whether `name` can lex as a tag name: non-empty, every char alphanumeric
/// (Unicode, matching the lexer) or `-`/`_`, and the first char alphanumeric
/// or `_`.
fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphanumeric() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_decl(name: &str, pattern: &str) -> TagDeclaration {
        TagDeclaration {
            name: name.to_string(),
            pattern: Some(pattern.to_string()),
            ..Default::default()
        }
    }

    fn kind_decl(name: &str, kind: CustomArgKind) -> TagDeclaration {
        TagDeclaration {
            name: name.to_string(),
            kind: Some(kind),
            ..Default::default()
        }
    }

    #[test]
    fn test_build_ok() {
        let table = TagTable::build([
            pattern_decl("book", r#""(?<title>[^"]+)"\s+by\s+(?<author>.+)"#),
            kind_decl("reading-time", CustomArgKind::Duration),
        ])
        .unwrap();
        assert_eq!(table.len(), 2);
        assert!(table.contains("book"));
        assert!(table.contains("reading-time"));
        assert!(!table.contains("deadline"));
        assert_eq!(
            table.group_names("book").unwrap(),
            vec!["title".to_string(), "author".to_string()]
        );
        assert_eq!(
            table.group_names("reading-time").unwrap(),
            vec!["duration".to_string()]
        );
        assert_eq!(table.group_names("nope"), None);
    }

    #[test]
    fn test_empty_table() {
        let table = TagTable::empty();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
        assert_eq!(table.get("anything").map(|_| ()), None);
    }

    #[test]
    fn test_build_bad_regex() {
        let err = TagTable::build([pattern_decl("book", "(?<title>[")]).unwrap_err();
        assert!(matches!(err, TagTableError::InvalidPattern { ref tag, .. } if tag == "book"));
        // The error message carries the tag name.
        assert!(err.to_string().contains("book"));
    }

    #[test]
    fn test_build_builtin_collision() {
        for builtin in ["deadline", "todo", "clock-in", "anchor"] {
            let err = TagTable::build([pattern_decl(builtin, ".*")]).unwrap_err();
            assert!(
                matches!(err, TagTableError::BuiltinCollision { ref tag } if tag == builtin),
                "expected collision for {builtin}, got {err:?}"
            );
        }
    }

    #[test]
    fn test_build_pattern_kind_conflict() {
        let err = TagTable::build([TagDeclaration {
            name: "book".to_string(),
            pattern: Some(".*".to_string()),
            kind: Some(CustomArgKind::Slug),
            ..Default::default()
        }])
        .unwrap_err();
        assert!(matches!(err, TagTableError::ConflictingRule { ref tag } if tag == "book"));
    }

    #[test]
    fn test_build_missing_rule() {
        let err = TagTable::build([TagDeclaration {
            name: "book".to_string(),
            ..Default::default()
        }])
        .unwrap_err();
        assert!(matches!(err, TagTableError::MissingRule { ref tag } if tag == "book"));
    }

    #[test]
    fn test_build_shape_only_declaration() {
        // A shape with no interpretation rule is a valid plain declaration.
        let table = TagTable::build([TagDeclaration {
            name: "task".to_string(),
            shape: Some(ArgShape::Quoted),
            ..Default::default()
        }])
        .unwrap();
        assert!(table.contains("task"));
        assert_eq!(table.shape("task"), ArgShape::Quoted);
        assert!(table.get("task").is_none(), "no interpretation rule");
        assert_eq!(table.group_names("task"), Some(Vec::new()));
    }

    #[test]
    fn test_shape_composes_with_rule_and_defaults_to_greedy() {
        let table = TagTable::build([
            TagDeclaration {
                name: "book".to_string(),
                pattern: Some(r"(?<title>.+)".to_string()),
                shape: Some(ArgShape::Quoted),
                ..Default::default()
            },
            kind_decl("reading-time", CustomArgKind::Duration),
        ])
        .unwrap();
        assert_eq!(table.shape("book"), ArgShape::Quoted);
        assert!(matches!(table.get("book"), Some(CustomRule::Pattern(_))));
        // No shape declared -> greedy; undeclared names -> greedy too.
        assert_eq!(table.shape("reading-time"), ArgShape::Greedy);
        assert_eq!(table.shape("nope"), ArgShape::Greedy);
    }

    #[test]
    fn test_arg_shape_roundtrip() {
        for shape in [
            ArgShape::Greedy,
            ArgShape::Quoted,
            ArgShape::Word,
            ArgShape::Kv,
            ArgShape::UntilPunct,
        ] {
            assert_eq!(ArgShape::from_str(shape.as_str()), Some(shape));
        }
        assert_eq!(ArgShape::from_str("regex"), None);
        assert_eq!(ArgShape::default(), ArgShape::Greedy);
    }

    #[test]
    fn test_build_duplicate() {
        let err = TagTable::build([
            kind_decl("book", CustomArgKind::Slug),
            pattern_decl("book", ".*"),
        ])
        .unwrap_err();
        assert!(matches!(err, TagTableError::DuplicateDeclaration { ref tag } if tag == "book"));
    }

    #[test]
    fn test_build_invalid_name() {
        for name in ["", "has space", "-leading", "#book"] {
            let err = TagTable::build([pattern_decl(name, ".*")]).unwrap_err();
            assert!(
                matches!(err, TagTableError::InvalidName { ref tag } if tag == name),
                "expected invalid name for {name:?}, got {err:?}"
            );
        }
        // Unicode tag names lex fine and must be declarable.
        assert!(TagTable::build([pattern_decl("日本語タグ", ".*")]).is_ok());
    }

    #[test]
    fn test_custom_arg_kind_roundtrip() {
        for kind in [
            CustomArgKind::Duration,
            CustomArgKind::Date,
            CustomArgKind::Timestamp,
            CustomArgKind::Slug,
        ] {
            assert_eq!(CustomArgKind::from_str(kind.as_str()), Some(kind));
        }
        assert_eq!(CustomArgKind::from_str("frobnicate"), None);
    }
}

//! User-defined tag interpretation (plan §10, T1).
//!
//! A [`TagTable`] holds compiled user tag declarations, typically parsed from
//! a `[tags]` config section by a consumer crate. The table changes nothing
//! about lexing — custom tags still lex as `UnknownTag` plus the greedy
//! argument rule — it only upgrades those tags to [`TagKind::Custom`] at
//! parse level via [`parse_document_with`].
//!
//! Declarations arrive as plain Rust data ([`TagDeclaration`]) so that config
//! layers (TOML, YAML, …) stay out of this crate. Each declaration carries
//! exactly one interpretation rule:
//!
//! - `pattern`: a regex (the `regex` crate — deliberately: no lookaround, no
//!   catastrophic backtracking) with named capture groups; compiled once when
//!   the table is built.
//! - `kind`: a shorthand reusing a built-in argument parser
//!   (`duration` | `date` | `timestamp` | `slug`).
//!
//! [`TagKind::Custom`]: crate::tags::TagKind::Custom
//! [`parse_document_with`]: crate::parser::parse_document_with

use std::collections::HashMap;

use regex::Regex;

use crate::tokens::Keyword;

/// A single user tag declaration, as plain data (one `[tags.<name>]` entry).
///
/// Exactly one of `pattern` and `kind` must be set; [`TagTable::build`]
/// rejects everything else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagDeclaration {
    /// The tag name (without the leading `#`).
    pub name: String,
    /// Regex with named capture groups, matched against the raw argument.
    pub pattern: Option<String>,
    /// Shorthand reusing a built-in argument parser.
    pub kind: Option<CustomArgKind>,
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
    /// Neither `pattern` nor `kind` was given.
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
                write!(f, "tag '{tag}': declare either 'pattern' or 'kind'")
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

/// Compiled user tag declarations, consulted when the parser types a tag
/// whose name is not a built-in keyword.
///
/// Build one with [`TagTable::build`] and pass it to
/// [`parse_document_with`](crate::parser::parse_document_with). The empty
/// table ([`TagTable::empty`] / [`Default`]) reproduces
/// [`parse_document`](crate::parser::parse_document) exactly.
#[derive(Debug, Clone, Default)]
pub struct TagTable {
    rules: HashMap<String, CustomRule>,
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
                (None, None) => return Err(TagTableError::MissingRule { tag }),
                (Some(pattern), None) => match Regex::new(&pattern) {
                    Ok(re) => CustomRule::Pattern(re),
                    Err(error) => {
                        return Err(TagTableError::InvalidPattern {
                            tag,
                            error: Box::new(error),
                        });
                    }
                },
                (None, Some(kind)) => CustomRule::Kind(kind),
            };
            if rules.insert(tag.clone(), rule).is_some() {
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

    /// The compiled rule for `name`, if declared.
    pub fn get(&self, name: &str) -> Option<&CustomRule> {
        self.rules.get(name)
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
    /// groups in pattern order, or the single kind name. `None` when `name`
    /// is not declared.
    pub fn group_names(&self, name: &str) -> Option<Vec<String>> {
        match self.rules.get(name)? {
            CustomRule::Pattern(re) => {
                Some(re.capture_names().flatten().map(str::to_string).collect())
            }
            CustomRule::Kind(kind) => Some(vec![kind.as_str().to_string()]),
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
            kind: None,
        }
    }

    fn kind_decl(name: &str, kind: CustomArgKind) -> TagDeclaration {
        TagDeclaration {
            name: name.to_string(),
            pattern: None,
            kind: Some(kind),
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
        }])
        .unwrap_err();
        assert!(matches!(err, TagTableError::ConflictingRule { ref tag } if tag == "book"));
    }

    #[test]
    fn test_build_missing_rule() {
        let err = TagTable::build([TagDeclaration {
            name: "book".to_string(),
            pattern: None,
            kind: None,
        }])
        .unwrap_err();
        assert!(matches!(err, TagTableError::MissingRule { ref tag } if tag == "book"));
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

/// Re-exported YAML library used for [`ast::Frontmatter::data`], so
/// downstream crates can match on the value without their own dependency.
pub use saphyr;

pub mod ast;
pub mod error;
pub mod lexer;
pub mod line_index;
pub mod parser;
pub mod span;
pub mod tag_table;
pub mod tags;
pub mod tokens;

pub use ast::*;
pub use error::{ParseError, ParseErrorKind};
pub use line_index::{LineCol, LineIndex};
pub use parser::{parse_document, parse_document_with};
pub use span::Span;
pub use tag_table::{ArgShape, CustomArgKind, CustomRule, TagDeclaration, TagTable, TagTableError};
pub use tags::*;

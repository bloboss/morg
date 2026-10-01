//! `morg emit-grammar` — render tree-sitter-morg's generated region
//! (plan §10.4 T2).
//!
//! The grammar's `grammar.js` keeps the built-in keyword list and the
//! per-shape custom-tag rules between `// BEGIN GENERATED (morg
//! emit-grammar)` / `// END GENERATED` markers. This command re-renders that
//! region from the canonical keyword list ([`Keyword::all`]) plus the user's
//! `[tags]` declarations: every tag with a non-greedy `shape` becomes a
//! token-level grammar rule mirroring the Rust lexer's extent semantics as
//! closely as tree-sitter allows; greedy and undeclared tags stay on the
//! default rule. The rewrite is idempotent — re-running on an up-to-date file
//! is byte-identical — and with an empty `[tags]` it reproduces the committed
//! built-ins-only default exactly.
//!
//! The command never runs `npm`/`npx` itself; it prints the next steps.

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use morg_parser::tag_table::ArgShape;
use morg_parser::tokens::Keyword;

use crate::config::Config;

const BEGIN_MARKER: &str = "// BEGIN GENERATED (morg emit-grammar)";
const END_MARKER: &str = "// END GENERATED";

pub fn run(cfg: &Config, grammar_dir: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let dir: PathBuf = match grammar_dir {
        Some(dir) => dir.to_path_buf(),
        None => cfg.grammar.dir.clone().ok_or(
            "no grammar directory: pass --grammar-dir DIR \
             or set `dir` under [grammar] in the config",
        )?,
    };
    let path = dir.join("grammar.js");
    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    // Validate the whole [tags] section (unknown shapes, built-in
    // collisions, bad patterns) exactly as document parsing would.
    cfg.build_tag_table()
        .map_err(|e| format!("invalid [tags] config: {e}"))?;

    // Config::tags is a BTreeMap, so the emitted rules are deterministically
    // ordered by tag name.
    let shaped: Vec<(String, ArgShape)> = cfg
        .tags
        .iter()
        .filter_map(|(name, tc)| {
            let shape = ArgShape::from_str(tc.shape.as_deref()?)?;
            (shape != ArgShape::Greedy).then(|| (name.clone(), shape))
        })
        .collect();

    // Distinct tag names may transliterate to the same JS rule key
    // (`my-tag` and `my_tag` both become `inline_tag_my_tag`); a duplicate
    // object key would silently drop a rule, so refuse instead.
    let mut keys: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for (name, _) in &shaped {
        if let Some(prev) = keys.insert(rule_key(name), name) {
            return Err(format!(
                "tags #{prev} and #{name} map to the same grammar rule key \
                 `{}`; rename one of them",
                rule_key(name)
            )
            .into());
        }
    }

    let region = render_region(&shaped);
    let updated = replace_region(&source, &region, &path)?;

    if updated == source {
        println!("{} is up to date (no changes)", path.display());
    } else {
        std::fs::write(&path, &updated)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!(
            "updated {} ({} shaped tag{})",
            path.display(),
            shaped.len(),
            if shaped.len() == 1 { "" } else { "s" }
        );
    }

    println!();
    println!("next steps (not run automatically):");
    println!("  cd {} && npx tree-sitter generate", dir.display());
    println!("  rebuild the parser where you use it (nvim: :TSInstall! morg)");
    Ok(())
}

/// Replace the marked region of `source` with `region` (which includes the
/// markers). Errors when the markers are missing, duplicated, or out of
/// order, so a hand-edited file never gets mangled.
fn replace_region(source: &str, region: &str, path: &Path) -> Result<String, Box<dyn Error>> {
    let find_unique = |marker: &str| -> Result<usize, Box<dyn Error>> {
        let first = source
            .find(marker)
            .ok_or_else(|| format!("{}: marker '{marker}' not found", path.display()))?;
        if source[first + marker.len()..].contains(marker) {
            return Err(format!(
                "{}: marker '{marker}' appears more than once",
                path.display()
            )
            .into());
        }
        Ok(first)
    };
    let begin = find_unique(BEGIN_MARKER)?;
    let end = find_unique(END_MARKER)?;
    if end < begin {
        return Err(format!("{}: generated-region markers out of order", path.display()).into());
    }
    let mut out = String::with_capacity(source.len() + region.len());
    out.push_str(&source[..begin]);
    out.push_str(region);
    out.push_str(&source[end + END_MARKER.len()..]);
    Ok(out)
}

/// Render the full generated region, markers included (no trailing newline —
/// the text after the END marker in grammar.js supplies it).
fn render_region(shaped: &[(String, ArgShape)]) -> String {
    let mut s = String::new();
    s.push_str(BEGIN_MARKER);
    s.push('\n');
    s.push_str(
        "// Built-in keyword tags, from define_keywords! in\n\
         // morg/crates/morg-parser/src/tokens.rs.\n\
         const KEYWORDS = [\n",
    );
    for kw in Keyword::all() {
        let _ = writeln!(s, "  \"{}\",", kw.as_str());
    }
    s.push_str("];\n\n");
    s.push_str(
        "// Per-shape rules for custom tags declared in the user's [tags] config\n\
         // (plan 10.4 T2): one rule per non-greedy shaped tag, spread into the\n\
         // grammar before tag_name so the literal name token wins the equal-length\n\
         // token tie. Each rule mirrors the Rust lexer's extent semantics as\n\
         // closely as tree-sitter's regex tokens allow; the plain tag_argument\n\
         // branch is the greedy fallback. Default: empty — built-ins only, every\n\
         // tag greedy. Regenerate with `morg emit-grammar --grammar-dir DIR`.\n",
    );
    if shaped.is_empty() {
        s.push_str("const SHAPED_TAG_RULES = {};\n");
    } else {
        s.push_str("const SHAPED_TAG_RULES = {\n");
        for (name, shape) in shaped {
            s.push_str(&render_rule(name, *shape));
        }
        s.push_str("};\n");
    }
    s.push_str(END_MARKER);
    s
}

/// One `inline_tag_<name>` rule: the literal `#name` token (aliased to
/// tag_name) followed by an optional argument — the shape's branch when it
/// matches, the generic greedy tag_argument otherwise.
fn render_rule(name: &str, shape: ArgShape) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "  // #{name} — shape: {shape}.");
    let _ = writeln!(s, "  {}: ($) =>", rule_key(name));
    s.push_str("    prec.right(\n      seq(\n        field(\"name\", alias(token(\"#");
    s.push_str(name);
    s.push_str(
        "\"), $.tag_name)),\n        optional(\n          field(\n            \"argument\",\n            choice(\n              ",
    );
    s.push_str(&shape_branch(shape));
    s.push_str(
        ",\n              $.tag_argument,\n            ),\n          ),\n        ),\n      ),\n    ),\n",
    );
    s
}

/// The grammar branch for a shape's argument, aliased to tag_argument.
///
/// Every shape except `kv` is a single token. `kv` must not be: a token
/// regex `pair([ \t]+pair)*` shares its space transition out of the
/// accepting pair state with the greedy tag_argument token's DFA path, so
/// when the trailing repetition fails mid-way (`a=1 rest`) the lexer has
/// already walked past the prec-2 accept and settles on the longer prec-0
/// greedy match. One token per pair with a syntactic repeat1 keeps each
/// accept final, and inline whitespace between pairs is handled by extras.
fn shape_branch(shape: ArgShape) -> String {
    match shape {
        ArgShape::Kv => {
            format!("alias(prec.right(repeat1(token(prec(2, /{KV_PAIR_REGEX}/)))), $.tag_argument)")
        }
        _ => format!(
            "alias(token(prec(2, /{}/)), $.tag_argument)",
            shape_token_regex(shape)
        ),
    }
}

/// One `key=value` pair: tag-name-charset key, quoted or unquoted value (an
/// unquoted value never starts with `"`, mirroring the Rust lexer's
/// quoted-first scan).
const KV_PAIR_REGEX: &str = r#"[\p{L}\p{N}_-]+=("(\\[^\r\n]|[^"\\\r\n])*"|[^\s#"][^\s#]*)"#;

/// The tree-sitter token regex for a single-token shape. Divergences from
/// the Rust lexer (documented in tree-sitter-morg's README): `word` and
/// unquoted `kv` values stop at any `#` (the Rust lexer only stops at a `#`
/// that starts a tag), `quoted` keeps its quotes and escapes in the node
/// text (the Rust lexer unescapes), and `until-punct` keeps interior
/// trailing spaces out via its final character class rather than trimming.
fn shape_token_regex(shape: ArgShape) -> &'static str {
    match shape {
        ArgShape::Greedy => unreachable!("greedy tags stay on the default rule"),
        ArgShape::Kv => unreachable!("kv renders as a repeat1 of pair tokens"),
        ArgShape::Quoted => r#""(\\[^\r\n]|[^"\\\r\n])*""#,
        ArgShape::Word => r"[^\s#]+",
        ArgShape::UntilPunct => r"[^.,;:!?\s]([^.,;:!?\r\n]*[^.,;:!?\s])?",
    }
}

/// JS-safe rule key for a tag name. Tag names allow Unicode alphanumerics,
/// `-` and `_`; anything outside `[A-Za-z0-9_]` is transliterated to a
/// `u<codepoint>` chunk so the key stays a plain ASCII identifier.
fn rule_key(name: &str) -> String {
    let mut key = String::from("inline_tag_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            key.push(c);
        } else if c == '-' {
            key.push('_');
        } else {
            let _ = write!(key, "u{:x}", c as u32);
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_region_default_is_builtins_only() {
        let region = render_region(&[]);
        assert!(region.starts_with(BEGIN_MARKER));
        assert!(region.ends_with(END_MARKER));
        assert!(region.contains("const SHAPED_TAG_RULES = {};"));
        for kw in Keyword::all() {
            assert!(region.contains(&format!("\"{}\",", kw.as_str())), "{kw}");
        }
    }

    #[test]
    fn test_render_region_shaped_rules() {
        let region = render_region(&[
            ("dep".to_string(), ArgShape::Kv),
            ("task".to_string(), ArgShape::Quoted),
        ]);
        assert!(region.contains("inline_tag_dep: ($) =>"));
        assert!(region.contains("inline_tag_task: ($) =>"));
        assert!(region.contains("token(\"#task\")"));
        assert!(region.contains("$.tag_argument,")); // greedy fallback branch
    }

    #[test]
    fn test_replace_region_is_idempotent() {
        let file = format!(
            "// header\n{}\nold stuff\n{}\n\nrest of grammar\n",
            BEGIN_MARKER, END_MARKER
        );
        let region = render_region(&[("task".to_string(), ArgShape::Word)]);
        let path = Path::new("grammar.js");
        let once = replace_region(&file, &region, path).unwrap();
        let twice = replace_region(&once, &region, path).unwrap();
        assert_eq!(once, twice, "rewrite must be byte-identical on re-run");
        assert!(once.starts_with("// header\n"));
        assert!(once.ends_with("\n\nrest of grammar\n"));
    }

    #[test]
    fn test_replace_region_rejects_broken_markers() {
        let region = render_region(&[]);
        let path = Path::new("grammar.js");
        assert!(replace_region("no markers here\n", &region, path).is_err());
        let doubled = format!("{m}\n{m}\n{e}\n", m = BEGIN_MARKER, e = END_MARKER);
        assert!(replace_region(&doubled, &region, path).is_err());
        let reversed = format!("{e}\n{m}\n", m = BEGIN_MARKER, e = END_MARKER);
        assert!(replace_region(&reversed, &region, path).is_err());
    }

    #[test]
    fn test_rule_key_sanitizes_unicode_names() {
        assert_eq!(rule_key("task"), "inline_tag_task");
        assert_eq!(rule_key("clock-in"), "inline_tag_clock_in");
        let key = rule_key("日本語");
        assert!(key.starts_with("inline_tag_u"), "{key}");
        assert!(key.is_ascii());
    }
}

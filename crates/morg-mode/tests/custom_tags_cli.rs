//! CLI tests for user-defined tags: `morg tags <name>` and the `morg lint`
//! shape-mismatch warning, driven by a `[tags]` config fixture.
//!
//! Each test gets its own temp directory with an isolated
//! `XDG_CONFIG_HOME/morg/config.toml`, so the user's real config never leaks
//! into the run.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A self-cleaning fixture: config dir with `[tags]` declarations plus a
/// vault dir with one morg file.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(config_toml: &str, document: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("morg_custom_tags_cli_{}_{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/morg")).unwrap();
        std::fs::create_dir_all(root.join("vault")).unwrap();
        std::fs::write(root.join("config/morg/config.toml"), config_toml).unwrap();
        std::fs::write(root.join("vault/notes.md"), document).unwrap();
        Self { root }
    }

    fn vault(&self) -> PathBuf {
        self.root.join("vault")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_morg"))
            .args(args)
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("HOME", &self.root)
            .output()
            .expect("failed to run morg binary")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn vault_arg(fixture: &Fixture) -> String {
    fixture.vault().display().to_string()
}

fn notes_path(fixture: &Fixture) -> String {
    Path::new(&vault_arg(fixture))
        .join("notes.md")
        .display()
        .to_string()
}

const BOOK_CONFIG: &str = r#"
[tags.book]
pattern = '"(?<title>[^"]+)"\s+by\s+(?<author>.+)'

[tags.reading-time]
kind = "duration"
"#;

#[test]
fn test_tags_command_text_output() {
    let fixture = Fixture::new(
        BOOK_CONFIG,
        "# Reading\n\n#book \"Dune\" by Frank Herbert\n\nAlso: #book \"砂の惑星\" by ハーバート\n",
    );
    let out = fixture.run(&["tags", "book", &vault_arg(&fixture)]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Header has one column per named capture group, in pattern order.
    let header = stdout.lines().next().unwrap();
    assert!(header.contains("FILE"), "{stdout}");
    assert!(header.contains("LINE"), "{stdout}");
    assert!(header.contains("TITLE"), "{stdout}");
    assert!(header.contains("AUTHOR"), "{stdout}");

    assert!(stdout.contains("Dune"));
    assert!(stdout.contains("Frank Herbert"));
    assert!(stdout.contains("砂の惑星"));
    assert!(stdout.contains("2 occurrence(s)"), "{stdout}");
}

#[test]
fn test_tags_command_json_output() {
    let fixture = Fixture::new(
        BOOK_CONFIG,
        "#book \"Dune\" by Frank Herbert\n\n#book unquoted mismatch\n",
    );
    let out = fixture.run(&["--format", "json", "tags", "book", &vault_arg(&fixture)]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let items: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout should be JSON");
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), 2);

    assert_eq!(items[0]["file"], notes_path(&fixture).as_str());
    assert_eq!(items[0]["line"], 1);
    assert_eq!(items[0]["fields"]["title"], "Dune");
    assert_eq!(items[0]["fields"]["author"], "Frank Herbert");
    assert_eq!(items[0]["shape_mismatch"], false);

    assert_eq!(items[1]["line"], 3);
    assert_eq!(items[1]["shape_mismatch"], true);
    assert!(items[1]["fields"].as_object().unwrap().is_empty());
}

#[test]
fn test_tags_command_kind_shorthand() {
    let fixture = Fixture::new(BOOK_CONFIG, "- session one #reading-time 90m\n");
    let out = fixture.run(&[
        "--format",
        "json",
        "tags",
        "reading-time",
        &vault_arg(&fixture),
    ]);
    assert!(out.status.success());
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // The kind shorthand yields one field named after the kind, holding the
    // canonical rendering (90m -> 1h30m).
    assert_eq!(items[0]["fields"]["duration"], "1h30m");
}

#[test]
fn test_tags_command_undeclared_name_errors() {
    let fixture = Fixture::new(BOOK_CONFIG, "#movie \"Dune\"\n");
    let out = fixture.run(&["tags", "movie", &vault_arg(&fixture)]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not declared"), "{stderr}");
}

#[test]
fn test_lint_warns_on_shape_mismatch() {
    let fixture = Fixture::new(
        BOOK_CONFIG,
        "#book \"Dune\" by Frank Herbert\n\n#book Dune without quotes\n\n#reading-time ages\n",
    );
    let out = fixture.run(&["lint", &vault_arg(&fixture)]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("argument does not match the declared pattern for #book"),
        "{stdout}"
    );
    assert!(
        stdout.contains("argument does not match the declared pattern for #reading-time"),
        "{stdout}"
    );
    // The well-shaped occurrence is not flagged: exactly two warnings.
    assert!(stdout.contains("0 error(s), 2 warning(s)"), "{stdout}");
}

#[test]
fn test_lint_json_mismatch_warning() {
    let fixture = Fixture::new(BOOK_CONFIG, "#book no quotes here\n");
    let out = fixture.run(&["--format", "json", "lint", &vault_arg(&fixture)]);
    assert!(out.status.success());
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["severity"], "warn");
    assert_eq!(items[0]["line"], 1);
    assert!(
        items[0]["message"]
            .as_str()
            .unwrap()
            .contains("declared pattern for #book")
    );
}

#[test]
fn test_invalid_tags_config_warns_and_degrades() {
    // A broken [tags] section must not take unrelated commands down: morg
    // warns on stderr and parses with an empty table.
    let fixture = Fixture::new(
        "[tags.book]\npattern = '('\n",
        "#book \"Dune\" by Frank Herbert\n",
    );
    let out = fixture.run(&["lint", &vault_arg(&fixture)]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid [tags] config"), "{stderr}");
    assert!(stderr.contains("book"), "{stderr}");
    // No mismatch warnings without a table.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("No issues found"), "{stdout}");
}

#[test]
fn test_lint_warns_on_shape_fallback() {
    // An extent shape (plan §10.4 T2) that fails at the lexer falls back to
    // the greedy rule and surfaces through the same shape_mismatch warning.
    let fixture = Fixture::new(
        "[tags.task]\nshape = \"quoted\"\n",
        "intro #task \"well shaped\" prose\n\nintro #task not quoted at all\n",
    );
    let out = fixture.run(&["lint", &vault_arg(&fixture)]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("argument does not match the declared pattern for #task"),
        "{stdout}"
    );
    assert!(stdout.contains("0 error(s), 1 warning(s)"), "{stdout}");
}

#[test]
fn test_invalid_shape_config_warns_and_degrades() {
    let fixture = Fixture::new("[tags.task]\nshape = \"regex\"\n", "intro #task whatever\n");
    let out = fixture.run(&["lint", &vault_arg(&fixture)]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown shape") && stderr.contains("until-punct"),
        "{stderr}"
    );
}

#[test]
fn test_builtin_collision_config_warns() {
    let fixture = Fixture::new(
        "[tags.deadline]\nkind = \"date\"\n",
        "#deadline 2099-01-01\n",
    );
    let out = fixture.run(&["lint", &vault_arg(&fixture)]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid [tags] config") && stderr.contains("built-in"),
        "{stderr}"
    );
}

// ---------------------------------------------------------------------------
// `morg emit-grammar`
// ---------------------------------------------------------------------------

/// A stand-in tree-sitter-morg grammar.js: only the marked region matters.
const GRAMMAR_STUB: &str = "// header kept verbatim\n\
// BEGIN GENERATED (morg emit-grammar)\n\
stale contents to be replaced\n\
// END GENERATED\n\
\n// trailer kept verbatim\n";

const SHAPES_CONFIG: &str = r#"
[tags.ztitle]
shape = "quoted"

[tags.zver]
shape = "word"

[tags.zdep]
shape = "kv"

[tags.znote]
shape = "until-punct"

[tags.zfree]
shape = "greedy"

[tags.book]
pattern = '"(?<title>[^"]+)"\s+by\s+(?<author>.+)'
"#;

#[test]
fn test_emit_grammar_renders_shaped_rules_idempotently() {
    let fixture = Fixture::new(SHAPES_CONFIG, "unused\n");
    let dir = fixture.root.join("ts");
    std::fs::create_dir_all(&dir).unwrap();
    let grammar = dir.join("grammar.js");
    std::fs::write(&grammar, GRAMMAR_STUB).unwrap();
    let dir_arg = dir.display().to_string();

    let out = fixture.run(&["emit-grammar", "--grammar-dir", &dir_arg]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("4 shaped tags"), "{stdout}");
    assert!(stdout.contains("tree-sitter generate"), "{stdout}");

    let once = std::fs::read_to_string(&grammar).unwrap();
    // Text outside the markers survives verbatim.
    assert!(once.starts_with("// header kept verbatim\n"), "{once}");
    assert!(once.ends_with("\n// trailer kept verbatim\n"), "{once}");
    // One rule per non-greedy shaped tag; greedy and pattern-only tags stay
    // on the default rule, built-ins come from the canonical keyword list.
    for rule in [
        "inline_tag_ztitle",
        "inline_tag_zver",
        "inline_tag_zdep",
        "inline_tag_znote",
    ] {
        assert!(once.contains(rule), "{rule} missing:\n{once}");
    }
    assert!(!once.contains("inline_tag_zfree"), "{once}");
    assert!(!once.contains("inline_tag_book"), "{once}");
    assert!(once.contains("\"clock-in\","), "{once}");
    assert!(!once.contains("stale contents"), "{once}");

    // Re-running is byte-identical and says so.
    let out = fixture.run(&["emit-grammar", "--grammar-dir", &dir_arg]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("up to date"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let twice = std::fs::read_to_string(&grammar).unwrap();
    assert_eq!(once, twice, "emit-grammar must be idempotent");
}

#[test]
fn test_emit_grammar_empty_tags_renders_default_region() {
    let fixture = Fixture::new("", "unused\n");
    let dir = fixture.root.join("ts");
    std::fs::create_dir_all(&dir).unwrap();
    let grammar = dir.join("grammar.js");
    std::fs::write(&grammar, GRAMMAR_STUB).unwrap();
    let dir_arg = dir.display().to_string();

    let out = fixture.run(&["emit-grammar", "--grammar-dir", &dir_arg]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = std::fs::read_to_string(&grammar).unwrap();
    assert!(
        content.contains("const SHAPED_TAG_RULES = {};"),
        "{content}"
    );
    assert!(content.contains("const KEYWORDS = ["), "{content}");
}

#[test]
fn test_emit_grammar_requires_a_grammar_dir() {
    let fixture = Fixture::new("", "unused\n");
    let out = fixture.run(&["emit-grammar"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--grammar-dir"), "{stderr}");
}

#[test]
fn test_emit_grammar_rejects_rule_key_collision() {
    let fixture = Fixture::new(
        "[tags.my-tag]\nshape = \"word\"\n\n[tags.my_tag]\nshape = \"quoted\"\n",
        "unused\n",
    );
    let dir = fixture.root.join("ts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("grammar.js"), GRAMMAR_STUB).unwrap();
    let out = fixture.run(&["emit-grammar", "--grammar-dir", &dir.display().to_string()]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("same grammar rule key"), "{stderr}");
}

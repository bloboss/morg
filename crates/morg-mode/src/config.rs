//! Global morg-mode configuration.
//!
//! Config file location: `$XDG_CONFIG_HOME/morg/config.toml`
//! (typically `~/.config/morg/config.toml`)

use std::collections::BTreeMap;
use std::path::PathBuf;

use morg_parser::tag_table::{CustomArgKind, TagDeclaration, TagTable};
use serde::Deserialize;

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root directory for morg files. All commands use this as the default
    /// search path when no files/directories are specified.
    pub root: PathBuf,

    /// Diary configuration.
    pub diary: DiaryConfig,

    /// Capture template configuration (legacy — still read from capture.yaml too).
    pub capture: CaptureConfig,

    /// User-defined tag declarations: `[tags.<name>]` with `pattern` (regex
    /// with named capture groups) or `kind` (duration | date | timestamp |
    /// slug). BTreeMap keeps build errors deterministic.
    pub tags: BTreeMap<String, TagConfig>,
}

/// One `[tags.<name>]` entry. Exactly one of `pattern`/`kind` must be set;
/// [`Config::build_tag_table`] enforces that.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct TagConfig {
    /// Regex with named capture groups, matched against the raw argument.
    pub pattern: Option<String>,
    /// Built-in argument parser shorthand.
    pub kind: Option<String>,
}

/// Diary configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DiaryConfig {
    /// Directory where diary notes are stored.
    /// Default: `<root>/diary`
    pub directory: Option<PathBuf>,

    /// Path to the daily note template file.
    /// Default: `<diary_directory>/daily_note.template`
    pub template: Option<PathBuf>,

    /// Filename for the active "today" note.
    /// Default: `today.md`
    pub today_file: String,

    /// Archive directory pattern inside the diary directory.
    /// Supports `{year}`, `{month}`, `{day}` placeholders.
    /// Default: `{year}/{month}`
    pub archive_pattern: String,

    /// Archive filename pattern.
    /// Default: `{day}.md`
    pub archive_filename: String,

    /// Whether to carry over unchecked todos from the previous note.
    /// Default: true
    pub carry_todos: bool,
}

/// Capture configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct CaptureConfig {
    /// Path to capture templates YAML file.
    /// Default: `$XDG_CONFIG_HOME/morg/capture.yaml`
    pub templates_file: Option<PathBuf>,
}

// ---- Defaults ----

impl Default for Config {
    fn default() -> Self {
        Self {
            root: default_root(),
            diary: DiaryConfig::default(),
            capture: CaptureConfig::default(),
            tags: BTreeMap::new(),
        }
    }
}

impl Default for DiaryConfig {
    fn default() -> Self {
        Self {
            directory: None,
            template: None,
            today_file: "today.md".to_string(),
            archive_pattern: "{year}/{month}".to_string(),
            archive_filename: "{day}.md".to_string(),
            carry_todos: true,
        }
    }
}

impl Config {
    /// Resolved diary directory (diary.directory or root/diary).
    pub fn diary_dir(&self) -> PathBuf {
        self.diary
            .directory
            .clone()
            .unwrap_or_else(|| self.root.join("diary"))
    }

    /// Resolved template path.
    pub fn diary_template(&self) -> PathBuf {
        self.diary
            .template
            .clone()
            .unwrap_or_else(|| self.diary_dir().join("daily_note.template"))
    }

    /// Resolved today file path.
    pub fn diary_today(&self) -> PathBuf {
        self.diary_dir().join(&self.diary.today_file)
    }

    /// Compile the `[tags]` section into a [`TagTable`]. Built once at
    /// startup and passed to the commands that parse documents.
    pub fn build_tag_table(&self) -> Result<TagTable, Box<dyn std::error::Error>> {
        let declarations = self
            .tags
            .iter()
            .map(|(name, tc)| {
                let kind = match tc.kind.as_deref() {
                    Some(k) => Some(CustomArgKind::from_str(k).ok_or_else(|| {
                        format!(
                            "tag '{name}': unknown kind '{k}' \
                             (expected duration, date, timestamp, or slug)"
                        )
                    })?),
                    None => None,
                };
                Ok(TagDeclaration {
                    name: name.clone(),
                    pattern: tc.pattern.clone(),
                    kind,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(TagTable::build(declarations)?)
    }

    /// Expand `~/` in all path fields.
    fn resolve_tildes(&mut self) {
        use crate::util::expand_tilde;
        self.root = expand_tilde(&self.root);
        if let Some(ref p) = self.diary.directory {
            self.diary.directory = Some(expand_tilde(p));
        }
        if let Some(ref p) = self.diary.template {
            self.diary.template = Some(expand_tilde(p));
        }
        if let Some(ref p) = self.capture.templates_file {
            self.capture.templates_file = Some(expand_tilde(p));
        }
    }
}

// ---- Loading ----

/// Path to the config file.
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Config directory ($XDG_CONFIG_HOME/morg).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("morg")
}

fn default_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".morg")
}

/// Load configuration from disk, falling back to defaults.
pub fn load() -> Config {
    let path = config_path();
    if !path.exists() {
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(mut config) => {
                config.resolve_tildes();
                config
            }
            Err(e) => {
                eprintln!("warning: invalid config at {}: {e}", path.display());
                Config::default()
            }
        },
        Err(e) => {
            eprintln!("warning: cannot read config at {}: {e}", path.display());
            Config::default()
        }
    }
}

/// Initialize config directory and write a default config.toml if none exists.
pub fn init_config() -> Result<(), Box<dyn std::error::Error>> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;

    let path = config_path();
    if path.exists() {
        println!("Config already exists at {}", path.display());
        return Ok(());
    }

    let default_toml = r#"# morg-mode configuration
# Location: $XDG_CONFIG_HOME/morg/config.toml

# Root directory for all morg files.
# All commands default to searching here when no files are specified.
# root = "~/.morg"

[diary]
# directory = "~/.diary"              # Where diary notes are stored
# template = "~/.diary/daily_note.template"  # Daily note template
# today_file = "today.md"             # Active note filename
# archive_pattern = "{year}/{month}"  # Archive subdirectory pattern
# archive_filename = "{day}.md"       # Archive filename pattern
# carry_todos = true                  # Carry unchecked todos to new notes

[capture]
# templates_file = "~/.config/morg/capture.yaml"

# User-defined tags: declare how a custom tag's argument is interpreted.
# Either a regex `pattern` with named capture groups, or a `kind` shorthand
# (duration | date | timestamp | slug). Built-in tag names cannot be redefined.
# [tags.book]
# pattern = '"(?<title>[^"]+)"\s+by\s+(?<author>.+)'
# [tags.reading-time]
# kind = "duration"
"#;

    std::fs::write(&path, default_toml)?;
    println!("Created default config at {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert!(cfg.root.to_string_lossy().ends_with(".morg"));
        assert!(cfg.diary.directory.is_none());
        assert!(cfg.diary.template.is_none());
        assert_eq!(cfg.diary.today_file, "today.md");
        assert_eq!(cfg.diary.archive_pattern, "{year}/{month}");
        assert_eq!(cfg.diary.archive_filename, "{day}.md");
        assert!(cfg.diary.carry_todos);
    }

    #[test]
    fn test_diary_dir_defaults_to_root_diary() {
        let cfg = Config::default();
        let diary_dir = cfg.diary_dir();
        assert!(diary_dir.to_string_lossy().ends_with(".morg/diary"));
    }

    #[test]
    fn test_diary_dir_uses_configured_value() {
        let mut cfg = Config::default();
        cfg.diary.directory = Some(PathBuf::from("/custom/diary"));
        assert_eq!(cfg.diary_dir(), PathBuf::from("/custom/diary"));
    }

    #[test]
    fn test_diary_template_defaults() {
        let cfg = Config::default();
        let tmpl = cfg.diary_template();
        assert!(tmpl.to_string_lossy().contains("daily_note.template"));
    }

    #[test]
    fn test_diary_today_path() {
        let cfg = Config::default();
        let today = cfg.diary_today();
        assert!(today.to_string_lossy().ends_with("today.md"));
    }

    #[test]
    fn test_parse_toml_full() {
        let toml_str = r#"
root = "/home/user/notes"

[diary]
directory = "/home/user/diary"
template = "/home/user/diary/my_template.md"
today_file = "current.md"
archive_pattern = "{year}-{month}"
archive_filename = "day-{day}.md"
carry_todos = false

[capture]
templates_file = "/home/user/.config/morg/capture.yaml"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.root, PathBuf::from("/home/user/notes"));
        assert_eq!(cfg.diary.directory, Some(PathBuf::from("/home/user/diary")));
        assert_eq!(
            cfg.diary.template,
            Some(PathBuf::from("/home/user/diary/my_template.md"))
        );
        assert_eq!(cfg.diary.today_file, "current.md");
        assert_eq!(cfg.diary.archive_pattern, "{year}-{month}");
        assert_eq!(cfg.diary.archive_filename, "day-{day}.md");
        assert!(!cfg.diary.carry_todos);
        assert_eq!(
            cfg.capture.templates_file,
            Some(PathBuf::from("/home/user/.config/morg/capture.yaml"))
        );
    }

    #[test]
    fn test_parse_toml_minimal() {
        let toml_str = "";
        let cfg: Config = toml::from_str(toml_str).unwrap();
        // All defaults should apply
        assert!(cfg.root.to_string_lossy().ends_with(".morg"));
        assert!(cfg.diary.carry_todos);
    }

    #[test]
    fn test_parse_toml_tags_section() {
        let toml_str = r#"
[tags.book]
pattern = '"(?<title>[^"]+)"\s+by\s+(?<author>.+)'

[tags.reading-time]
kind = "duration"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tags.len(), 2);
        let table = cfg.build_tag_table().unwrap();
        assert!(table.contains("book"));
        assert_eq!(
            table.group_names("reading-time").unwrap(),
            vec!["duration".to_string()]
        );
    }

    #[test]
    fn test_build_tag_table_errors() {
        // Unknown kind word.
        let cfg: Config = toml::from_str("[tags.x]\nkind = \"frobnicate\"\n").unwrap();
        let err = cfg.build_tag_table().unwrap_err().to_string();
        assert!(err.contains("x") && err.contains("frobnicate"), "{err}");

        // Table-build errors (bad regex, builtin collision, conflict)
        // propagate with the tag name.
        let cfg: Config = toml::from_str("[tags.y]\npattern = \"(\"\n").unwrap();
        assert!(cfg.build_tag_table().unwrap_err().to_string().contains("y"));

        let cfg: Config = toml::from_str("[tags.deadline]\nkind = \"date\"\n").unwrap();
        let err = cfg.build_tag_table().unwrap_err().to_string();
        assert!(
            err.contains("deadline") && err.contains("built-in"),
            "{err}"
        );

        let cfg: Config = toml::from_str("[tags.z]\npattern = \".*\"\nkind = \"slug\"\n").unwrap();
        let err = cfg.build_tag_table().unwrap_err().to_string();
        assert!(err.contains("mutually exclusive"), "{err}");
    }

    #[test]
    fn test_default_config_empty_tag_table() {
        let table = Config::default().build_tag_table().unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn test_parse_toml_partial() {
        let toml_str = r#"
[diary]
carry_todos = false
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        // Only carry_todos changed, rest defaults
        assert!(!cfg.diary.carry_todos);
        assert_eq!(cfg.diary.today_file, "today.md");
        assert!(cfg.diary.directory.is_none());
    }
}

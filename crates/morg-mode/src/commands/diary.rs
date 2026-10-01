//! `morg diary` — daily note manager.
//!
//! Opens today's diary note, creating a new one from a template if the current
//! note is outdated or missing. Stale notes are archived into a date-based
//! directory structure and unchecked todos are carried forward.

use std::path::Path;

use chrono::Local;

use crate::config::Config;
use crate::util::expand_tilde;

pub fn run(config: &Config, no_edit: bool) -> Result<(), Box<dyn std::error::Error>> {
    let diary_dir = expand_tilde(&config.diary_dir());
    let template_path = expand_tilde(&config.diary_template());
    let today_path = expand_tilde(&config.diary_today());

    let now = Local::now();
    let current_date = now.format("%Y-%m-%d").to_string();
    let year = now.format("%Y").to_string();
    let month = now.format("%m").to_string();
    let day = now.format("%d").to_string();
    let week = now.format("%V").to_string();
    let time = now.format("%H:%M").to_string();

    // Ensure diary directory exists
    std::fs::create_dir_all(&diary_dir)?;

    // Check if template exists
    if !template_path.exists() {
        create_default_template(&template_path)?;
        eprintln!("Created default template at {}", template_path.display());
    }

    // Read the current today file's date (if it exists)
    let note_date = get_note_date(&today_path);

    if note_date.as_deref() == Some(current_date.as_str()) {
        // Today's note is current — just open it
        println!("Opening today's note ({current_date}).");
    } else {
        // Rotate: archive the old note, create a new one
        if let Some(ref old_date) = note_date {
            println!("Note date ({old_date}) does not match today ({current_date}). Rotating.");
        } else if today_path.exists() {
            println!("Note has no date. Rotating.");
        } else {
            println!("No existing note found. Creating a new note for {current_date}.");
        }

        // Archive using the OLD note's date for the archive path
        let (arch_year, arch_month, arch_day) = if let Some(ref d) = note_date {
            parse_date_parts(d)
        } else {
            (year.clone(), month.clone(), day.clone())
        };

        let old_archive_subdir = config
            .diary
            .archive_pattern
            .replace("{year}", &arch_year)
            .replace("{month}", &arch_month)
            .replace("{day}", &arch_day);
        let old_archive_filename = config
            .diary
            .archive_filename
            .replace("{year}", &arch_year)
            .replace("{month}", &arch_month)
            .replace("{day}", &arch_day);

        let archive_dir = diary_dir.join(&old_archive_subdir);
        let archive_file = archive_dir.join(&old_archive_filename);

        rotate_note(
            &today_path,
            &template_path,
            &archive_dir,
            &archive_file,
            &current_date,
            &week,
            &time,
            config.diary.carry_todos,
        )?;
    }

    if no_edit {
        println!("{}", today_path.display());
    } else {
        // Open in $EDITOR
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());

        let status = std::process::Command::new(&editor)
            .arg(&today_path)
            .status()?;

        if !status.success() {
            return Err(format!("{editor} exited with {status}").into());
        }
    }

    Ok(())
}

/// Extract the `date:` field from YAML frontmatter.
fn get_note_date(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_frontmatter = false;

    for line in content.lines() {
        if line.trim() == "---" {
            if in_frontmatter {
                return None; // End of frontmatter without finding date
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter && let Some(rest) = line.strip_prefix("date:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Archive the old note, create a fresh one from template, carry todos.
#[allow(clippy::too_many_arguments)]
fn rotate_note(
    today_path: &Path,
    template_path: &Path,
    archive_dir: &Path,
    archive_file: &Path,
    current_date: &str,
    week: &str,
    time: &str,
    carry_todos: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Archive existing note
    let mut unchecked_todos: Vec<String> = Vec::new();

    if today_path.exists() {
        std::fs::create_dir_all(archive_dir)?;
        std::fs::copy(today_path, archive_file)?;
        println!("Archived previous note -> {}", archive_file.display());

        // Collect unchecked todos from the archived note
        if carry_todos {
            let content = std::fs::read_to_string(archive_file)?;
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("- [ ] ") && trimmed.len() > 6 {
                    unchecked_todos.push(line.to_string());
                }
            }
        }
    }

    // Create new note from template
    let template = std::fs::read_to_string(template_path)?;
    let mut note = template
        .replace("YYYY-MM-DD", current_date)
        .replace("WW", week)
        .replace("HH:MM", time);

    // Carry over unchecked todos
    if !unchecked_todos.is_empty() {
        // Replace the placeholder todo block (lines of `- [ ]` after `## TODOs`)
        let placeholder_block = "- [ ]\n- [ ]\n- [ ]\n";
        let carried = unchecked_todos.join("\n") + "\n";

        if note.contains(placeholder_block) {
            note = note.replace(placeholder_block, &carried);
        } else {
            // If no placeholder block, append after ## TODOs
            if let Some(pos) = note.find("## TODOs")
                && let Some(newline_pos) = note[pos..].find('\n')
            {
                let insert_at = pos + newline_pos + 1;
                // Skip blank line after heading
                let skip = if note[insert_at..].starts_with('\n') {
                    1
                } else {
                    0
                };
                note.insert_str(insert_at + skip, &carried);
            }
        }

        println!("Carried over {} unchecked todo(s).", unchecked_todos.len());
    }

    std::fs::write(today_path, &note)?;
    println!("Created new note from template.");

    Ok(())
}

fn parse_date_parts(date: &str) -> (String, String, String) {
    let parts: Vec<&str> = date.split('-').collect();
    (
        parts.first().unwrap_or(&"2026").to_string(),
        parts.get(1).unwrap_or(&"01").to_string(),
        parts.get(2).unwrap_or(&"01").to_string(),
    )
}

fn create_default_template(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        r#"---
date: YYYY-MM-DD
week: WW
created: HH:MM
---

# Daily Planner — YYYY-MM-DD

## Most Important Tasks

1.
2.
3.

---

## TODOs

- [ ]
- [ ]
- [ ]

---

## Time Blocks

| Time        | Block |
|-------------|-------|
| 06:00–06:30 |       |
| 06:30–07:00 |       |
| 07:00–07:30 |       |
| 07:30–08:00 |       |
| 08:00–08:30 |       |
| 08:30–09:00 |       |
| 09:00–09:30 |       |
| 09:30–10:00 |       |
| 10:00–10:30 |       |
| 10:30–11:00 |       |
| 11:00–11:30 |       |
| 11:30–12:00 |       |
| 12:00–12:30 |       |
| 12:30–13:00 |       |
| 13:00–13:30 |       |
| 13:30–14:00 |       |
| 14:00–14:30 |       |
| 14:30–15:00 |       |
| 15:00–15:30 |       |
| 15:30–16:00 |       |
| 16:00–16:30 |       |
| 16:30–17:00 |       |
| 17:00–17:30 |       |
| 17:30–18:00 |       |
| 18:00–18:30 |       |
| 18:30–19:00 |       |
| 19:00–19:30 |       |
| 19:30–20:00 |       |
| 20:00–20:30 |       |
| 20:30–21:00 |       |
| 21:00–21:30 |       |
| 21:30–22:00 |       |

---

## Diary

<!-- Free-form notes, reflections, and thoughts for the day -->

"#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("morg_diary_test_{}_{}", std::process::id(), id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[allow(unused)]
    fn make_config(dir: &Path) -> crate::config::Config {
        crate::config::Config {
            root: dir.to_path_buf(),
            diary: crate::config::DiaryConfig {
                directory: Some(dir.join("diary")),
                template: Some(dir.join("diary/daily_note.template")),
                today_file: "today.md".to_string(),
                archive_pattern: "{year}/{month}".to_string(),
                archive_filename: "{day}.md".to_string(),
                carry_todos: true,
            },
            capture: crate::config::CaptureConfig::default(),
            tags: Default::default(),
            grammar: Default::default(),
        }
    }

    #[test]
    fn test_get_note_date_valid() {
        let dir = temp_dir();
        let file = dir.join("test.md");
        fs::write(&file, "---\ndate: 2026-04-05\nweek: 14\n---\n# Hello\n").unwrap();

        assert_eq!(get_note_date(&file), Some("2026-04-05".to_string()));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_note_date_missing() {
        let dir = temp_dir();
        let file = dir.join("nonexistent.md");
        assert_eq!(get_note_date(&file), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_note_date_no_frontmatter() {
        let dir = temp_dir();
        let file = dir.join("nofront.md");
        fs::write(&file, "# Just a heading\nSome text.\n").unwrap();

        assert_eq!(get_note_date(&file), None);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_note_date_no_date_field() {
        let dir = temp_dir();
        let file = dir.join("nodate.md");
        fs::write(&file, "---\ntitle: Note\nweek: 14\n---\n# Hello\n").unwrap();

        assert_eq!(get_note_date(&file), None);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_create_default_template() {
        let dir = temp_dir();
        let template = dir.join("template.md");

        create_default_template(&template).unwrap();

        assert!(template.exists());
        let content = fs::read_to_string(&template).unwrap();
        assert!(content.contains("YYYY-MM-DD"));
        assert!(content.contains("## TODOs"));
        assert!(content.contains("## Time Blocks"));
        assert!(content.contains("## Diary"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_rotate_note_archives_and_creates_new() {
        let dir = temp_dir();
        let diary_dir = dir.join("diary");
        fs::create_dir_all(&diary_dir).unwrap();

        // Create a template
        let template = diary_dir.join("template.md");
        fs::write(&template, "---\ndate: YYYY-MM-DD\nweek: WW\ncreated: HH:MM\n---\n\n## TODOs\n\n- [ ]\n- [ ]\n- [ ]\n").unwrap();

        // Create a stale today.md with todos
        let today = diary_dir.join("today.md");
        fs::write(&today, "---\ndate: 2026-04-05\n---\n\n## TODOs\n\n- [ ] Buy milk\n- [x] Send email\n- [ ] Fix bug\n").unwrap();

        let archive_dir = diary_dir.join("2026/04");
        let archive_file = archive_dir.join("05.md");

        rotate_note(
            &today,
            &template,
            &archive_dir,
            &archive_file,
            "2026-04-06",
            "15",
            "09:30",
            true,
        )
        .unwrap();

        // Check archive exists
        assert!(archive_file.exists());
        let archived = fs::read_to_string(&archive_file).unwrap();
        assert!(archived.contains("date: 2026-04-05"));

        // Check new today.md has correct date
        let new_note = fs::read_to_string(&today).unwrap();
        assert!(new_note.contains("date: 2026-04-06"));
        assert!(new_note.contains("week: 15"));
        assert!(new_note.contains("created: 09:30"));

        // Check unchecked todos were carried over (but not the checked one)
        assert!(new_note.contains("- [ ] Buy milk"));
        assert!(new_note.contains("- [ ] Fix bug"));
        assert!(!new_note.contains("Send email"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_rotate_note_no_carry_todos() {
        let dir = temp_dir();
        let diary_dir = dir.join("diary");
        fs::create_dir_all(&diary_dir).unwrap();

        let template = diary_dir.join("template.md");
        fs::write(
            &template,
            "---\ndate: YYYY-MM-DD\n---\n\n## TODOs\n\n- [ ]\n- [ ]\n- [ ]\n",
        )
        .unwrap();

        let today = diary_dir.join("today.md");
        fs::write(
            &today,
            "---\ndate: 2026-04-05\n---\n\n- [ ] Leftover task\n",
        )
        .unwrap();

        let archive_dir = diary_dir.join("2026/04");
        let archive_file = archive_dir.join("05.md");

        rotate_note(
            &today,
            &template,
            &archive_dir,
            &archive_file,
            "2026-04-06",
            "15",
            "10:00",
            false, // carry_todos = false
        )
        .unwrap();

        let new_note = fs::read_to_string(&today).unwrap();
        // Placeholder todos should remain (not replaced with carried ones)
        assert!(new_note.contains("- [ ]\n- [ ]\n- [ ]"));
        assert!(!new_note.contains("Leftover task"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_rotate_note_no_existing_note() {
        let dir = temp_dir();
        let diary_dir = dir.join("diary");
        fs::create_dir_all(&diary_dir).unwrap();

        let template = diary_dir.join("template.md");
        fs::write(&template, "---\ndate: YYYY-MM-DD\n---\n# Planner\n").unwrap();

        let today = diary_dir.join("today.md");
        // today.md doesn't exist

        let archive_dir = diary_dir.join("2026/04");
        let archive_file = archive_dir.join("06.md");

        rotate_note(
            &today,
            &template,
            &archive_dir,
            &archive_file,
            "2026-04-06",
            "15",
            "08:00",
            true,
        )
        .unwrap();

        assert!(today.exists());
        let content = fs::read_to_string(&today).unwrap();
        assert!(content.contains("date: 2026-04-06"));
        // No archive created since there was no prior note
        assert!(!archive_file.exists());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_date_parts() {
        assert_eq!(
            parse_date_parts("2026-04-05"),
            ("2026".into(), "04".into(), "05".into())
        );
        assert_eq!(
            parse_date_parts("2025-12-31"),
            ("2025".into(), "12".into(), "31".into())
        );
        assert_eq!(
            parse_date_parts("bad"),
            ("bad".into(), "01".into(), "01".into())
        );
    }

    #[test]
    fn test_expand_tilde() {
        let home = dirs::home_dir().unwrap();
        let expanded = expand_tilde(Path::new("~/foo/bar"));
        assert_eq!(expanded, home.join("foo/bar"));

        let absolute = expand_tilde(Path::new("/tmp/test"));
        assert_eq!(absolute, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_diary_idempotent_same_day() {
        let dir = temp_dir();
        let diary_dir = dir.join("diary");
        fs::create_dir_all(&diary_dir).unwrap();

        let today = diary_dir.join("today.md");
        let now = chrono::Local::now();
        let current_date = now.format("%Y-%m-%d").to_string();

        // Create a note dated today
        fs::write(&today, format!("---\ndate: {current_date}\n---\n# Today\n")).unwrap();

        // get_note_date should return today's date
        assert_eq!(get_note_date(&today), Some(current_date));

        fs::remove_dir_all(&dir).unwrap();
    }
}

//! `morg tags <name>` — tabulate every occurrence of a custom tag declared
//! in the `[tags]` config section, one column per named capture group (or
//! the single `kind` column).

use std::collections::BTreeMap;
use std::path::PathBuf;

use morg_parser::tag_table::TagTable;
use morg_parser::tags::TagKind;

use crate::collect::{self, TagContext};

pub fn run(
    name: &str,
    paths: &[PathBuf],
    json: bool,
    table: &TagTable,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(groups) = table.group_names(name) else {
        return Err(format!("tag '#{name}' is not declared in the [tags] config").into());
    };

    let parsed = collect::parse_files_with(paths, table);

    struct Occurrence {
        file: String,
        line: u32,
        fields: BTreeMap<String, String>,
        shape_mismatch: bool,
    }

    let mut occurrences: Vec<Occurrence> = Vec::new();
    for pf in &parsed {
        collect::walk_tags(&pf.path, &pf.document, |ctx: TagContext<'_>| {
            if let TagKind::Custom {
                name: tag_name,
                fields,
                shape_mismatch,
                ..
            } = &ctx.tag.kind
                && tag_name == name
            {
                occurrences.push(Occurrence {
                    file: ctx.file.display().to_string(),
                    line: ctx.tag.span.line,
                    fields: fields
                        .iter()
                        .map(|f| (f.group.clone(), f.value.clone()))
                        .collect(),
                    shape_mismatch: *shape_mismatch,
                });
            }
        });
    }

    occurrences.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    if json {
        let items: Vec<serde_json::Value> = occurrences
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.file,
                    "line": o.line,
                    "fields": o.fields,
                    "shape_mismatch": o.shape_mismatch,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    if occurrences.is_empty() {
        println!("No occurrences of #{name} found.");
        return Ok(());
    }

    // Pipe table in the `morg columns` style: FILE, LINE, one column per group.
    let headers: Vec<String> = ["file", "line"]
        .into_iter()
        .map(String::from)
        .chain(groups.iter().cloned())
        .collect();

    let cell = |o: &Occurrence, i: usize| -> String {
        match i {
            0 => o.file.clone(),
            1 => o.line.to_string(),
            _ => o.fields.get(&groups[i - 2]).cloned().unwrap_or_else(|| {
                if o.shape_mismatch {
                    "!".to_string()
                } else {
                    "-".to_string()
                }
            }),
        }
    };

    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            occurrences
                .iter()
                .map(|o| cell(o, i).len())
                .max()
                .unwrap_or(0)
                .max(h.len())
                .max(4)
        })
        .collect();

    let header: String = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:width$}", h.to_uppercase(), width = widths[i]))
        .collect::<Vec<_>>()
        .join(" | ");
    println!("| {header} |");

    let separator: String = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("-|-");
    println!("|-{separator}-|");

    for o in &occurrences {
        let line: String = (0..headers.len())
            .map(|i| format!("{:width$}", cell(o, i), width = widths[i]))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("| {line} |");
    }

    let mismatches = occurrences.iter().filter(|o| o.shape_mismatch).count();
    if mismatches > 0 {
        println!(
            "\n{} occurrence(s), {mismatches} not matching the declared pattern (!).",
            occurrences.len()
        );
    } else {
        println!("\n{} occurrence(s).", occurrences.len());
    }

    Ok(())
}

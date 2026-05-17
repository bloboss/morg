use std::collections::HashMap;
use std::path::PathBuf;

use morg_parser::tags::{MealKind, TagKind, Timestamp};

use crate::collect::{self, TagContext};
use crate::report;

struct MealEntry {
    name: String,
    ingredients: Vec<String>,
    date: Option<Timestamp>,
    location: String,
}

struct MealRef {
    recipe_name: String,
    date: Option<Timestamp>,
    location: String,
}

pub fn run(paths: &[PathBuf], json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);

    let mut recipes: Vec<MealEntry> = Vec::new();
    let mut references: Vec<MealRef> = Vec::new();

    for pf in &parsed {
        collect::walk_tags(&pf.path, &pf.document, |ctx: TagContext<'_>| {
            if ctx.is_archived {
                return;
            }
            if let TagKind::Meal { kind, date } = &ctx.tag.kind {
                let loc = report::format_location(ctx.file, &ctx.tag.span);
                match kind {
                    MealKind::Recipe { name, ingredients } => {
                        recipes.push(MealEntry {
                            name: name.clone(),
                            ingredients: ingredients.clone(),
                            date: *date,
                            location: loc,
                        });
                    }
                    MealKind::Reference { name } => {
                        references.push(MealRef {
                            recipe_name: name.clone(),
                            date: *date,
                            location: loc,
                        });
                    }
                }
            }
        });
    }

    // Build a lookup from normalised recipe name → ingredients
    let recipe_index: HashMap<String, &[String]> = recipes
        .iter()
        .map(|r| (normalise_name(&r.name), r.ingredients.as_slice()))
        .collect();

    if json {
        return output_json(&recipes, &references, &recipe_index);
    }

    output_text(&recipes, &references, &recipe_index);
    Ok(())
}

fn output_text(
    recipes: &[MealEntry],
    references: &[MealRef],
    recipe_index: &HashMap<String, &[String]>,
) {
    // Recipes section
    if !recipes.is_empty() {
        println!("Recipes\n-------");
        for r in recipes {
            let date_str = r
                .date
                .map(|d| format!("  ({})", d))
                .unwrap_or_default();
            println!("  {}{}", r.name, date_str);
            for ing in &r.ingredients {
                println!("    - {ing}");
            }
            println!("    -- {}", r.location);
        }
    }

    // Meal plan: collect all dated entries (definitions + references)
    let mut plan: Vec<(Timestamp, String, String, Option<Vec<String>>)> = Vec::new();

    for r in recipes {
        if let Some(date) = r.date {
            plan.push((date, r.name.clone(), r.location.clone(), None));
        }
    }
    for rf in references {
        if let Some(date) = rf.date {
            let ingredients = recipe_index
                .get(&normalise_name(&rf.recipe_name))
                .map(|ings| ings.to_vec());
            plan.push((
                date,
                rf.recipe_name.clone(),
                rf.location.clone(),
                ingredients,
            ));
        }
    }

    // Unresolved references (no date)
    let unplanned: Vec<&MealRef> = references.iter().filter(|r| r.date.is_none()).collect();

    if !plan.is_empty() {
        plan.sort_by_key(|(ts, _, _, _)| *ts);
        println!("\nMeal Plan\n---------");
        for (date, name, loc, ingredients) in &plan {
            print!("  {date}  {name}");
            if let Some(ings) = ingredients {
                print!("  [{}]", ings.join(", "));
            }
            println!("  -- {loc}");
        }
    }

    if !unplanned.is_empty() {
        println!("\nUnscheduled References\n----------------------");
        for rf in unplanned {
            let resolved = if recipe_index.contains_key(&normalise_name(&rf.recipe_name)) {
                ""
            } else {
                " (recipe not found)"
            };
            println!(
                "  {}{}  -- {}",
                rf.recipe_name, resolved, rf.location
            );
        }
    }

    if recipes.is_empty() && references.is_empty() {
        println!("No meals found.");
    }
}

fn output_json(
    recipes: &[MealEntry],
    references: &[MealRef],
    recipe_index: &HashMap<String, &[String]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let recipes_json: Vec<serde_json::Value> = recipes
        .iter()
        .map(|r| {
            serde_json::json!({
                "type": "recipe",
                "name": r.name,
                "ingredients": r.ingredients,
                "date": r.date.map(|d| d.to_string()),
                "location": r.location,
            })
        })
        .collect();

    let refs_json: Vec<serde_json::Value> = references
        .iter()
        .map(|rf| {
            let ingredients = recipe_index
                .get(&normalise_name(&rf.recipe_name))
                .map(|ings| serde_json::json!(ings));
            serde_json::json!({
                "type": "reference",
                "name": rf.recipe_name,
                "date": rf.date.map(|d| d.to_string()),
                "ingredients": ingredients,
                "location": rf.location,
            })
        })
        .collect();

    let all: Vec<serde_json::Value> = recipes_json.into_iter().chain(refs_json).collect();
    println!("{}", serde_json::to_string(&all)?);
    Ok(())
}

fn normalise_name(name: &str) -> String {
    name.to_lowercase()
}

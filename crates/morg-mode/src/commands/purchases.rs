use std::path::PathBuf;

use morg_parser::ast::Heading;
use morg_parser::tags::{Money, TagKind};

use crate::collect::{self, TagContext};
use crate::report;

pub fn run(paths: &[PathBuf], json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);

    let mut purchases: Vec<PurchaseEntry> = Vec::new();

    for pf in &parsed {
        collect::walk_tags(&pf.path, &pf.document, |ctx: TagContext<'_>| {
            if ctx.is_archived {
                return;
            }
            if let TagKind::Purchase(p) = &ctx.tag.kind {
                purchases.push(PurchaseEntry {
                    item: p.item.clone(),
                    price_cents: p.price.map(|m| m.cents),
                    currency: p.price.and_then(|m| m.currency),
                    quantity: p.quantity,
                    total_cents: p.total_cents(),
                    category: p.category.clone(),
                    location: report::format_location(ctx.file, &ctx.tag.span),
                    heading: ctx.parent_heading.map(heading_text),
                });
            }
        });
    }

    // Sort by category (uncategorized last), then by item name.
    purchases.sort_by(|a, b| {
        let cat_key = |c: &Option<String>| match c {
            Some(s) => (0, s.to_lowercase()),
            None => (1, String::new()),
        };
        cat_key(&a.category)
            .cmp(&cat_key(&b.category))
            .then_with(|| a.item.to_lowercase().cmp(&b.item.to_lowercase()))
    });

    if json {
        let items: Vec<serde_json::Value> = purchases
            .iter()
            .map(|e| {
                let (file, lnum) = parse_location(&e.location);
                serde_json::json!({
                    "item": e.item,
                    "price": e.price_cents.map(|c| Money::format_with(e.currency, c)),
                    "quantity": e.quantity,
                    "total": e.total_cents.map(|c| Money::format_with(e.currency, c)),
                    "category": e.category,
                    "file": file,
                    "line": lnum,
                    "heading": e.heading,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    if purchases.is_empty() {
        println!("No purchases found.");
        return Ok(());
    }

    // A representative currency symbol for totals: the first one seen.
    let symbol = purchases.iter().find_map(|e| e.currency);

    println!("Purchases\n");

    let mut grand_total: u64 = 0;
    let mut any_priced = false;
    let mut idx = 0;
    while idx < purchases.len() {
        let category = &purchases[idx].category;
        let label = category.as_deref().unwrap_or("(uncategorized)");
        println!("{label}");

        let mut subtotal: u64 = 0;
        let mut subtotal_priced = false;
        while idx < purchases.len() && &purchases[idx].category == category {
            let entry = &purchases[idx];
            let qty = if entry.quantity == 1 {
                String::new()
            } else {
                format!(" x{}", entry.quantity)
            };
            let price = match entry.total_cents {
                Some(cents) => {
                    subtotal += cents;
                    subtotal_priced = true;
                    format!("  {}", Money::format_with(entry.currency.or(symbol), cents))
                }
                None => String::new(),
            };
            println!(
                "  {item}{qty}{price}  -- {loc}",
                item = entry.item,
                loc = entry.location,
            );
            idx += 1;
        }

        if subtotal_priced {
            grand_total += subtotal;
            any_priced = true;
            println!("  subtotal: {}", Money::format_with(symbol, subtotal));
        }
        println!();
    }

    let count = purchases.len();
    if any_priced {
        println!(
            "{count} purchase(s), total {}",
            Money::format_with(symbol, grand_total)
        );
    } else {
        println!("{count} purchase(s)");
    }

    Ok(())
}

struct PurchaseEntry {
    item: String,
    price_cents: Option<u64>,
    currency: Option<char>,
    quantity: u32,
    total_cents: Option<u64>,
    category: Option<String>,
    location: String,
    heading: Option<String>,
}

fn heading_text(h: &Heading) -> String {
    h.content.plain_text()
}

fn parse_location(loc: &str) -> (&str, u32) {
    if let Some((file, line)) = loc.rsplit_once(':') {
        (file, line.parse().unwrap_or(0))
    } else {
        (loc, 0)
    }
}

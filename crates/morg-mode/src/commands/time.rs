use std::collections::HashMap;
use std::path::PathBuf;

use chrono::NaiveDateTime;
use morg_parser::tags::{ClockValue, TagKind};

use crate::collect::{self, TagContext};
use crate::report;

pub fn run(paths: &[PathBuf], project: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);

    let mut clock_events: Vec<ClockEvent> = Vec::new();

    for pf in &parsed {
        collect::walk_tags(&pf.path, &pf.document, |ctx: TagContext<'_>| {
            let heading = ctx
                .parent_heading
                .map(heading_plain_text)
                .unwrap_or_default();

            // Filter by project if specified
            if let Some(proj) = project
                && !heading.to_lowercase().contains(&proj.to_lowercase())
            {
                return;
            }

            match &ctx.tag.kind {
                TagKind::ClockIn { datetime } => {
                    clock_events.push(ClockEvent {
                        kind: ClockEventKind::In(*datetime),
                        heading: heading.clone(),
                        location: report::format_location(ctx.file, &ctx.tag.span),
                    });
                }
                TagKind::ClockOut { datetime } => {
                    clock_events.push(ClockEvent {
                        kind: ClockEventKind::Out(*datetime),
                        heading: heading.clone(),
                        location: report::format_location(ctx.file, &ctx.tag.span),
                    });
                }
                TagKind::Clock(value) => {
                    let minutes = match value {
                        ClockValue::Range { start, end } => {
                            let dur = *end - *start;
                            dur.num_minutes().unsigned_abs()
                        }
                        ClockValue::Duration { minutes } => *minutes,
                    };
                    clock_events.push(ClockEvent {
                        kind: ClockEventKind::Duration(minutes),
                        heading: heading.clone(),
                        location: report::format_location(ctx.file, &ctx.tag.span),
                    });
                }
                _ => {}
            }
        });
    }

    if clock_events.is_empty() {
        println!("No time tracking entries found.");
        return Ok(());
    }

    // Pair clock-in/out events and compute durations
    let mut total_minutes: u64 = 0;
    let mut per_heading: HashMap<String, u64> = HashMap::new();
    let mut unpaired_ins: Vec<&ClockEvent> = Vec::new();

    let mut i = 0;
    while i < clock_events.len() {
        match &clock_events[i].kind {
            ClockEventKind::Duration(mins) => {
                total_minutes += mins;
                *per_heading
                    .entry(clock_events[i].heading.clone())
                    .or_default() += mins;
                i += 1;
            }
            ClockEventKind::In(start) => {
                // Look for the next Out event
                let mut found = false;
                for j in (i + 1)..clock_events.len() {
                    if let ClockEventKind::Out(end) = &clock_events[j].kind {
                        let mins = (*end - *start).num_minutes().unsigned_abs();
                        total_minutes += mins;
                        *per_heading
                            .entry(clock_events[i].heading.clone())
                            .or_default() += mins;
                        // Remove the paired out event by skipping it
                        i = j + 1;
                        found = true;
                        break;
                    }
                }
                if !found {
                    unpaired_ins.push(&clock_events[i]);
                    i += 1;
                }
            }
            ClockEventKind::Out(_) => {
                // Orphaned clock-out — warn and skip
                eprintln!(
                    "warning: unpaired clock-out at {}",
                    clock_events[i].location
                );
                i += 1;
            }
        }
    }

    for ev in &unpaired_ins {
        eprintln!("warning: unpaired clock-in at {}", ev.location);
    }

    // Print report
    println!("Time Report");
    println!("{}", "=".repeat(40));

    let mut headings: Vec<_> = per_heading.into_iter().collect();
    headings.sort_by_key(|a| std::cmp::Reverse(a.1));

    for (heading, mins) in &headings {
        let label = if heading.is_empty() {
            "(no heading)"
        } else {
            heading
        };
        println!("  {:<30} {}", label, report::format_duration_minutes(*mins));
    }

    println!("{}", "-".repeat(40));
    println!(
        "  {:<30} {}",
        "TOTAL",
        report::format_duration_minutes(total_minutes)
    );

    Ok(())
}

struct ClockEvent {
    kind: ClockEventKind,
    heading: String,
    location: String,
}

enum ClockEventKind {
    In(NaiveDateTime),
    Out(NaiveDateTime),
    Duration(u64),
}

fn heading_plain_text(h: &morg_parser::ast::Heading) -> String {
    h.content.plain_text()
}

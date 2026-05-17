use chrono::{NaiveDate, NaiveDateTime};

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub kind: TagKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TagKind {
    Todo {
        text: Option<String>,
    },
    Done {
        text: Option<String>,
    },
    Deadline {
        date: Timestamp,
        repeater: Option<Repeater>,
        warning: Option<u32>,
    },
    Scheduled {
        date: Timestamp,
        repeater: Option<Repeater>,
        warning: Option<u32>,
    },
    Date {
        date: Timestamp,
        repeater: Option<Repeater>,
    },
    Event {
        date: Timestamp,
        repeater: Option<Repeater>,
        description: Option<String>,
    },
    ClockIn {
        datetime: NaiveDateTime,
    },
    ClockOut {
        datetime: NaiveDateTime,
    },
    Clock(ClockValue),
    Tangle,
    Priority {
        level: PriorityLevel,
    },
    Effort {
        minutes: u64,
    },
    Closed {
        datetime: NaiveDateTime,
    },
    Archive,
    Progress,
    /// A meal definition or reference for meal planning.
    Meal {
        kind: MealKind,
        date: Option<Timestamp>,
    },
    /// A custom TODO workflow state defined in frontmatter.
    CustomState {
        name: String,
        is_done: bool,
        text: Option<String>,
    },
    Unknown {
        name: String,
        value: Option<String>,
    },
}

/// A timestamp that may be date-only or date+time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Timestamp {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

impl Timestamp {
    pub fn date(&self) -> NaiveDate {
        match self {
            Self::Date(d) => *d,
            Self::DateTime(dt) => dt.date(),
        }
    }

    pub fn has_time(&self) -> bool {
        matches!(self, Self::DateTime(_))
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Date(d) => write!(f, "{d}"),
            Self::DateTime(dt) => write!(f, "{}", dt.format("%Y-%m-%d %H:%M")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityLevel {
    A,
    B,
    C,
    Custom(char),
}

impl std::fmt::Display for PriorityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
            Self::C => write!(f, "C"),
            Self::Custom(c) => write!(f, "{c}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    pub interval: u32,
    pub unit: RepeaterUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeaterUnit {
    Day,
    Week,
    Month,
    Year,
}

impl Repeater {
    pub fn as_rrule(&self) -> String {
        let freq = match self.unit {
            RepeaterUnit::Day => "DAILY",
            RepeaterUnit::Week => "WEEKLY",
            RepeaterUnit::Month => "MONTHLY",
            RepeaterUnit::Year => "YEARLY",
        };
        format!("FREQ={freq};INTERVAL={}", self.interval)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClockValue {
    Range {
        start: NaiveDateTime,
        end: NaiveDateTime,
    },
    Duration {
        minutes: u64,
    },
}

/// A meal tag — either a recipe definition or a reference to a named recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum MealKind {
    /// `#meal Name [date] | ingredient1, ingredient2, ...`
    Recipe {
        name: String,
        ingredients: Vec<String>,
    },
    /// `#meal(name) [date]` — references a named recipe.
    Reference { name: String },
}

/// Parse a tag from its name and optional argument.
///
/// Keyword resolution is driven by [`Keyword::from_str`] from `tokens.rs`,
/// which is the single source of truth for all known tag names.
pub fn parse_tag(name: &str, arg: Option<&str>, span: Span) -> Tag {
    use crate::tokens::Keyword;

    let kind = match Keyword::from_str(name) {
        Some(Keyword::Todo) => TagKind::Todo {
            text: non_empty(arg),
        },
        Some(Keyword::Done) => TagKind::Done {
            text: non_empty(arg),
        },
        Some(Keyword::Deadline) => match parse_timestamp_full(arg) {
            Some((date, repeater, warning)) => TagKind::Deadline {
                date,
                repeater,
                warning,
            },
            None => unknown(name, arg),
        },
        Some(Keyword::Scheduled) => match parse_timestamp_full(arg) {
            Some((date, repeater, warning)) => TagKind::Scheduled {
                date,
                repeater,
                warning,
            },
            None => unknown(name, arg),
        },
        Some(Keyword::Date) => match parse_timestamp_full(arg) {
            Some((date, repeater, _)) => TagKind::Date { date, repeater },
            None => unknown(name, arg),
        },
        Some(Keyword::Event) => match parse_event(arg) {
            Some((date, repeater, description)) => TagKind::Event {
                date,
                repeater,
                description,
            },
            None => unknown(name, arg),
        },
        Some(Keyword::ClockIn) => match parse_datetime(arg) {
            Some(datetime) => TagKind::ClockIn { datetime },
            None => unknown(name, arg),
        },
        Some(Keyword::ClockOut) => match parse_datetime(arg) {
            Some(datetime) => TagKind::ClockOut { datetime },
            None => unknown(name, arg),
        },
        Some(Keyword::Clock) => match parse_clock(arg) {
            Some(value) => TagKind::Clock(value),
            None => unknown(name, arg),
        },
        Some(Keyword::Tangle) => TagKind::Tangle,
        Some(Keyword::Priority) => match parse_priority(arg) {
            Some(level) => TagKind::Priority { level },
            None => unknown(name, arg),
        },
        Some(Keyword::Effort) => match arg.and_then(|a| parse_duration(a.trim())) {
            Some(minutes) => TagKind::Effort { minutes },
            None => unknown(name, arg),
        },
        Some(Keyword::Closed) => match parse_datetime(arg) {
            Some(datetime) => TagKind::Closed { datetime },
            None => unknown(name, arg),
        },
        Some(Keyword::Archive) => TagKind::Archive,
        Some(Keyword::Progress) => TagKind::Progress,
        Some(Keyword::Meal) => match parse_meal(arg) {
            Some((kind, date)) => TagKind::Meal { kind, date },
            None => unknown(name, arg),
        },
        // Properties/End are structural, not inline tags — treat as unknown if used as tags
        Some(Keyword::Properties) | Some(Keyword::End) => unknown(name, arg),
        None => unknown(name, arg),
    };

    Tag { kind, span }
}

/// Parse a meal tag argument into a `(MealKind, Option<Timestamp>)`.
///
/// Reference form:  `(name) [date]`  e.g. `(chili)` or `(chili) 2026-05-20`
/// Definition form: `Name [date] | ingredient1, ingredient2, ...`
fn parse_meal(arg: Option<&str>) -> Option<(MealKind, Option<Timestamp>)> {
    let s = arg?.trim();
    if s.is_empty() {
        return None;
    }

    // Reference form: starts with '('
    if let Some(rest) = s.strip_prefix('(') {
        let close = rest.find(')')?;
        let name = rest[..close].trim().to_string();
        if name.is_empty() {
            return None;
        }
        let after = rest[close + 1..].trim();
        let date = if after.len() >= 10 {
            parse_timestamp_full(Some(after)).map(|(ts, _, _)| ts)
        } else {
            None
        };
        return Some((MealKind::Reference { name }, date));
    }

    // Definition form: "Name [date] | ingredients"
    let (head, ingredients_str) = s.split_once('|')?;
    let head = head.trim();
    let ingredients: Vec<String> = ingredients_str
        .split(',')
        .map(|i| i.trim().to_string())
        .filter(|i| !i.is_empty())
        .collect();
    if ingredients.is_empty() {
        return None;
    }

    // head is "Name" or "Name date"
    // Try to find a date by checking if the last token looks like one
    let (name, date) = parse_name_and_date(head);
    if name.is_empty() {
        return None;
    }

    Some((MealKind::Recipe { name, ingredients }, date))
}

/// Split a string like `"Pasta Primavera 2026-05-20"` into `("Pasta Primavera", Some(date))`.
/// The date, if present, must be the last space-separated token in ISO format.
fn parse_name_and_date(s: &str) -> (String, Option<Timestamp>) {
    if let Some(idx) = s.rfind(' ') {
        let candidate = &s[idx + 1..];
        if candidate.len() >= 10 {
            if let Some((ts, _, _)) = parse_timestamp_full(Some(candidate)) {
                let name = s[..idx].trim().to_string();
                return (name, Some(ts));
            }
        }
    }
    (s.to_string(), None)
}

fn unknown(name: &str, arg: Option<&str>) -> TagKind {
    TagKind::Unknown {
        name: name.to_string(),
        value: non_empty(arg),
    }
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// Parse a timestamp with optional time, repeater, and warning period.
/// Formats: "2026-04-03", "2026-04-03T14:00", "2026-04-03 +1w", "2026-04-03T14:00 +1w -3d"
fn parse_timestamp_full(arg: Option<&str>) -> Option<(Timestamp, Option<Repeater>, Option<u32>)> {
    let s = arg?.trim();
    if s.len() < 10 {
        return None;
    }

    // Try datetime first (YYYY-MM-DDTHH:MM or YYYY-MM-DDTHH:MM:SS)
    let (ts, rest) = if s.len() >= 16 && s.as_bytes()[10] == b'T' {
        // Try HH:MM:SS (19 chars) then HH:MM (16 chars)
        if s.len() >= 19 {
            if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%dT%H:%M:%S") {
                (Timestamp::DateTime(dt), s[19..].trim())
            } else if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..16], "%Y-%m-%dT%H:%M") {
                (Timestamp::DateTime(dt), s[16..].trim())
            } else {
                return None;
            }
        } else if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..16], "%Y-%m-%dT%H:%M") {
            (Timestamp::DateTime(dt), s[16..].trim())
        } else {
            return None;
        }
    } else {
        let date = NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()?;
        (Timestamp::Date(date), s[10..].trim())
    };

    // Parse optional repeater (+Nunit) and warning (-Nd) from rest
    let mut repeater = None;
    let mut warning = None;
    let mut remaining = rest;

    // Repeater: +Nunit
    if remaining.starts_with('+') {
        let after_plus = &remaining[1..];
        let num_end = after_plus
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_plus.len());
        if num_end > 0 {
            let unit_end = 1 + num_end + 1; // +digits+unit
            if unit_end <= remaining.len() {
                repeater = parse_repeater(&remaining[..unit_end]);
                remaining = remaining[unit_end..].trim();
            }
        }
    }

    // Warning: -Nd
    if let Some(after_minus) = remaining.strip_prefix('-') {
        let num_end = after_minus
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_minus.len());
        if num_end > 0
            && let Ok(days) = after_minus[..num_end].parse::<u32>()
        {
            // Verify the unit is 'd'
            if after_minus.len() > num_end {
                let unit = after_minus.as_bytes()[num_end];
                if unit == b'd' || unit == b'D' {
                    warning = Some(days);
                }
            }
        }
    }

    Some((ts, repeater, warning))
}

fn parse_repeater(s: &str) -> Option<Repeater> {
    let s = s.trim();
    let s = s.strip_prefix('+')?;
    if s.is_empty() {
        return None;
    }

    let num_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if num_end == 0 {
        return None;
    }
    let interval: u32 = s[..num_end].parse().ok()?;
    let unit_char = s[num_end..].chars().next()?;

    let unit = match unit_char {
        'd' | 'D' => RepeaterUnit::Day,
        'w' | 'W' => RepeaterUnit::Week,
        'm' | 'M' => RepeaterUnit::Month,
        'y' | 'Y' => RepeaterUnit::Year,
        _ => return None,
    };

    Some(Repeater { interval, unit })
}

fn parse_event(arg: Option<&str>) -> Option<(Timestamp, Option<Repeater>, Option<String>)> {
    let s = arg?.trim();
    if s.len() < 10 {
        return None;
    }

    // Parse timestamp (date or datetime)
    let (ts, after_ts) = if s.len() >= 16 && s.as_bytes()[10] == b'T' {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..16], "%Y-%m-%dT%H:%M") {
            (Timestamp::DateTime(dt), s[16..].trim())
        } else {
            let date = NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()?;
            (Timestamp::Date(date), s[10..].trim())
        }
    } else {
        let date = NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()?;
        (Timestamp::Date(date), s[10..].trim())
    };

    // Check for repeater before description
    let (repeater, description_part) = if let Some(after_plus) = after_ts.strip_prefix('+') {
        let num_end = after_plus
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_plus.len());
        let repeater_end = 1 + num_end + 1;
        if repeater_end <= after_ts.len() {
            let rep = parse_repeater(&after_ts[..repeater_end]);
            let desc = after_ts[repeater_end..].trim();
            (rep, desc)
        } else {
            (parse_repeater(after_ts), "")
        }
    } else {
        (None, after_ts)
    };

    let description = if description_part.is_empty() {
        None
    } else {
        Some(description_part.to_string())
    };
    Some((ts, repeater, description))
}

fn parse_datetime(arg: Option<&str>) -> Option<NaiveDateTime> {
    let s = arg?.trim();
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
}

fn parse_clock(arg: Option<&str>) -> Option<ClockValue> {
    let s = arg?.trim();

    // Try range format: 2026-04-03T09:00/2026-04-03T10:30
    if let Some((start_str, end_str)) = s.split_once('/') {
        let start = NaiveDateTime::parse_from_str(start_str.trim(), "%Y-%m-%dT%H:%M")
            .or_else(|_| NaiveDateTime::parse_from_str(start_str.trim(), "%Y-%m-%dT%H:%M:%S"))
            .ok()?;
        let end = NaiveDateTime::parse_from_str(end_str.trim(), "%Y-%m-%dT%H:%M")
            .or_else(|_| NaiveDateTime::parse_from_str(end_str.trim(), "%Y-%m-%dT%H:%M:%S"))
            .ok()?;
        return Some(ClockValue::Range { start, end });
    }

    // Try duration format: 1h30m, 2h, 45m
    parse_duration(s).map(|minutes| ClockValue::Duration { minutes })
}

fn parse_priority(arg: Option<&str>) -> Option<PriorityLevel> {
    let s = arg?.trim();
    match s {
        "A" | "a" => Some(PriorityLevel::A),
        "B" | "b" => Some(PriorityLevel::B),
        "C" | "c" => Some(PriorityLevel::C),
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            if c.is_alphanumeric() {
                Some(PriorityLevel::Custom(c.to_ascii_uppercase()))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn parse_duration(s: &str) -> Option<u64> {
    let mut total_minutes: u64 = 0;
    let mut current_num = String::new();
    let mut found_unit = false;

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else if c == 'h' || c == 'H' {
            let hours: u64 = current_num.parse().ok()?;
            total_minutes += hours * 60;
            current_num.clear();
            found_unit = true;
        } else if c == 'm' || c == 'M' {
            let mins: u64 = current_num.parse().ok()?;
            total_minutes += mins;
            current_num.clear();
            found_unit = true;
        } else {
            return None;
        }
    }

    if found_unit && current_num.is_empty() {
        Some(total_minutes)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todo() {
        let tag = parse_tag("todo", Some("fix the bug"), Span::empty(1, 1));
        assert!(matches!(tag.kind, TagKind::Todo { text: Some(ref t) } if t == "fix the bug"));
    }

    #[test]
    fn test_parse_deadline() {
        let tag = parse_tag("deadline", Some("2026-04-10"), Span::empty(1, 1));
        assert!(
            matches!(tag.kind, TagKind::Deadline { date: Timestamp::Date(d), repeater: None, warning: None } if d == NaiveDate::from_ymd_opt(2026, 4, 10).unwrap())
        );
    }

    #[test]
    fn test_parse_deadline_with_repeater() {
        let tag = parse_tag("deadline", Some("2026-04-10 +1w"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Deadline {
                date: Timestamp::Date(d),
                repeater: Some(Repeater { interval: 1, unit: RepeaterUnit::Week }),
                warning: None,
            } if d == NaiveDate::from_ymd_opt(2026, 4, 10).unwrap()
        ));
    }

    #[test]
    fn test_parse_deadline_with_time() {
        let tag = parse_tag("deadline", Some("2026-04-10T14:00"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Deadline {
                date: Timestamp::DateTime(_),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_deadline_with_warning() {
        let tag = parse_tag("deadline", Some("2026-04-10 -3d"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Deadline {
                warning: Some(3),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_deadline_full() {
        let tag = parse_tag(
            "deadline",
            Some("2026-04-10T14:00 +1w -3d"),
            Span::empty(1, 1),
        );
        assert!(matches!(
            tag.kind,
            TagKind::Deadline {
                date: Timestamp::DateTime(_),
                repeater: Some(Repeater {
                    interval: 1,
                    unit: RepeaterUnit::Week
                }),
                warning: Some(3),
            }
        ));
    }

    #[test]
    fn test_parse_scheduled() {
        let tag = parse_tag("scheduled", Some("2026-04-05"), Span::empty(1, 1));
        assert!(
            matches!(tag.kind, TagKind::Scheduled { date: Timestamp::Date(d), repeater: None, warning: None } if d == NaiveDate::from_ymd_opt(2026, 4, 5).unwrap())
        );
    }

    #[test]
    fn test_parse_scheduled_with_repeater() {
        let tag = parse_tag("scheduled", Some("2026-04-05 +2m"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Scheduled {
                date: Timestamp::Date(d),
                repeater: Some(Repeater { interval: 2, unit: RepeaterUnit::Month }),
                warning: None,
            } if d == NaiveDate::from_ymd_opt(2026, 4, 5).unwrap()
        ));
    }

    #[test]
    fn test_parse_date_with_repeater() {
        let tag = parse_tag("date", Some("2026-01-01 +1y"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Date {
                repeater: Some(Repeater {
                    interval: 1,
                    unit: RepeaterUnit::Year
                }),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_event_with_repeater() {
        let tag = parse_tag("event", Some("2026-01-01 +1y New Year"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Event {
                date: Timestamp::Date(d),
                repeater: Some(Repeater { interval: 1, unit: RepeaterUnit::Year }),
                description: Some(ref desc),
            } if d == NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() && desc == "New Year"
        ));
    }

    #[test]
    fn test_parse_clock_range() {
        let tag = parse_tag(
            "clock",
            Some("2026-04-03T09:00/2026-04-03T10:30"),
            Span::empty(1, 1),
        );
        assert!(matches!(tag.kind, TagKind::Clock(ClockValue::Range { .. })));
    }

    #[test]
    fn test_parse_clock_duration() {
        let tag = parse_tag("clock", Some("1h30m"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Clock(ClockValue::Duration { minutes: 90 })
        ));
    }

    #[test]
    fn test_parse_event() {
        let tag = parse_tag("event", Some("2026-04-10 Team meeting"), Span::empty(1, 1));
        assert!(
            matches!(tag.kind, TagKind::Event { date: Timestamp::Date(d), description: Some(ref desc), .. } if d == NaiveDate::from_ymd_opt(2026, 4, 10).unwrap() && desc == "Team meeting")
        );
    }

    #[test]
    fn test_unknown_tag() {
        let tag = parse_tag("custom", Some("value"), Span::empty(1, 1));
        assert!(
            matches!(tag.kind, TagKind::Unknown { ref name, ref value } if name == "custom" && value.as_deref() == Some("value"))
        );
    }

    #[test]
    fn test_parse_meal_recipe() {
        let tag = parse_tag(
            "meal",
            Some("Chili | beef, beans, tomatoes"),
            Span::empty(1, 1),
        );
        assert!(
            matches!(&tag.kind, TagKind::Meal { kind: MealKind::Recipe { name, ingredients }, date: None }
                if name == "Chili" && ingredients == &["beef", "beans", "tomatoes"])
        );
    }

    #[test]
    fn test_parse_meal_recipe_with_date() {
        let tag = parse_tag(
            "meal",
            Some("Pasta 2026-05-20 | pasta, garlic, tomatoes"),
            Span::empty(1, 1),
        );
        assert!(
            matches!(&tag.kind, TagKind::Meal { kind: MealKind::Recipe { name, .. }, date: Some(_) }
                if name == "Pasta")
        );
    }

    #[test]
    fn test_parse_meal_reference() {
        let tag = parse_tag("meal", Some("(chili)"), Span::empty(1, 1));
        assert!(
            matches!(&tag.kind, TagKind::Meal { kind: MealKind::Reference { name }, date: None }
                if name == "chili")
        );
    }

    #[test]
    fn test_parse_meal_reference_with_date() {
        let tag = parse_tag("meal", Some("(chili) 2026-05-20"), Span::empty(1, 1));
        assert!(
            matches!(&tag.kind, TagKind::Meal { kind: MealKind::Reference { name }, date: Some(_) }
                if name == "chili")
        );
    }

    #[test]
    fn test_parse_meal_no_ingredients_is_unknown() {
        // Missing | separator should fall back to Unknown
        let tag = parse_tag("meal", Some("Chili no ingredients here"), Span::empty(1, 1));
        assert!(matches!(tag.kind, TagKind::Unknown { .. }));
    }

    #[test]
    fn test_bad_deadline_falls_back() {
        let tag = parse_tag("deadline", Some("not-a-date"), Span::empty(1, 1));
        assert!(matches!(tag.kind, TagKind::Unknown { ref name, .. } if name == "deadline"));
    }

    #[test]
    fn test_duration_parsing() {
        assert_eq!(parse_duration("2h"), Some(120));
        assert_eq!(parse_duration("45m"), Some(45));
        assert_eq!(parse_duration("1h30m"), Some(90));
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn test_repeater_parsing() {
        assert_eq!(
            parse_repeater("+1d"),
            Some(Repeater {
                interval: 1,
                unit: RepeaterUnit::Day
            })
        );
        assert_eq!(
            parse_repeater("+2w"),
            Some(Repeater {
                interval: 2,
                unit: RepeaterUnit::Week
            })
        );
        assert_eq!(
            parse_repeater("+3m"),
            Some(Repeater {
                interval: 3,
                unit: RepeaterUnit::Month
            })
        );
        assert_eq!(
            parse_repeater("+1y"),
            Some(Repeater {
                interval: 1,
                unit: RepeaterUnit::Year
            })
        );
        assert_eq!(
            parse_repeater("+0d"),
            Some(Repeater {
                interval: 0,
                unit: RepeaterUnit::Day
            })
        );
        assert_eq!(parse_repeater("nope"), None);
        assert_eq!(parse_repeater("+"), None);
        assert_eq!(parse_repeater("+x"), None);
    }

    #[test]
    fn test_rrule_generation() {
        let r = Repeater {
            interval: 1,
            unit: RepeaterUnit::Week,
        };
        assert_eq!(r.as_rrule(), "FREQ=WEEKLY;INTERVAL=1");
        let r = Repeater {
            interval: 3,
            unit: RepeaterUnit::Month,
        };
        assert_eq!(r.as_rrule(), "FREQ=MONTHLY;INTERVAL=3");
    }
}

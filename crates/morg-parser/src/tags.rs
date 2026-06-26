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
    /// A media item (book, movie, album, …) for "to read/watch/listen" lists.
    Media {
        kind: MediaKind,
        title: Option<String>,
        creator: Option<String>,
        status: MediaStatus,
        rating: Option<u8>,
        year: Option<i32>,
    },
    /// A tracked purchase, e.g. `#purchase USB-C cable price=12.99 category=cables`.
    Purchase(PurchaseValue),
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

/// The medium of a `#media` item. Determines which aggregated list it lands on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaKind {
    Book,
    Article,
    Comic,
    Manga,
    Movie,
    Show,
    Anime,
    Album,
    Podcast,
    Song,
    Game,
    /// Any unrecognized medium, preserving the word the user wrote.
    Other(String),
}

impl MediaKind {
    /// Parse a medium word. Unknown words become [`MediaKind::Other`].
    pub fn parse(s: &str) -> MediaKind {
        match s.trim().to_ascii_lowercase().as_str() {
            "book" | "books" | "novel" | "ebook" | "audiobook" => MediaKind::Book,
            "article" | "paper" | "essay" | "blog" | "post" => MediaKind::Article,
            "comic" | "comics" | "graphicnovel" => MediaKind::Comic,
            "manga" => MediaKind::Manga,
            "movie" | "film" => MediaKind::Movie,
            "show" | "tv" | "series" => MediaKind::Show,
            "anime" => MediaKind::Anime,
            "album" | "music" | "record" => MediaKind::Album,
            "podcast" => MediaKind::Podcast,
            "song" | "track" | "single" => MediaKind::Song,
            "game" | "videogame" => MediaKind::Game,
            other => MediaKind::Other(other.to_string()),
        }
    }

    /// The canonical word for this medium.
    pub fn as_str(&self) -> &str {
        match self {
            MediaKind::Book => "book",
            MediaKind::Article => "article",
            MediaKind::Comic => "comic",
            MediaKind::Manga => "manga",
            MediaKind::Movie => "movie",
            MediaKind::Show => "show",
            MediaKind::Anime => "anime",
            MediaKind::Album => "album",
            MediaKind::Podcast => "podcast",
            MediaKind::Song => "song",
            MediaKind::Game => "game",
            MediaKind::Other(s) => s,
        }
    }

    /// Which aggregated list this medium belongs to.
    pub fn category(&self) -> MediaCategory {
        match self {
            MediaKind::Book | MediaKind::Article | MediaKind::Comic | MediaKind::Manga => {
                MediaCategory::Read
            }
            MediaKind::Movie | MediaKind::Show | MediaKind::Anime => MediaCategory::Watch,
            MediaKind::Album | MediaKind::Podcast | MediaKind::Song => MediaCategory::Listen,
            MediaKind::Game => MediaCategory::Play,
            MediaKind::Other(_) => MediaCategory::Other,
        }
    }
}

impl std::fmt::Display for MediaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The aggregated list a media item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaCategory {
    Read,
    Watch,
    Listen,
    Play,
    Other,
}

impl MediaCategory {
    /// Human-readable heading for this list, e.g. "To Read".
    pub fn label(&self) -> &'static str {
        match self {
            MediaCategory::Read => "To Read",
            MediaCategory::Watch => "To Watch",
            MediaCategory::Listen => "To Listen",
            MediaCategory::Play => "To Play",
            MediaCategory::Other => "Other",
        }
    }

    /// Categories in display order.
    pub fn all() -> &'static [MediaCategory] {
        &[
            MediaCategory::Read,
            MediaCategory::Watch,
            MediaCategory::Listen,
            MediaCategory::Play,
            MediaCategory::Other,
        ]
    }
}

/// Where a media item sits in the consume-it lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaStatus {
    /// Want to consume it (the default): on the "to read/watch/listen" list.
    Todo,
    /// Currently consuming it.
    Active,
    /// Finished with it.
    Done,
}

impl MediaStatus {
    /// Parse a status word, accepting medium-specific synonyms. Returns `None`
    /// for unrecognized words so callers can fall back to the default.
    pub fn parse(s: &str) -> Option<MediaStatus> {
        let normalized: String = s
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .collect();
        match normalized.as_str() {
            "todo" | "want" | "queued" | "queue" | "backlog" | "planned" | "wishlist"
            | "toread" | "towatch" | "tolisten" | "toplay" | "unread" => Some(MediaStatus::Todo),
            "doing" | "active" | "inprogress" | "current" | "reading" | "watching"
            | "listening" | "playing" => Some(MediaStatus::Active),
            "done" | "read" | "watched" | "listened" | "played" | "finished" | "complete"
            | "completed" => Some(MediaStatus::Done),
            _ => None,
        }
    }

    /// The canonical word for this status.
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaStatus::Todo => "todo",
            MediaStatus::Active => "active",
            MediaStatus::Done => "done",
        }
    }
}

impl std::fmt::Display for MediaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
/// Structured value for a `#purchase` tag.
///
/// The free-text item name is required; price, category, and quantity are
/// optional `key=value` attributes that may appear in any order:
/// `#purchase USB-C cable price=12.99 category=cables qty=2`.
#[derive(Debug, Clone, PartialEq)]
pub struct PurchaseValue {
    /// What was (or is to be) purchased.
    pub item: String,
    /// Unit price, if given.
    pub price: Option<Money>,
    /// Free-form category for grouping (e.g. `books`, `cables`).
    pub category: Option<String>,
    /// Number of units. Defaults to 1.
    pub quantity: u32,
}

impl PurchaseValue {
    /// Total cost in cents (`price * quantity`), if a price was given.
    pub fn total_cents(&self) -> Option<u64> {
        self.price.map(|p| p.cents * self.quantity as u64)
    }
}

/// A monetary amount stored as integer cents to avoid floating-point drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    /// Optional currency symbol (`$`, `£`, `€`).
    pub currency: Option<char>,
    /// Amount in the smallest unit (cents).
    pub cents: u64,
}

impl Money {
    /// Format `cents` as a decimal amount, prefixed with `symbol` when given.
    pub fn format_with(symbol: Option<char>, cents: u64) -> String {
        match symbol {
            Some(c) => format!("{c}{}.{:02}", cents / 100, cents % 100),
            None => format!("{}.{:02}", cents / 100, cents % 100),
        }
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&Money::format_with(self.currency, self.cents))
    }
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
        Some(Keyword::Media) => match parse_media(arg) {
            Some(kind) => kind,
            None => unknown(name, arg),
        },
        Some(Keyword::Purchase) => match parse_purchase(arg) {
            Some(value) => TagKind::Purchase(value),
            None => unknown(name, arg),
        },
        // Properties/End are structural, not inline tags — treat as unknown if used as tags
        Some(Keyword::Properties) | Some(Keyword::End) => unknown(name, arg),
        None => unknown(name, arg),
    };

    Tag { kind, span }
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

/// Parse a `#media` argument into a structured media item.
///
/// Syntax: `<kind> <title…> [key=value …]`
///   - The first bare word is the medium (`book`, `movie`, `album`, …).
///   - Remaining bare words form the title. Wrap multi-word values in
///     double quotes to keep them together (`director="Denis Villeneuve"`).
///   - Recognized attributes: `by`/`author`/`director`/`artist`/`creator`,
///     `status`/`state`, `rating`/`score`, `year`.
///
/// Returns `None` when there is no medium word, so the caller falls back to
/// treating it as an unknown tag.
fn parse_media(arg: Option<&str>) -> Option<TagKind> {
    let s = arg?.trim();
    if s.is_empty() {
        return None;
    }

    let mut kind: Option<MediaKind> = None;
    let mut title_parts: Vec<String> = Vec::new();
    let mut creator: Option<String> = None;
    let mut status: Option<MediaStatus> = None;
    let mut rating: Option<u8> = None;
    let mut year: Option<i32> = None;

    for token in split_media_args(s) {
        if let Some((key, value)) = token.split_once('=') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.trim().to_ascii_lowercase().as_str() {
                "by" | "author" | "director" | "artist" | "creator" => {
                    creator = Some(value.to_string());
                }
                "status" | "state" => {
                    status = MediaStatus::parse(value);
                }
                "rating" | "score" => {
                    rating = value.parse().ok();
                }
                "year" => {
                    year = value.parse().ok();
                }
                // Unknown attributes are ignored rather than polluting the title.
                _ => {}
            }
        } else if kind.is_none() {
            kind = Some(MediaKind::parse(&token));
        } else {
            title_parts.push(token);
        }
    }

    let kind = kind?;
    let title = if title_parts.is_empty() {
        None
    } else {
        Some(title_parts.join(" "))
    };

    Some(TagKind::Media {
        kind,
        title,
        creator,
        status: status.unwrap_or(MediaStatus::Todo),
        rating,
        year,
    })
}

/// Split a media argument into whitespace-separated tokens, keeping
/// double-quoted spans (and `key="quoted value"` attributes) intact.
fn split_media_args(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_content = false;

    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_content = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_content {
                    tokens.push(std::mem::take(&mut current));
                    has_content = false;
                }
            }
            c => {
                current.push(c);
                has_content = true;
            }
        }
    }
    if has_content {
        tokens.push(current);
    }
    tokens
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

/// Parse a `#purchase` argument into structured fields.
///
/// Tokens of the form `key=value` with a recognized key (`price`/`cost`,
/// `category`/`cat`, `qty`/`quantity`/`count`) are consumed as attributes;
/// everything else joins to form the item name. Returns `None` if no item
/// name remains.
fn parse_purchase(arg: Option<&str>) -> Option<PurchaseValue> {
    let s = arg?.trim();
    if s.is_empty() {
        return None;
    }

    let mut item_parts: Vec<&str> = Vec::new();
    let mut price = None;
    let mut category = None;
    let mut quantity: u32 = 1;

    for token in s.split_whitespace() {
        if let Some((key, value)) = token.split_once('=')
            && !value.is_empty()
        {
            match key.to_ascii_lowercase().as_str() {
                "price" | "cost" => {
                    if let Some(money) = parse_money(value) {
                        price = Some(money);
                        continue;
                    }
                }
                "category" | "cat" => {
                    category = Some(value.to_string());
                    continue;
                }
                "qty" | "quantity" | "count" => {
                    if let Ok(q) = value.parse::<u32>()
                        && q > 0
                    {
                        quantity = q;
                        continue;
                    }
                }
                _ => {}
            }
        }
        item_parts.push(token);
    }

    let item = item_parts.join(" ");
    if item.is_empty() {
        return None;
    }

    Some(PurchaseValue {
        item,
        price,
        category,
        quantity,
    })
}

/// Parse a monetary amount with an optional leading currency symbol.
/// Accepts `12.99`, `$12.99`, `£8`, `40`, `9.5`.
fn parse_money(s: &str) -> Option<Money> {
    let s = s.trim();
    let (currency, rest) = match s.chars().next() {
        Some(c @ ('$' | '£' | '€')) => (Some(c), &s[c.len_utf8()..]),
        _ => (None, s),
    };
    let cents = parse_cents(rest.trim())?;
    Some(Money { currency, cents })
}

/// Parse a decimal amount (up to two fractional digits) into integer cents.
fn parse_cents(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    match s.split_once('.') {
        Some((whole, frac)) => {
            if frac.is_empty() || frac.len() > 2 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let whole: u64 = if whole.is_empty() {
                0
            } else {
                whole.parse().ok()?
            };
            let frac_value: u64 = frac.parse().ok()?;
            let cents = if frac.len() == 1 {
                frac_value * 10
            } else {
                frac_value
            };
            Some(whole * 100 + cents)
        }
        None => {
            let whole: u64 = s.parse().ok()?;
            Some(whole * 100)
        }
    }
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
    fn test_parse_media_book() {
        let tag = parse_tag("media", Some("book The Hobbit"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Media {
                kind: MediaKind::Book,
                ref title,
                status: MediaStatus::Todo,
                ..
            } if title.as_deref() == Some("The Hobbit")
        ));
    }

    #[test]
    fn test_parse_media_category() {
        assert_eq!(MediaKind::Book.category(), MediaCategory::Read);
        assert_eq!(MediaKind::Movie.category(), MediaCategory::Watch);
        assert_eq!(MediaKind::Album.category(), MediaCategory::Listen);
        assert_eq!(MediaKind::Game.category(), MediaCategory::Play);
        assert_eq!(
            MediaKind::parse("boardgame").category(),
            MediaCategory::Other
        );
    }

    #[test]
    fn test_parse_media_full() {
        let tag = parse_tag(
            "media",
            Some(
                r#"movie "Blade Runner 2049" director="Denis Villeneuve" status=watched rating=9 year=2017"#,
            ),
            Span::empty(1, 1),
        );
        assert!(matches!(
            tag.kind,
            TagKind::Media {
                kind: MediaKind::Movie,
                ref title,
                ref creator,
                status: MediaStatus::Done,
                rating: Some(9),
                year: Some(2017),
            } if title.as_deref() == Some("Blade Runner 2049")
                && creator.as_deref() == Some("Denis Villeneuve")
        ));
    }

    #[test]
    fn test_parse_purchase_full() {
        let tag = parse_tag(
            "purchase",
            Some("USB-C cable price=12.99 category=cables qty=2"),
            Span::empty(1, 1),
        );
        match tag.kind {
            TagKind::Purchase(p) => {
                assert_eq!(p.item, "USB-C cable");
                assert_eq!(p.category.as_deref(), Some("cables"));
                assert_eq!(p.quantity, 2);
                let money = p.price.unwrap();
                assert_eq!(money.cents, 1299);
                assert_eq!(money.currency, None);
                assert_eq!(p.total_cents(), Some(2598));
            }
            other => panic!("expected purchase, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_purchase_item_only() {
        let tag = parse_tag(
            "purchase",
            Some("The Rust Programming Language"),
            Span::empty(1, 1),
        );
        assert!(matches!(
            tag.kind,
            TagKind::Purchase(PurchaseValue {
                ref item,
                price: None,
                category: None,
                quantity: 1,
            }) if item == "The Rust Programming Language"
        ));
    }

    #[test]
    fn test_parse_media_status_synonyms() {
        let tag = parse_tag("media", Some("book Dune status=reading"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Media {
                status: MediaStatus::Active,
                ..
            }
        ));

        let tag = parse_tag("media", Some("album OK by=Radiohead"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Media {
                kind: MediaKind::Album,
                status: MediaStatus::Todo,
                ref creator,
                ..
            } if creator.as_deref() == Some("Radiohead")
        ));
    }

    #[test]
    fn test_parse_media_unknown_kind_preserved() {
        let tag = parse_tag("media", Some("boardgame Catan"), Span::empty(1, 1));
        assert!(matches!(
            tag.kind,
            TagKind::Media { kind: MediaKind::Other(ref k), .. } if k == "boardgame"
        ));
    }

    #[test]
    fn test_parse_media_empty_falls_back() {
        let tag = parse_tag("media", None, Span::empty(1, 1));
        assert!(matches!(tag.kind, TagKind::Unknown { ref name, .. } if name == "media"));
    }

    fn test_parse_purchase_currency_symbol() {
        let tag = parse_tag(
            "purchase",
            Some("HDMI cable price=$8.50"),
            Span::empty(1, 1),
        );
        match tag.kind {
            TagKind::Purchase(p) => {
                let money = p.price.unwrap();
                assert_eq!(money.cents, 850);
                assert_eq!(money.currency, Some('$'));
                assert_eq!(money.to_string(), "$8.50");
            }
            other => panic!("expected purchase, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_purchase_attributes_any_order() {
        let tag = parse_tag(
            "purchase",
            Some("category=books price=40 Some Book"),
            Span::empty(1, 1),
        );
        match tag.kind {
            TagKind::Purchase(p) => {
                assert_eq!(p.item, "Some Book");
                assert_eq!(p.category.as_deref(), Some("books"));
                assert_eq!(p.price.unwrap().cents, 4000);
            }
            other => panic!("expected purchase, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_purchase_empty_falls_back() {
        let tag = parse_tag("purchase", None, Span::empty(1, 1));
        assert!(matches!(tag.kind, TagKind::Unknown { ref name, .. } if name == "purchase"));
    }

    #[test]
    fn test_money_parsing() {
        assert_eq!(
            parse_money("12.99"),
            Some(Money {
                currency: None,
                cents: 1299
            })
        );
        assert_eq!(
            parse_money("40"),
            Some(Money {
                currency: None,
                cents: 4000
            })
        );
        assert_eq!(
            parse_money("9.5"),
            Some(Money {
                currency: None,
                cents: 950
            })
        );
        assert_eq!(
            parse_money("$8.50"),
            Some(Money {
                currency: Some('$'),
                cents: 850
            })
        );
        assert_eq!(parse_money("abc"), None);
        assert_eq!(parse_money("1.999"), None);
        assert_eq!(parse_money(""), None);
    }

    #[test]
    fn test_unknown_tag() {
        let tag = parse_tag("custom", Some("value"), Span::empty(1, 1));
        assert!(
            matches!(tag.kind, TagKind::Unknown { ref name, ref value } if name == "custom" && value.as_deref() == Some("value"))
        );
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

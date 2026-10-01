# Syntax Reference

## Tags

All morg-mode metadata uses `#tag` syntax. A `#` followed immediately by an alphanumeric character (no space) is a tag. `# ` with a space is a heading. Alphanumeric is Unicode-aware: names like `#café` or `#予定` are tags too, both block-level and inline.

### Block-level tags

A tag on its own line is block-level. The argument extends to end of line.

```
#todo refactor the parser
#deadline 2026-04-10
#clock-in 2026-04-03T09:00
#clock-out 2026-04-03T10:30
#clock 1h30m
#event 2026-04-10 Team meeting
```

### Inline tags

Tags can appear within text. The argument extends to the next `#` or end of line.

```
Some text #todo fix this before #deadline 2026-04-15
```

### Escaping

Use `\#` for a literal hash: `Price is \#100`.

### User-defined tags

Custom tags lex like any unknown tag (name plus greedy argument); a `[tags]`
section in the config gives them *interpretation* on top, without changing
how any document parses:

```toml
[tags.book]
pattern = '"(?<title>[^"]+)"\s+by\s+(?<author>.+)'   # named capture groups

[tags.reading-time]
kind = "duration"      # duration | date | timestamp | slug
```

Each declaration carries exactly one rule. A `pattern` is a regex (the
`regex` crate: no lookaround, no catastrophic backtracking) whose named
capture groups become the tag's fields. A `kind` reuses a built-in argument
parser — `duration` (`#effort`-style `1h30m`), `date` / `timestamp`
(`#deadline`-style), or `slug` (`#anchor`-style) — and yields one field named
after the kind holding the value's canonical rendering. Built-in tag names
cannot be redefined.

Interpretation is lenient: an argument that does not match the declared
shape leaves the document untouched and only flags the tag, which `morg
lint` reports as a warning. `morg tags <name>` tabulates every occurrence
of a declared tag with one column per field.

## Media

The `#media` tag records books, movies, music, games, and similar items so
they can be aggregated into to-read / to-watch / to-listen lists with
`morg media`.

```
#media book The Hobbit by="J.R.R. Tolkien" status=to-read
#media movie "Blade Runner 2049" director="Denis Villeneuve" status=watched rating=9 year=2017
#media album "OK Computer" by=Radiohead status=to-listen
#media game Hades status=playing
```

The argument is `<kind> <title…> [key=value …]`:

- **kind** -- the first word selects the medium and which list the item lands
  on:
  - *To Read* -- `book`, `article`, `comic`, `manga`
  - *To Watch* -- `movie`, `film`, `show`, `tv`, `series`, `anime`
  - *To Listen* -- `album`, `music`, `podcast`, `song`
  - *To Play* -- `game`
  - any other word is kept verbatim and grouped under *Other*.
- **title** -- the remaining bare words. Wrap multi-word values in double
  quotes to keep them intact (`"Blade Runner 2049"`).
- **attributes** (`key=value`):
  - `by` / `author` / `director` / `artist` / `creator`
  - `status` / `state` -- `todo` (default), `active`, or `done`. Synonyms are
    accepted: `to-read`/`want`/`queued` → todo, `reading`/`watching`/`playing`
    → active, `read`/`watched`/`finished` → done.
  - `rating` / `score` -- a number
  - `year` -- release year

List and filter items with:

```sh
morg media                      # grouped to-read/watch/listen lists
morg media --status todo        # only items you haven't started
morg media --category watch     # only the to-watch list
morg media --format json        # machine-readable
```

## Purchases

The `#purchase` tag records something to buy or bought. The free-text item name
is required; `price`, `category`, and `qty` are optional `key=value` attributes
that may appear in any order, before or after the item name.

```
#purchase USB-C cable price=12.99 category=cables qty=2
#purchase HDMI cable price=$8.50 category=cables
#purchase The Rust Programming Language price=39.99 category=books
#purchase Notebook
```

- `price` (alias `cost`) -- amount with an optional leading currency symbol
  (`$`, `£`, `€`) and up to two decimal places, e.g. `12.99`, `$8.50`, `40`.
- `category` (alias `cat`) -- a single word used to group entries.
- `qty` (aliases `quantity`, `count`) -- a positive integer; defaults to `1`.

`morg purchases` aggregates every `#purchase` across your files, grouped by
category, with per-category subtotals and a grand total (line totals are
`price × qty`). Like other tags, `#purchase` works inline or block-level and is
skipped under `#archive` headings.

```
$ morg purchases
Purchases

books
  The Rust Programming Language  $39.99  -- notes.md:6
  subtotal: $39.99

cables
  HDMI cable  $8.50  -- notes.md:4
  USB-C cable x2  $25.98  -- notes.md:3
  subtotal: $34.48

3 purchase(s), total $74.47
```

## Anchors

The `#anchor` tag gives a block a stable, user-chosen name so it can be
addressed as `id#name` (document id plus anchor name) no matter where it
moves. It works anywhere other tags do -- trailing on a heading, paragraph,
or list item, or on its own line as a block-level tag:

```
## Methods #anchor methods

The key claim of the paper. #anchor claim-1

- supporting evidence #anchor ev_2021-1

#anchor standalone-note
```

Anchor names are slugs: ASCII letters and digits plus `-` and `_`, starting
with a letter or digit. Anything else (empty, whitespace, non-ASCII) is not
a valid name -- the tag falls back to an unknown tag in the parser's usual
lenient style.

## Citations

Pandoc-style citations reference a bibliography key inline, with an
optional locator after a comma:

```
The gradient flows end to end [@paszke_pytorch_2019].
A narrower claim [@kohler_2019, p. 4].
```

- **key** -- starts right after `[@`: ASCII letters, digits, and `_`, with
  `-` also allowed after the first character (`real_key-1`).
- **locator** -- free text between the comma and the closing `]`, trimmed,
  e.g. `p. 4`, `pp. 10-12`, `ch. 2`. An empty locator (`[@key, ]`) is
  treated as absent.

Parsing is lenient: an empty or non-ASCII key, a bracket that never closes
on the line, or unexpected content after the key is left as plain text
rather than an error. Link syntax keeps precedence -- `[@key](url)` parses
as a link, not a citation.

HTML export renders a citation verbatim inside
`<span class="cite" data-cite-key="key">`, so output stays lossless until
bibliography resolution exists.

## Code Blocks

Standard markdown fences with tags and attributes on the info string:

````
```rust #tangle file=src/main.rs
fn main() {}
```
````

## Callouts

GitHub/Obsidian-style callouts with optional metadata:

```
> [!note][#tangle file=output.txt]
> Content here.
```

## Frontmatter

YAML between `---` delimiters at the start of a file:

```
---
title: My Document
tags: [rust, morg]
---
```

Frontmatter is parsed with [saphyr](https://docs.rs/saphyr), which preserves
source positions; the raw text between the delimiters is kept verbatim.
`morg frontmatter` aggregates and merges frontmatter across files -- merged
output is standard YAML, with sequence items indented and strings containing
commas double-quoted.

## Tables

Standard markdown pipe tables with alignment:

```
| Left | Center | Right |
|:-----|:------:|------:|
| a    |   b    |     c |
```

# Syntax Reference

## Tags

All morg-mode metadata uses `#tag` syntax. A `#` followed immediately by an alphanumeric character (no space) is a tag. `# ` with a space is a heading.

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

## Tables

Standard markdown pipe tables with alignment:

```
| Left | Center | Right |
|:-----|:------:|------:|
| a    |   b    |     c |
```

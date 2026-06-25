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

### Purchases

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

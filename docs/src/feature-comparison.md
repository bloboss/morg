# Feature Comparison: Org-Mode vs Morg-Mode

This table tracks which org-mode file-level features morg-mode supports, adapts, or intentionally omits.

## Document Structure

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Headings with levels | `* `, `** `, etc. | Supported | `# `, `## ` (markdown) |
| YAML frontmatter | N/A (`#+KEYWORD:`) | Supported | `---` delimited YAML at file start |

## TODO System

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| TODO/DONE keywords | `* TODO Task` | Supported | `#todo`/`#done` tags |
| Custom TODO sequences | `#+TODO: NEXT WAIT \| DONE` | Not yet | Custom keyword workflows |
| Priority cookies | `[#A]`, `[#B]`, `[#C]` | Not yet | Task prioritization |
| Statistics cookies | `[2/5]`, `[40%]` | Not yet | Progress tracking on parents |

## Timestamps & Planning

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Date tags | `<2026-04-03>` | Supported | `#date 2026-04-03`, `#deadline 2026-04-03` |
| Events | N/A | Supported | `#event 2026-04-10 Description` |
| Recurring timestamps | `<2026-04-03 +1w>` | Supported | `#deadline 2026-04-03 +1w` (`+Nd/w/m/y`) |
| SCHEDULED keyword | `SCHEDULED: <date>` | Supported | `#scheduled 2026-04-03` (with optional repeater) |
| Inactive timestamps | `[2026-04-03]` | Not yet | Timestamps that don't trigger agenda |
| Timestamp ranges | `<start>--<end>` | Not yet | Multi-day spans |
| Time-of-day | `<2026-04-03 14:00>` | Not yet | Only dates currently |
| Warning periods | `<2026-04-03 -3d>` | Not yet | Advance warning before deadline |
| CLOSED timestamp | `CLOSED: [timestamp]` | Not yet | Auto-recorded completion time |

## Time Tracking

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Clock entries | `CLOCK: [ts]--[ts]` | Supported | `#clock-in`/`#clock-out`/`#clock` tags |
| Duration formats | `1h30m` | Supported | Same |
| Effort estimates | `:EFFORT: 3:12` | Not yet | Planned vs actual tracking |

## Tags & Properties

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Inline tags | `:tag1:tag2:` on headings | Different | `#tag` anywhere in text |
| Tag inheritance | Child inherits parent tags | Not yet | |
| File-level tags | `#+FILETAGS:` | Not yet | Could use frontmatter |
| Property drawers | `:PROPERTIES:...:END:` | Supported | `#properties`...`#end` after headings |
| Special properties (ID) | `:ID: uuid` | Not yet | |

## Markup & Formatting

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Bold | `*bold*` | Supported | `**bold**` (markdown) |
| Italic | `/italic/` | Supported | `*italic*` (markdown) |
| Strikethrough | `+strike+` | Supported | `~~strike~~` (markdown) |
| Inline code | `=code=`, `~code~` | Supported | `` `code` `` (markdown) |
| Superscript/subscript | `a^b`, `a_b` | Not yet | |
| LaTeX fragments | `$x^2$` | Not yet | |
| Entities | `\alpha` | Not yet | |
| Line break | `\\` at EOL | Not yet | |

## Links

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| External links | `[[url][desc]]` | Supported | `[desc](url)` with optional title and metadata |
| Internal links | `[[#id]]`, `[[*Heading]]` | Supported | `[](#anchor)` (markdown) |
| Include/noweb links | `#+INCLUDE:` | Supported | `<<block-name>>` noweb + link metadata |
| Radio targets | `<<<target>>>` | Not planned | |

## Lists

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Unordered lists | `- `, `+ `, `* ` | Supported | Markdown standard |
| Ordered lists | `1. `, `1) ` | Supported | Markdown standard (`N. `) |
| Description lists | `- term :: desc` | Not yet | |
| Checkboxes | `- [ ]`, `- [X]` | Supported | Collected as TODOs by `morg todos` |

## Code Blocks

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Fenced code blocks | `#+BEGIN_SRC lang` | Supported | `` ``` `` markdown fences |
| Tangling | `:tangle filename` | Supported | `#tangle file=path` on fence |
| Named blocks | `#+NAME: name` | Supported | `name=blockname` attribute on fence |
| Noweb references | `<<block-name>>` | Supported | Full-line and inline, with indent preservation |
| `:var` arguments | `:var x=value` | Not yet | |
| `:results` | `:results output` | Not yet | |
| `:session` | `:session name` | Not yet | |
| CALL syntax | `#+CALL: block(args)` | Not yet | |
| Inline code execution | `src_python{1+1}` | Not yet | |

## Special Blocks

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Callouts | N/A | Supported | `> [!type][metadata]` with tangling |
| Quote blocks | `#+BEGIN_QUOTE` | Partial | `>` blockquotes parsed as callouts |
| Tables | `\| a \| b \|` | Supported | Same syntax, no formulas |
| Table formulas | `#+TBLFM:` | Not yet | |
| HTML passthrough | N/A | Supported | `<div>...</div>` preserved |

## Footnotes

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Footnote references | `[fn:name]` | Not yet | Could use `[^name]` (markdown) |
| Inline footnotes | `[fn::text]` | Not yet | |

## Export

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| iCalendar export | org-icalendar (UI) | Supported | `morg ical` CLI command |
| Frontmatter merge | N/A | Supported | `morg frontmatter` CLI command |
| Export options | `#+OPTIONS:` | Not yet | |
| Captions | `#+CAPTION:` | Not yet | |

## Include & Macros

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Include directive | `#+INCLUDE: "file"` | Supported | Via noweb `<<name>>` references |
| Macro definitions | `#+MACRO: name $1` | Not yet | |
| Macro calls | `{{{name(args)}}}` | Not yet | |

## Other

| Feature | Org-Mode Syntax | Morg Status | Morg Syntax / Notes |
|---|---|---|---|
| Comments | `# comment` | Not yet | Conflicts with heading syntax |
| Horizontal rules | `-----` | Not yet | |
| Drawers | `:NAME:...:END:` | Partial | Property drawers via `#properties`...`#end` |
| Archiving | `:ARCHIVE:` tag | Not yet | |
| Citations | `[cite:@key]` | Supported | `[@key]`, `[@key, p. 4]` (pandoc-style) |

## Morg-Unique Features

| Feature | Morg Syntax | Notes |
|---|---|---|
| `#tag` inline system | `#todo`, `#deadline`, etc. | Tags anywhere in text, not position-dependent |
| Callout metadata | `> [!type][#tangle file=x]` | Tangleable callouts |
| Block anchors | `#anchor intro-claim` | Stable block addresses (`id#name`), trailing or standalone |
| YAML frontmatter merge | `morg frontmatter` | Aggregate structured metadata |
| Escaped hash | `\#` | Literal `#` in text |

# File-Level Tags

## Summary

Allow tags declared in YAML frontmatter to apply to every entry in the file, equivalent to org-mode's `#+FILETAGS:`.

## Syntax

```yaml
---
tags: [backend, sprint-42]
---
```

All headings and entries in this file implicitly carry `#backend` and `#sprint-42`.

## Implementation

- **`collect.rs`**: After parsing a file, extract `tags` from frontmatter. Include them as inherited tags for all entries in that file.
- No parser changes needed — frontmatter already parsed as `saphyr::YamlOwned`.

### Files to modify
- `crates/morg-mode/src/collect.rs`

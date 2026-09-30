use std::path::{Path, PathBuf};

use morg_parser::ast::*;
use morg_parser::tags::TagKind;

use crate::collect;

pub fn run(
    paths: &[PathBuf],
    output: Option<&Path>,
    standalone: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);
    let mut html = String::new();

    if standalone {
        let title = parsed
            .first()
            .and_then(|pf| pf.document.frontmatter.as_ref())
            .and_then(|fm| fm.data.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("morg document");

        html.push_str(&format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>\n{CSS}\n</style>\n</head>\n<body>\n<article>\n"
        ));
    }

    // Collect footnote definitions for rendering at the end
    let mut footnotes: Vec<(String, String)> = Vec::new();

    for pf in &parsed {
        render_blocks(&pf.document.children, &mut html, &mut footnotes);
    }

    // Render footnotes section
    if !footnotes.is_empty() {
        html.push_str("<section class=\"footnotes\">\n<hr>\n<ol>\n");
        for (label, content) in &footnotes {
            html.push_str(&format!(
                "<li id=\"fn-{label}\"><p>{content} <a href=\"#fnref-{label}\">↩</a></p></li>\n"
            ));
        }
        html.push_str("</ol>\n</section>\n");
    }

    if standalone {
        html.push_str("</article>\n</body>\n</html>\n");
    }

    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &html)?;
            eprintln!("Exported to {}", path.display());
        }
        None => {
            print!("{html}");
        }
    }

    Ok(())
}

fn render_blocks(blocks: &[Block], out: &mut String, footnotes: &mut Vec<(String, String)>) {
    for block in blocks {
        render_block(block, out, footnotes);
    }
}

fn render_block(block: &Block, out: &mut String, footnotes: &mut Vec<(String, String)>) {
    match block {
        Block::Heading(h) => {
            let level = h.level;
            let id = slug(&h.content.plain_text());
            out.push_str(&format!("<h{level} id=\"{id}\">"));
            render_inline(&h.content, out);
            out.push_str(&format!("</h{level}>\n"));
        }
        Block::Paragraph(p) => {
            out.push_str("<p>");
            render_inline(&p.content, out);
            out.push_str("</p>\n");
        }
        Block::CodeBlock(cb) => {
            let lang_attr = cb
                .lang
                .as_deref()
                .map(|l| format!(" class=\"language-{l}\""))
                .unwrap_or_default();
            out.push_str(&format!(
                "<pre><code{lang_attr}>{}</code></pre>\n",
                escape_html(&cb.body)
            ));
        }
        Block::BlankLine(_) => {}
        Block::BlockTag(tag) => {
            render_block_tag(tag, out);
        }
        Block::Callout(c) => {
            out.push_str(&format!(
                "<blockquote class=\"callout callout-{}\">\n",
                c.kind
            ));
            out.push_str(&format!(
                "<p class=\"callout-title\">{}</p>\n",
                capitalize(&c.kind)
            ));
            render_blocks(&c.content, out, footnotes);
            out.push_str("</blockquote>\n");
        }
        Block::Table(t) => {
            out.push_str("<table>\n<thead>\n<tr>\n");
            for (i, header) in t.headers.iter().enumerate() {
                let align = t.alignments.get(i).copied().unwrap_or(Alignment::None);
                let style = alignment_style(align);
                out.push_str(&format!("<th{style}>"));
                render_inline(header, out);
                out.push_str("</th>\n");
            }
            out.push_str("</tr>\n</thead>\n<tbody>\n");
            for row in &t.rows {
                out.push_str("<tr>\n");
                for (i, cell) in row.iter().enumerate() {
                    let align = t.alignments.get(i).copied().unwrap_or(Alignment::None);
                    let style = alignment_style(align);
                    out.push_str(&format!("<td{style}>"));
                    render_inline(cell, out);
                    out.push_str("</td>\n");
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
        Block::HtmlBlock(h) => {
            out.push_str(&h.raw);
            out.push('\n');
        }
        Block::List(list) => {
            let tag = match list.kind {
                ListKind::Unordered => "ul",
                ListKind::Ordered => "ol",
            };
            out.push_str(&format!("<{tag}>\n"));
            for item in &list.items {
                render_list_item(item, out, footnotes);
            }
            out.push_str(&format!("</{tag}>\n"));
        }
        Block::HorizontalRule(_) => {
            out.push_str("<hr>\n");
        }
        Block::Comment(_) => {
            // Comments are not rendered
        }
        Block::FootnoteDefinition(fd) => {
            let mut content = String::new();
            render_inline(&fd.content, &mut content);
            footnotes.push((fd.label.clone(), content));
        }
    }
}

fn render_list_item(item: &ListItem, out: &mut String, footnotes: &mut Vec<(String, String)>) {
    out.push_str("<li>");
    if let Some(checkbox) = &item.checkbox {
        let checked = match checkbox {
            Checkbox::Checked => " checked disabled",
            Checkbox::Unchecked => " disabled",
        };
        out.push_str(&format!("<input type=\"checkbox\"{checked}> "));
    }
    render_inline(&item.content, out);
    if let Some(ref desc) = item.description {
        out.push_str("<dl><dd>");
        render_inline(desc, out);
        out.push_str("</dd></dl>");
    }
    for child in &item.children {
        render_block(child, out, footnotes);
    }
    out.push_str("</li>\n");
}

fn render_inline(content: &InlineContent, out: &mut String) {
    for seg in &content.segments {
        render_inline_segment(seg, out);
    }
}

fn render_inline_segment(seg: &InlineSegment, out: &mut String) {
    match &seg.kind {
        InlineKind::Text(t) => out.push_str(&escape_html(t)),
        InlineKind::Tag(tag) => render_inline_tag(tag, out),
        InlineKind::Bold(inner) => {
            out.push_str("<strong>");
            render_inline(inner, out);
            out.push_str("</strong>");
        }
        InlineKind::Italic(inner) => {
            out.push_str("<em>");
            render_inline(inner, out);
            out.push_str("</em>");
        }
        InlineKind::Strikethrough(inner) => {
            out.push_str("<del>");
            render_inline(inner, out);
            out.push_str("</del>");
        }
        InlineKind::Code(c) => {
            out.push_str(&format!("<code>{}</code>", escape_html(c)));
        }
        InlineKind::Link(link) => {
            let title_attr = link
                .title
                .as_deref()
                .map(|t| format!(" title=\"{}\"", escape_html(t)))
                .unwrap_or_default();
            out.push_str(&format!(
                "<a href=\"{}\"{}>{}</a>",
                escape_html(&link.url),
                title_attr,
                escape_html(&link.text),
            ));
        }
        InlineKind::FootnoteRef(label) => {
            out.push_str(&format!(
                "<sup><a id=\"fnref-{label}\" href=\"#fn-{label}\">{label}</a></sup>"
            ));
        }
    }
}

fn render_inline_tag(tag: &morg_parser::tags::Tag, out: &mut String) {
    match &tag.kind {
        TagKind::Todo { text } => {
            out.push_str("<span class=\"tag tag-todo\">TODO</span>");
            if let Some(t) = text {
                out.push_str(&format!(" {}", escape_html(t)));
            }
        }
        TagKind::Done { text } => {
            out.push_str("<span class=\"tag tag-done\">DONE</span>");
            if let Some(t) = text {
                out.push_str(&format!(" <del>{}</del>", escape_html(t)));
            }
        }
        TagKind::Deadline { date, .. } => {
            out.push_str(&format!(
                "<span class=\"tag tag-deadline\">DEADLINE: {date}</span>"
            ));
        }
        TagKind::Scheduled { date, .. } => {
            out.push_str(&format!(
                "<span class=\"tag tag-scheduled\">SCHEDULED: {date}</span>"
            ));
        }
        TagKind::Priority { level } => {
            out.push_str(&format!(
                "<span class=\"tag tag-priority tag-priority-{}\">#{level}</span>",
                level.to_string().to_lowercase()
            ));
        }
        TagKind::Archive => {
            out.push_str("<span class=\"tag tag-archive\">ARCHIVE</span>");
        }
        // Other tags: render as subtle spans
        _ => {}
    }
}

fn render_block_tag(tag: &morg_parser::tags::Tag, out: &mut String) {
    match &tag.kind {
        TagKind::Todo { text } => {
            out.push_str("<p><span class=\"tag tag-todo\">TODO</span>");
            if let Some(t) = text {
                out.push_str(&format!(" {}", escape_html(t)));
            }
            out.push_str("</p>\n");
        }
        TagKind::Done { text } => {
            out.push_str("<p><span class=\"tag tag-done\">DONE</span>");
            if let Some(t) = text {
                out.push_str(&format!(" <del>{}</del>", escape_html(t)));
            }
            out.push_str("</p>\n");
        }
        TagKind::Deadline { date, .. } => {
            out.push_str(&format!(
                "<p class=\"planning\"><strong>DEADLINE:</strong> {date}</p>\n"
            ));
        }
        TagKind::Scheduled { date, .. } => {
            out.push_str(&format!(
                "<p class=\"planning\"><strong>SCHEDULED:</strong> {date}</p>\n"
            ));
        }
        _ => {}
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn alignment_style(align: Alignment) -> &'static str {
    match align {
        Alignment::Left => " style=\"text-align:left\"",
        Alignment::Center => " style=\"text-align:center\"",
        Alignment::Right => " style=\"text-align:right\"",
        Alignment::None => "",
    }
}

const CSS: &str = r#"
body { max-width: 48em; margin: 2em auto; padding: 0 1em; font-family: system-ui, sans-serif; line-height: 1.6; color: #222; }
h1, h2, h3, h4, h5, h6 { margin-top: 1.5em; }
pre { background: #f5f5f5; padding: 1em; overflow-x: auto; border-radius: 4px; }
code { font-size: 0.9em; }
p code { background: #f0f0f0; padding: 0.15em 0.3em; border-radius: 3px; }
blockquote { border-left: 4px solid #ddd; margin: 1em 0; padding: 0.5em 1em; }
blockquote.callout { border-radius: 4px; }
blockquote.callout-note { border-left-color: #4a9eff; background: #f0f7ff; }
blockquote.callout-warning { border-left-color: #f5a623; background: #fff8f0; }
blockquote.callout-tip { border-left-color: #2ecc71; background: #f0fff5; }
blockquote.callout-danger { border-left-color: #e74c3c; background: #fff0f0; }
.callout-title { font-weight: bold; margin: 0 0 0.5em; }
table { border-collapse: collapse; width: 100%; margin: 1em 0; }
th, td { border: 1px solid #ddd; padding: 0.5em; }
th { background: #f5f5f5; }
.tag { display: inline-block; padding: 0.1em 0.4em; border-radius: 3px; font-size: 0.85em; font-weight: bold; }
.tag-todo { background: #fff3cd; color: #856404; }
.tag-done { background: #d4edda; color: #155724; }
.tag-deadline { background: #f8d7da; color: #721c24; }
.tag-scheduled { background: #cce5ff; color: #004085; }
.tag-priority-a { background: #f8d7da; color: #721c24; }
.tag-priority-b { background: #fff3cd; color: #856404; }
.tag-priority-c { background: #d4edda; color: #155724; }
.tag-archive { background: #e2e3e5; color: #383d41; }
.planning { color: #666; font-size: 0.9em; }
.footnotes { font-size: 0.9em; color: #555; }
li { margin: 0.25em 0; }
input[type="checkbox"] { margin-right: 0.5em; }
hr { border: none; border-top: 1px solid #ddd; margin: 2em 0; }
del { color: #888; }
"#;

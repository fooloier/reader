use anyhow::{Context, Result, bail, ensure};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use roxmltree::Node;
use serde::Serialize;
use std::{collections::HashMap, path::Path};

#[derive(Clone, Debug, Serialize)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub depth: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Block {
    pub anchor: String,
    pub aliases: Vec<String>,
    pub markdown: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Chapter {
    pub href: String,
    pub blocks: Vec<Block>,
}
pub struct EpubBook {
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    pub images: HashMap<String, Vec<u8>>,
}

impl EpubBook {
    pub fn open(path: &Path) -> Result<Self> {
        let epub = rbook::Epub::open(path)?;
        let mut chapters = Vec::new();
        let mut total = 0;
        for entry in epub.spine().iter().filter(|e| e.is_linear()) {
            let entry = entry.manifest_entry().context("EPUB 正文资源缺失")?;
            let content = entry.read_str()?;
            total += content.len();
            ensure!(total < 32 * 1024 * 1024, "EPUB 正文超过 32 MiB 上限");
            let href = canonical_href(entry.href().as_ref());
            chapters.push(parse_chapter(&href, &content)?);
        }
        ensure!(!chapters.is_empty(), "EPUB 没有可阅读的章节");
        let toc = epub
            .toc()
            .contents()
            .map(|root| {
                root.flatten()
                    .filter_map(|e| {
                        Some(TocEntry {
                            title: e.label().into(),
                            href: canonical_href(e.href()?.as_ref()),
                            depth: e.depth().saturating_sub(1),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut images = HashMap::new();
        let mut image_bytes = 0;
        for entry in epub.manifest().images() {
            let bytes = entry.read_bytes()?;
            image_bytes += bytes.len();
            ensure!(image_bytes < 64 * 1024 * 1024, "EPUB 图片超过 64 MiB 上限");
            images.insert(canonical_href(entry.href().as_ref()), bytes);
        }
        Ok(Self {
            chapters,
            toc,
            images,
        })
    }
    pub fn destination(&self, href: &str) -> Option<(usize, usize)> {
        let normalized = canonical_href(href);
        let (path, fragment) = normalized.split_once('#').unwrap_or((&normalized, ""));
        let chapter = self.chapters.iter().position(|c| c.href == path)?;
        if fragment.is_empty() {
            return Some((chapter, 0));
        }
        let fragment = percent_decode_str(fragment).decode_utf8().ok()?;
        let block = self.chapters[chapter].blocks.iter().position(|b| {
            b.anchor == fragment || b.aliases.iter().any(|v| v.as_str() == fragment.as_ref())
        })?;
        Some((chapter, block))
    }
}

pub fn encode_component(value: &str) -> String {
    const SAFE: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(value, SAFE).to_string()
}

fn canonical_component(value: &str) -> String {
    encode_component(&percent_decode_str(value).decode_utf8_lossy())
}

pub fn canonical_href(value: &str) -> String {
    resolve_href("/", value)
}

pub fn resolve_href(base: &str, value: &str) -> String {
    if value
        .split(['/', '#', '?'])
        .next()
        .is_some_and(|part| part.contains(':'))
        || value.starts_with("//")
    {
        return value.into();
    }
    let (path, fragment) = value
        .split_once('#')
        .map_or((value, None), |(p, f)| (p, Some(f)));
    let path = path.split('?').next().unwrap_or(path);
    let base = base.split(['#', '?']).next().unwrap_or(base);
    let combined = if path.is_empty() {
        base.to_owned()
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        format!(
            "{}/{path}",
            base.rsplit_once('/').map(|v| v.0).unwrap_or("")
        )
    };
    let mut parts = Vec::new();
    for part in combined.split('/') {
        let part = canonical_component(part);
        match part.as_str() {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut result = format!("/{}", parts.join("/"));
    if let Some(fragment) = fragment {
        result.push('#');
        result.push_str(&canonical_component(fragment));
    }
    result
}

fn xml_entities(content: &str) -> Result<String> {
    ensure!(
        content.len() <= 32 * 1024 * 1024,
        "EPUB 章节超过 32 MiB 上限"
    );
    let mut output = String::with_capacity(content.len());
    let mut rest = content;
    while !rest.is_empty() {
        if rest.starts_with("<!--") || rest.starts_with("<![CDATA[") {
            let end = if rest.starts_with("<!--") {
                "-->"
            } else {
                "]]>"
            };
            let length = rest.find(end).context("EPUB XML 注释或 CDATA 未结束")? + end.len();
            output.push_str(&rest[..length]);
            rest = &rest[length..];
        } else if rest.starts_with("<!DOCTYPE") {
            let mut quote = None;
            let mut end = None;
            for (i, c) in rest.char_indices().skip(9) {
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else {
                    match c {
                        '\'' | '"' => quote = Some(c),
                        '[' => bail!("EPUB 不支持自定义 DTD 实体"),
                        '>' => {
                            end = Some(i + 1);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            rest = &rest[end.context("EPUB DOCTYPE 未结束")?..];
        } else if rest.starts_with('&') {
            let length = rest[1..]
                .bytes()
                .take(40)
                .take_while(|c| c.is_ascii_alphanumeric())
                .count()
                + 1;
            if rest.as_bytes().get(length) == Some(&b';') {
                let entity = &rest[1..=length];
                if let Some(&(a, b)) = markup5ever::data::NAMED_ENTITIES.get(entity) {
                    output.push_str(&format!("&#{a};"));
                    if b != 0 {
                        output.push_str(&format!("&#{b};"));
                    }
                    rest = &rest[length + 1..];
                    continue;
                }
            }
            output.push('&');
            rest = &rest[1..];
        } else {
            let c = rest.chars().next().unwrap();
            output.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(output)
}

pub fn parse_chapter(href: &str, content: &str) -> Result<Chapter> {
    let content = xml_entities(content)?;
    let doc = roxmltree::Document::parse_with_options(
        &content,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .context("EPUB XHTML 无效")?;
    ensure!(
        doc.descendants()
            .all(|node| node.ancestors().take(130).count() <= 128),
        "EPUB XHTML 嵌套超过 128 层"
    );
    let body = doc
        .descendants()
        .find(|n| n.has_tag_name("body"))
        .or_else(|| {
            doc.root_element()
                .has_tag_name("svg")
                .then(|| doc.root_element())
        })
        .context("EPUB 章节缺少正文")?;
    let mut blocks = Vec::new();
    collect_blocks(body, href, &mut blocks);
    let mut anchors: std::collections::HashSet<String> = blocks
        .iter()
        .flat_map(|block| block.aliases.iter().cloned())
        .collect();
    for block in &mut blocks {
        if block.aliases.is_empty() {
            while anchors.contains(&block.anchor) {
                block.anchor.push('-');
            }
            anchors.insert(block.anchor.clone());
        }
    }
    Ok(Chapter {
        href: canonical_href(href),
        blocks,
    })
}

fn is_block(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "table"
            | "blockquote"
            | "pre"
            | "hr"
            | "div"
            | "section"
            | "article"
            | "figure"
            | "body"
            | "main"
            | "aside"
            | "nav"
            | "header"
            | "footer"
            | "dl"
            | "dt"
            | "dd"
    )
}

fn push_block(nodes: &[Node<'_, '_>], href: &str, blocks: &mut Vec<Block>) {
    let markdown: String = nodes.iter().map(|n| markdown(*n, href)).collect();
    let aliases: Vec<_> = nodes
        .iter()
        .flat_map(|n| n.descendants())
        .filter_map(|n| n.attribute("id"))
        .map(str::to_owned)
        .collect();
    if !markdown.trim().is_empty() || !aliases.is_empty() {
        blocks.push(Block {
            anchor: aliases
                .first()
                .cloned()
                .unwrap_or_else(|| format!("block-{}", blocks.len())),
            aliases,
            markdown,
        });
    }
}

fn collect_blocks(node: Node<'_, '_>, href: &str, blocks: &mut Vec<Block>) {
    if matches!(node.tag_name().name(), "script" | "style" | "head") {
        return;
    }
    if !matches!(
        node.tag_name().name(),
        "body"
            | "div"
            | "section"
            | "article"
            | "figure"
            | "main"
            | "aside"
            | "nav"
            | "header"
            | "footer"
            | "dl"
            | "dd"
    ) {
        push_block(&[node], href, blocks);
        return;
    }
    let start = blocks.len();
    let mut inline = Vec::new();
    for child in node.children() {
        if is_block(child) {
            push_block(&inline, href, blocks);
            inline.clear();
            collect_blocks(child, href, blocks);
        } else {
            inline.push(child);
        }
    }
    push_block(&inline, href, blocks);
    if let Some(id) = node.attribute("id") {
        if let Some(block) = blocks.get_mut(start) {
            if block.aliases.is_empty() {
                block.anchor = id.into();
            }
            block.aliases.push(id.into());
        } else {
            push_block(&[node], href, blocks);
        }
    }
}

fn escape(text: &str) -> String {
    let mut escaped = String::new();
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' | '*' | '_' | '[' | ']' | '`' | '~' | '|' | '#' | '-' | '+' | '.' | '!' | ')'
            | '=' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped
}
fn destination_markdown(href: &str, value: &str) -> String {
    const UNSAFE: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'(')
        .add(b')')
        .add(b'<')
        .add(b'>')
        .add(b'[')
        .add(b']')
        .add(b'"')
        .add(b'\\');
    utf8_percent_encode(&resolve_href(href, value), UNSAFE).to_string()
}

fn code_content(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

fn backticks(text: &str, minimum: usize) -> String {
    "`".repeat(
        text.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(minimum),
    )
}

fn markdown(node: Node<'_, '_>, href: &str) -> String {
    if node.is_text() {
        let text = node.text().unwrap_or("");
        let normalized = escape(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if normalized.is_empty() {
            return if text.is_empty() {
                String::new()
            } else {
                " ".into()
            };
        }
        return format!(
            "{}{}{}",
            if text.starts_with(char::is_whitespace) {
                " "
            } else {
                ""
            },
            normalized,
            if text.ends_with(char::is_whitespace) {
                " "
            } else {
                ""
            }
        );
    }
    let inner = || {
        node.children()
            .map(|c| markdown(c, href))
            .collect::<String>()
    };
    match node.tag_name().name() {
        "script" | "style" => String::new(),
        "h1" => format!("# {}", inner()),
        "h2" => format!("## {}", inner()),
        "h3" => format!("### {}", inner()),
        "h4" | "h5" | "h6" => format!("#### {}", inner()),
        "strong" | "b" => format!("**{}**", inner()),
        "em" | "i" => format!("*{}*", inner()),
        "s" | "del" => format!("~~{}~~", inner()),
        "br" => "  \n".into(),
        "hr" => "---".into(),
        "img" | "image" => format!(
            "![{}]({})",
            escape(node.attribute("alt").unwrap_or("插图")),
            destination_markdown(
                href,
                node.attribute("src")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
                    .or_else(|| node.attribute("href"))
                    .unwrap_or("")
            )
        ),
        "a" => format!(
            "[{}]({})",
            inner(),
            destination_markdown(href, node.attribute("href").unwrap_or(""))
        ),
        "code" => {
            let text = code_content(node).replace('\n', " ");
            let fence = backticks(&text, 1);
            format!("{fence} {text} {fence}")
        }
        "pre" => {
            let text = code_content(node);
            let fence = backticks(&text, 3);
            format!("{fence}\n{text}\n{fence}\n")
        }
        "blockquote" => format!("> {}", inner().replace('\n', "\n> ")),
        "ul" | "ol" => node
            .children()
            .filter(|n| n.has_tag_name("li"))
            .enumerate()
            .map(|(i, item)| {
                let marker = if node.has_tag_name("ol") {
                    format!("{}.", i + 1)
                } else {
                    "-".into()
                };
                let indent = " ".repeat(marker.len() + 1);
                let mut content = String::new();
                for child in item.children() {
                    if child.has_tag_name("ul") || child.has_tag_name("ol") {
                        if !content.ends_with('\n') {
                            content.push('\n');
                        }
                        content.push_str(&markdown(child, href));
                    } else {
                        content.push_str(&markdown(child, href));
                    }
                }
                let mut lines = content.trim().lines();
                let mut result = format!("{marker} {}\n", lines.next().unwrap_or(""));
                for line in lines {
                    result.push_str(&format!("{indent}{line}\n"));
                }
                result
            })
            .collect(),
        "table" => {
            let rows: Vec<Vec<_>> = node
                .descendants()
                .filter(|n| n.has_tag_name("tr"))
                .map(|r| {
                    r.children()
                        .filter(|n| n.has_tag_name("td") || n.has_tag_name("th"))
                        .map(|n| inner_text(n, href))
                        .collect()
                })
                .collect();
            let mut s = String::new();
            for (i, row) in rows.iter().enumerate() {
                s.push_str(&format!("| {} |\n", row.join(" | ")));
                if i == 0 {
                    s.push_str(&format!("| {} |\n", vec!["---"; row.len()].join(" | ")));
                }
            }
            s
        }
        "p" => format!("{}\n\n", inner()),
        _ => inner(),
    }
}
fn inner_text(node: Node<'_, '_>, href: &str) -> String {
    node.children().map(|n| markdown(n, href)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn internal_links_and_inline_spacing_survive_conversion() {
        let chapter=super::parse_chapter("/OPS/a.xhtml", r#"<html><body><p id="a">Some <b>bold</b> and <i>italic</i> text.</p><a href="b.xhtml#next">Next</a></body></html>"#).unwrap();
        assert!(
            markdown::to_html(&chapter.blocks[0].markdown)
                .contains("Some <strong>bold</strong> and <em>italic</em> text.")
        );
        assert_eq!(chapter.blocks[1].markdown, "[Next](/OPS/b.xhtml#next)");
        assert_eq!(
            super::resolve_href("/OPS/a.xhtml", "../Images/c.png"),
            "/Images/c.png"
        );
    }
}

#[cfg(test)]
mod regressions {
    use super::*;

    #[test]
    fn real_xhtml_doctype_entities_svg_and_empty_chapters() {
        let chapter = parse_chapter("/OPS/a.xhtml", r#"<?xml version="1.0"?><!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html xmlns:xlink="http://www.w3.org/1999/xlink"><body><div id="intro">Hello <b>bold</b> world&nbsp;&copy; vertical-rl</div><svg><image xlink:href="封面%20%25%23.png"/></svg></body></html>"#).unwrap();
        assert_eq!(chapter.blocks.len(), 2);
        assert!(
            markdown::to_html(&chapter.blocks[0].markdown)
                .contains("Hello <strong>bold</strong> world © vertical-rl")
        );
        assert!(chapter.blocks[1].markdown.contains("%25%23.png"));
        let svg = parse_chapter("/cover.svg", r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="image%20%23.png"/></svg>"#).unwrap();
        assert!(svg.blocks[0].markdown.contains("/image%20%23.png"));
        assert!(
            parse_chapter("/empty.xhtml", "<html><body/></html>")
                .unwrap()
                .blocks
                .is_empty()
        );
        assert!(
            parse_chapter(
                "/attack.xhtml",
                r#"<!DOCTYPE html [<!ENTITY x "boom">]><html><body>&x;</body></html>"#
            )
            .is_err()
        );
    }

    #[test]
    fn xhtml_entities_and_markup_are_bounded_and_xml_safe() {
        let chapter = parse_chapter("/a.xhtml", r#"<?xml version="1.0"?><!DOCTYPE html><html><body><p id="&copy;">&amp;nbsp; <![CDATA[&nbsp;]]> &NotEqualTilde;</p><img src="a%20b.png" alt="a &quot; ] ) &amp; b"/><pre>```
# literal
```</pre></body></html>"#).unwrap();
        let rendered = markdown::to_html(&chapter.blocks[0].markdown);
        assert!(rendered.contains("&amp;nbsp; &amp;nbsp; ≂̸"), "{rendered}");
        assert_eq!(chapter.blocks[0].anchor, "©");
        let rendered = markdown::to_html(&chapter.blocks[1].markdown);
        assert!(
            rendered.contains("alt=\"a &quot; ] ) &amp; b\""),
            "{rendered}"
        );
        let rendered = markdown::to_html(&chapter.blocks[2].markdown);
        assert!(
            rendered.contains("<pre><code>```\n# literal\n```\n</code></pre>"),
            "{rendered}"
        );
        let deep = format!(
            "<html><body>{}x{}</body></html>",
            "<div>".repeat(130),
            "</div>".repeat(130)
        );
        assert!(parse_chapter("/deep.xhtml", &deep).is_err());
        assert!(parse_chapter("/bad.xhtml", "<html><body>&#0;</body></html>").is_err());
        assert!(parse_chapter("/unknown.xhtml", "<html><body>&custom;</body></html>").is_err());
    }

    #[test]
    fn url_components_resolve_once_and_unknown_fragments_are_disabled() {
        let href = "/OPS/%E4%B8%AD%E6%96%87%20%2520%23.xhtml";
        let book = EpubBook {
            chapters: vec![
                parse_chapter(href, "<html><body><p id=\"节 %20#\">目标</p></body></html>")
                    .unwrap(),
            ],
            toc: vec![],
            images: HashMap::new(),
        };
        assert_eq!(
            book.destination(&format!("{href}#%E8%8A%82%20%2520%23")),
            Some((0, 0))
        );
        assert_eq!(book.destination(&format!("{href}#missing")), None);
        assert_eq!(
            resolve_href("/OPS/a.xhtml", "中文%20%2520%23.xhtml#节%20%2520%23"),
            format!("{href}#%E8%8A%82%20%2520%23")
        );
    }

    #[test]
    fn mixed_containers_keep_inline_runs_and_literal_markdown() {
        let chapter = parse_chapter("/a.xhtml", r#"<html><body><div id="outer">Hello <b>bold</b> world<p id="p">paragraph</p>after <i>italic</i></div><p># heading - list 1. literal `code` ~tilde~ | pipe</p><img alt="a ] b" src="image%20%5D%29.png"/></body></html>"#).unwrap();
        assert_eq!(chapter.blocks.len(), 5);
        assert!(chapter.blocks[0].markdown.contains("Hello **bold** world"));
        assert!(chapter.blocks[2].markdown.contains("after *italic*"));
        assert!(chapter.blocks[3].markdown.contains("\\# heading"));
        assert!(chapter.blocks[3].markdown.contains("\\`code\\`"));
        assert!(chapter.blocks[4].markdown.contains("a \\] b"));
        let rendered = markdown::to_html(&chapter.blocks[3].markdown);
        assert!(
            rendered.contains("<p># heading - list 1. literal `code` ~tilde~ | pipe</p>"),
            "{rendered}"
        );
        let rendered = markdown::to_html(&chapter.blocks[4].markdown);
        assert!(rendered.contains("alt=\"a ] b\""), "{rendered}");
        assert!(rendered.contains("image%20%5D%29.png"), "{rendered}");
        for container in ["main", "aside", "nav", "header", "footer", "dl", "dd"] {
            let source = format!(
                "<html><body><{container}><p id=\"a\">one</p><p id=\"b\">two</p></{container}></body></html>"
            );
            let chapter = parse_chapter("/a.xhtml", &source).unwrap();
            assert_eq!(chapter.blocks.len(), 2, "{container}");
            assert_eq!(chapter.blocks[1].anchor, "b", "{container}");
        }
        let chapter = parse_chapter("/a.xhtml", "<html><body><p>1) first</p><p>Title<br/>===</p><ul><li>a</li><li>b<ul><li>c</li></ul></li></ul></body></html>").unwrap();
        assert_eq!(
            markdown::to_html(&chapter.blocks[0].markdown).trim(),
            "<p>1) first</p>"
        );
        let rendered = markdown::to_html(&chapter.blocks[1].markdown);
        assert!(!rendered.contains("<h1>"), "{rendered}");
        assert!(rendered.contains("==="), "{rendered}");
        let rendered = markdown::to_html(&chapter.blocks[2].markdown);
        assert_eq!(rendered.matches("<ul>").count(), 2, "{rendered}");
        assert!(rendered.contains("<li>c</li>"), "{rendered}");
        assert!(!rendered.contains("b- c"), "{rendered}");
        let chapter = parse_chapter(
            "/a.xhtml",
            "<html><body><p>generated</p><p id=\"block-0\">explicit</p></body></html>",
        )
        .unwrap();
        assert_ne!(chapter.blocks[0].anchor, chapter.blocks[1].anchor);
        let book = EpubBook {
            chapters: vec![chapter],
            toc: vec![],
            images: HashMap::new(),
        };
        assert_eq!(book.destination("/a.xhtml#block-0"), Some((0, 1)));
    }
}

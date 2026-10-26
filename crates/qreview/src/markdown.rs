//! A Markdown file, rendered as HTML for the reader.
//!
//! Every tag of the output is written here, from a short list. HTML in the
//! source comes out as text, so a file of the repository can never put a tag
//! of its own on the page. See design section 3.8.
//!
//! Each leaf block carries `data-lines="12-18"`, the lines of the source it
//! came from. That is what lets the rendered pane follow the diff.

use std::ops::Range;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::model::Span;

/// Above this size, a file has no rendered view.
pub const MAX_BYTES: usize = 1024 * 1024;

/// Render a Markdown text.
///
/// `dir` is the directory of the file in the tree, which a relative image
/// is read from. `paint` colors a fenced block: it takes the language of the
/// fence and the code, and answers the spans of each line, in bytes.
pub fn render(text: &str, dir: &str, paint: &dyn Fn(&str, &str) -> Vec<Vec<Span>>) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut writer = Writer {
        out: Vec::new(),
        starts: line_starts(text),
        run: None,
        blocks: Vec::new(),
        links: Vec::new(),
        aligns: Vec::new(),
        cell: 0,
        head: false,
        code: None,
        image: None,
        dir,
        paint,
    };

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        writer.event(event, range);
    }
    writer.close_run();
    writer.out.concat()
}

struct Writer<'p> {
    /// Pieces rather than one string, so the opening tag of a run can be
    /// written once its last line is known.
    out: Vec<String>,
    /// The byte where each line starts.
    starts: Vec<usize>,
    /// Text directly in a list item, which has no paragraph of its own: the
    /// piece that holds its opening tag, and the bytes it covers so far.
    run: Option<(usize, Range<usize>)>,
    /// For each open block, whether text may sit in it with no paragraph:
    /// an item of a tight list, or a definition.
    blocks: Vec<bool>,
    /// For each open link, whether it was written as an `<a>`.
    links: Vec<bool>,
    aligns: Vec<Alignment>,
    cell: usize,
    head: bool,
    /// A fenced or indented block being read: its language, its code, and
    /// its lines.
    code: Option<(String, String, String)>,
    /// An image of the tree being read: its path, its alternative text and
    /// its title. The text arrives as events, and an `alt` needs all of it.
    image: Option<(String, String, String)>,
    dir: &'p str,
    paint: &'p dyn Fn(&str, &str) -> Vec<Vec<Span>>,
}

impl Writer<'_> {
    fn event(&mut self, event: Event, range: Range<usize>) {
        if let Some((_, code, _)) = self.code.as_mut() {
            match event {
                Event::Text(text) => code.push_str(&text),
                Event::End(TagEnd::CodeBlock) => self.code_block(),
                _ => {}
            }
            return;
        }
        if let Some((_, alt, _)) = self.image.as_mut() {
            match event {
                Event::Text(text) | Event::Code(text) => alt.push_str(&text),
                Event::End(TagEnd::Image) => self.close_image(&range),
                _ => {}
            }
            return;
        }

        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.inline(&range, escape(&text)),
            Event::Code(text) => self.inline(&range, format!("<code>{}</code>", escape(&text))),
            Event::Html(text) | Event::InlineHtml(text) => self.inline(&range, escape(&text)),
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.inline(&range, format!("<code>{}</code>", escape(&text)))
            }
            Event::FootnoteReference(label) => {
                self.inline(&range, format!("<sup>[{}]</sup>", escape(&label)))
            }
            Event::SoftBreak => self.inline(&range, "\n".to_owned()),
            Event::HardBreak => self.inline(&range, "<br>".to_owned()),
            Event::TaskListMarker(done) => {
                let mark = if done { "☑" } else { "☐" };
                self.inline(&range, format!("<span class=\"md-task\">{mark}</span> "))
            }
            Event::Rule => {
                self.close_run();
                let lines = self.lines(&range);
                self.push(format!("<hr {lines}>"));
            }
        }
    }

    fn start(&mut self, tag: Tag, range: Range<usize>) {
        // Inline tags open inside a block. Every other tag closes the text
        // that sat directly in a list item.
        let inline = matches!(
            tag,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        );
        if !inline {
            self.close_run();
            if !matches!(tag, Tag::CodeBlock(_)) {
                self.blocks
                    .push(matches!(tag, Tag::Item | Tag::DefinitionListDefinition));
            }
        }
        let lines = self.lines(&range);

        let html = match tag {
            Tag::Paragraph => format!("<p {lines}>"),
            Tag::Heading { level, .. } => format!("<{} {lines}>", heading(level)),
            Tag::BlockQuote(kind) => match kind.map(alert) {
                Some(name) => format!("<blockquote class=\"md-alert md-alert-{name}\">"),
                None => "<blockquote>".to_owned(),
            },
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((language, String::new(), lines));
                return;
            }
            Tag::HtmlBlock => format!("<pre class=\"md-html\" {lines}>"),
            Tag::List(Some(first)) => format!("<ol start=\"{first}\">"),
            Tag::List(None) => "<ul>".to_owned(),
            Tag::Item => "<li>".to_owned(),
            Tag::FootnoteDefinition(label) => {
                format!("<div class=\"md-footnote\"><sup>[{}]</sup>", escape(&label))
            }
            Tag::DefinitionList => "<dl>".to_owned(),
            Tag::DefinitionListTitle => format!("<dt {lines}>"),
            Tag::DefinitionListDefinition => format!("<dd {lines}>"),
            Tag::Table(aligns) => {
                self.aligns = aligns;
                "<table>".to_owned()
            }
            Tag::TableHead => {
                self.head = true;
                self.cell = 0;
                format!("<thead><tr {lines}>")
            }
            Tag::TableRow => {
                self.cell = 0;
                format!("<tr {lines}>")
            }
            Tag::TableCell => {
                let name = if self.head { "th" } else { "td" };
                let align = match self.aligns.get(self.cell) {
                    Some(Alignment::Left) => " class=\"md-left\"",
                    Some(Alignment::Center) => " class=\"md-center\"",
                    Some(Alignment::Right) => " class=\"md-right\"",
                    _ => "",
                };
                self.cell += 1;
                format!("<{name}{align}>")
            }
            Tag::Emphasis => "<em>".to_owned(),
            Tag::Strong => "<strong>".to_owned(),
            Tag::Strikethrough => "<del>".to_owned(),
            Tag::Superscript => "<sup>".to_owned(),
            Tag::Subscript => "<sub>".to_owned(),
            Tag::Link { dest_url, .. } => {
                let open = opens(&dest_url);
                self.links.push(open);
                match open {
                    true => format!(
                        "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">",
                        escape(&dest_url)
                    ),
                    // The target may not be part of the change, and an
                    // anchor would move the page rather than the document.
                    false => format!("<span class=\"md-link\" title=\"{}\">", escape(&dest_url)),
                }
            }
            // An image of the tree is read from the object database. One on
            // another host is a link: opening a review reaches no network.
            Tag::Image {
                dest_url, title, ..
            } => {
                if let Some(path) = resolve(self.dir, &dest_url) {
                    self.image = Some((path, String::new(), title.to_string()));
                    return;
                }
                let open = opens(&dest_url);
                self.links.push(open);
                match open {
                    true => format!(
                        "<a class=\"md-image\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">",
                        escape(&dest_url)
                    ),
                    false => {
                        format!("<span class=\"md-image\" title=\"{}\">", escape(&dest_url))
                    }
                }
            }
            Tag::MetadataBlock(_) => String::new(),
        };
        match inline {
            true => self.inline(&range, html),
            false => self.push(html),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        let inline = matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        );
        if !inline {
            self.close_run();
            self.blocks.pop();
        }
        let html = match tag {
            TagEnd::Paragraph => "</p>",
            TagEnd::Heading(level) => {
                self.push(format!("</{}>", heading(level)));
                return;
            }
            TagEnd::BlockQuote(_) => "</blockquote>",
            TagEnd::CodeBlock => "",
            TagEnd::HtmlBlock => "</pre>",
            TagEnd::List(true) => "</ol>",
            TagEnd::List(false) => "</ul>",
            TagEnd::Item => "</li>",
            TagEnd::FootnoteDefinition => "</div>",
            TagEnd::DefinitionList => "</dl>",
            TagEnd::DefinitionListTitle => "</dt>",
            TagEnd::DefinitionListDefinition => "</dd>",
            TagEnd::Table => "</tbody></table>",
            TagEnd::TableHead => {
                self.head = false;
                "</tr></thead><tbody>"
            }
            TagEnd::TableRow => "</tr>",
            TagEnd::TableCell => match self.head {
                true => "</th>",
                false => "</td>",
            },
            TagEnd::Emphasis => "</em>",
            TagEnd::Strong => "</strong>",
            TagEnd::Strikethrough => "</del>",
            TagEnd::Superscript => "</sup>",
            TagEnd::Subscript => "</sub>",
            TagEnd::Link | TagEnd::Image => match self.links.pop() {
                Some(true) => "</a>",
                _ => "</span>",
            },
            TagEnd::MetadataBlock(_) => "",
        };
        self.push(html.to_owned());
    }

    /// Write inline content, and open a run when it sits directly in an
    /// item: a tight list has no paragraph to carry the lines.
    fn inline(&mut self, range: &Range<usize>, html: String) {
        if self.in_bare_item() {
            match self.run.as_mut() {
                Some((_, covered)) => covered.end = covered.end.max(range.end),
                None => {
                    self.out.push(String::new());
                    self.run = Some((self.out.len() - 1, range.clone()));
                }
            }
        }
        self.push(html);
    }

    /// The last open block is an item or a definition, with no paragraph.
    fn in_bare_item(&self) -> bool {
        self.blocks.last().copied().unwrap_or(false)
    }

    fn close_run(&mut self) {
        if let Some((piece, covered)) = self.run.take() {
            self.out[piece] = format!("<div class=\"md-line\" {}>", self.lines(&covered));
            self.push("</div>".to_owned());
        }
    }

    fn close_image(&mut self, range: &Range<usize>) {
        let Some((path, alt, title)) = self.image.take() else {
            return;
        };
        // The page turns the path into an address of the blob route: only
        // it knows the change and the version being read.
        let mut html = format!(
            "<img class=\"md-img\" data-path=\"{}\" alt=\"{}\" loading=\"lazy\"",
            escape(&path),
            escape(&alt)
        );
        if !title.is_empty() {
            html.push_str(&format!(" title=\"{}\"", escape(&title)));
        }
        html.push('>');
        self.inline(range, html);
    }

    fn code_block(&mut self) {
        let Some((language, code, lines)) = self.code.take() else {
            return;
        };
        let spans = match language.is_empty() {
            true => Vec::new(),
            false => (self.paint)(&language, &code),
        };

        let mut html = format!("<pre class=\"md-code\" {lines}><code>");
        for (number, line) in code.lines().enumerate() {
            if number > 0 {
                html.push('\n');
            }
            html.push_str(&painted(line, spans.get(number).map_or(&[], Vec::as_slice)));
        }
        html.push_str("</code></pre>");
        self.push(html);
    }

    fn push(&mut self, html: String) {
        self.out.push(html);
    }

    /// The `data-lines` attribute of a range of bytes. Lines count from 1.
    fn lines(&self, range: &Range<usize>) -> String {
        let first = self.line_of(range.start);
        let last = self.line_of(range.end.saturating_sub(1).max(range.start));
        format!("data-lines=\"{first}-{last}\"")
    }

    fn line_of(&self, byte: usize) -> usize {
        self.starts.partition_point(|start| *start <= byte)
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// One line of code, with its syntax classes.
fn painted(line: &str, spans: &[Span]) -> String {
    let mut html = String::new();
    let mut last = 0;

    for span in spans {
        let (Some(before), Some(inside)) =
            (line.get(last..span.start), line.get(span.start..span.end))
        else {
            continue;
        };
        html.push_str(&escape(before));
        html.push_str(&format!(
            "<span class=\"{}\">{}</span>",
            escape(&span.cls),
            escape(inside)
        ));
        last = span.end;
    }
    html.push_str(&escape(line.get(last..).unwrap_or_default()));
    html
}

/// The path in the tree of an image that a file in `dir` names, or `None`
/// when the image is not in the tree: on another host, in a `data:` address,
/// or above the top of the tree.
fn resolve(dir: &str, dest: &str) -> Option<String> {
    let dest = dest.split(['#', '?']).next()?;
    // A scheme is a word and a colon before any slash: `https:`, `data:`.
    let scheme = dest
        .find(':')
        .is_some_and(|colon| !dest[..colon].contains('/'));
    if dest.is_empty() || scheme || dest.starts_with("//") {
        return None;
    }
    let dest = unescape(dest);
    // A leading slash names the top of the tree, the way a forge reads it.
    let (base, rest) = match dest.strip_prefix('/') {
        Some(rest) => ("", rest),
        None => (dir, dest.as_str()),
    };

    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Undo the percent escapes of an address: `my%20figure.png`.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Only a link that leaves the page for another host opens.
fn opens(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

fn heading(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

fn alert(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "note",
        BlockQuoteKind::Tip => "tip",
        BlockQuoteKind::Important => "important",
        BlockQuoteKind::Warning => "warning",
        BlockQuoteKind::Caution => "caution",
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> String {
        render(text, "docs", &|_, _| Vec::new())
    }

    #[test]
    fn each_block_carries_its_lines() {
        let html = plain("# Title\n\nOne line,\nand a second.\n\n---\n");

        assert_eq!(
            html,
            "<h1 data-lines=\"1-1\">Title</h1>\
             <p data-lines=\"3-4\">One line,\nand a second.</p>\
             <hr data-lines=\"6-6\">"
        );
    }

    #[test]
    fn a_tight_item_carries_its_own_lines_not_those_of_its_children() {
        let html = plain("- one\n  - two\n- three\n");

        assert_eq!(
            html,
            "<ul><li><div class=\"md-line\" data-lines=\"1-1\">one</div>\
             <ul><li><div class=\"md-line\" data-lines=\"2-2\">two</div></li></ul></li>\
             <li><div class=\"md-line\" data-lines=\"3-3\">three</div></li></ul>"
        );
    }

    #[test]
    fn a_loose_item_leaves_the_lines_to_its_paragraphs() {
        let html = plain("1. one\n\n2. two\n");

        assert_eq!(
            html,
            "<ol start=\"1\"><li><p data-lines=\"1-1\">one</p></li>\
             <li><p data-lines=\"3-3\">two</p></li></ol>"
        );
    }

    #[test]
    fn a_table_row_carries_its_lines() {
        let html = plain("| a | b |\n|:--|--:|\n| 1 | 2 |\n");

        assert_eq!(
            html,
            "<table><thead><tr data-lines=\"1-1\">\
             <th class=\"md-left\">a</th><th class=\"md-right\">b</th></tr></thead>\
             <tbody><tr data-lines=\"3-3\">\
             <td class=\"md-left\">1</td><td class=\"md-right\">2</td></tr></tbody></table>"
        );
    }

    #[test]
    fn raw_html_is_shown_as_text() {
        let html = plain("<script>alert(1)</script>\n\nA <b>bold</b> word.\n");

        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("A &lt;b&gt;bold&lt;/b&gt; word."), "{html}");
    }

    #[test]
    fn only_a_link_to_another_host_opens() {
        let html = plain("[web](https://example.com) [bad](javascript:alert(1)) [doc](other.md)\n");

        assert!(html.contains(
            "<a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">web</a>"
        ));
        assert!(!html.contains("href=\"javascript"), "{html}");
        assert!(html.contains("<span class=\"md-link\" title=\"other.md\">doc</span>"));
    }

    #[test]
    fn an_image_of_the_tree_names_its_path() {
        let html = plain("![a `big` diagram](img/a.png \"The flow\")\n");

        assert_eq!(
            html,
            "<p data-lines=\"1-1\"><img class=\"md-img\" data-path=\"docs/img/a.png\" \
             alt=\"a big diagram\" loading=\"lazy\" title=\"The flow\"></p>"
        );
    }

    #[test]
    fn an_image_on_another_host_is_a_link_and_is_never_fetched() {
        let html = plain("![logo](https://example.com/logo.png) ![x](data:image/png;base64,AA)\n");

        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains(
            "<a class=\"md-image\" href=\"https://example.com/logo.png\" target=\"_blank\" \
             rel=\"noopener noreferrer\">logo</a>"
        ));
        assert!(
            html.contains("<span class=\"md-image\" title=\"data:image/png;base64,AA\">x</span>")
        );
    }

    #[test]
    fn a_path_is_resolved_against_the_directory_of_the_file() {
        assert_eq!(resolve("docs", "a.png").as_deref(), Some("docs/a.png"));
        assert_eq!(
            resolve("docs/guide", "../img/a.png").as_deref(),
            Some("docs/img/a.png")
        );
        assert_eq!(
            resolve("docs", "./a.png#top").as_deref(),
            Some("docs/a.png")
        );
        assert_eq!(resolve("docs", "/img/a.png").as_deref(), Some("img/a.png"));
        assert_eq!(
            resolve("docs", "my%20figure.png").as_deref(),
            Some("docs/my figure.png")
        );
        assert_eq!(resolve("", "a.png").as_deref(), Some("a.png"));
    }

    #[test]
    fn a_path_outside_the_tree_is_not_an_image_of_it() {
        assert_eq!(resolve("docs", "../../a.png"), None);
        assert_eq!(resolve("docs", "https://example.com/a.png"), None);
        assert_eq!(resolve("docs", "//example.com/a.png"), None);
        assert_eq!(resolve("docs", "data:image/png;base64,AA"), None);
        assert_eq!(resolve("docs", ""), None);
    }

    #[test]
    fn a_fenced_block_is_painted_in_the_language_of_its_fence() {
        let paint = |language: &str, code: &str| {
            assert_eq!(language, "c");
            assert_eq!(code, "int a;\nint <b>;\n");
            vec![
                vec![Span {
                    start: 0,
                    end: 3,
                    cls: "tok-storage".to_owned(),
                }],
                Vec::new(),
            ]
        };
        let html = render("Text.\n\n```c\nint a;\nint <b>;\n```\n", "", &paint);

        assert!(html.ends_with(
            "<pre class=\"md-code\" data-lines=\"3-6\"><code>\
             <span class=\"tok-storage\">int</span> a;\nint &lt;b&gt;;</code></pre>"
        ));
    }

    #[test]
    fn a_task_and_an_alert_render() {
        let html = plain("- [x] done\n\n> [!WARNING]\n> Careful.\n");

        assert!(
            html.contains("<span class=\"md-task\">☑</span> done"),
            "{html}"
        );
        assert!(
            html.contains(
                "<blockquote class=\"md-alert md-alert-warning\"><p data-lines=\"4-4\">Careful.</p>"
            ),
            "{html}"
        );
    }
}

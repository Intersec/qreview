//! An AsciiDoc file, rendered as HTML for the reader.
//!
//! `asciidoc-html5` writes the tags, and AsciiDoc lets raw HTML through in a
//! `++++` block or an attribute value. So `ammonia` cleans the output against
//! an allowlist before anything else reads it. See design section 3.8.
//!
//! The converter puts `data-source-line` on the outer element of each block.
//! The tree of the parser gives the last line of the block, and this module
//! turns the pair into `data-lines`, with the marks of the diff, the way
//! `markdown.rs` writes them.

use std::collections::{HashMap, HashSet, VecDeque};

use asciidoc_html5::{Options, SafeMode};
use asciidoc_parser::HasSpan;
use asciidoc_parser::blocks::{Block, FindBlocks, IsBlock};

use crate::markdown::{Changes, Plan, Removed, escape, opens, painted, resolve};
use crate::model::Span;

/// Render an AsciiDoc text, with the marks of what the diff changed.
///
/// `dir` is the directory of the file in the tree, which a relative image
/// is read from. `paint` colors a source block: it takes the language of the
/// block and the code, and answers the spans of each line, in bytes.
pub fn render(
    text: &str,
    dir: &str,
    changes: &Changes,
    paint: &dyn Fn(&str, &str) -> Vec<Vec<Span>>,
) -> String {
    // `secure` turns an include into a link: nothing is read beside the blob.
    let options = Options::new()
        .safe_mode(SafeMode::Secure)
        .embedded(true)
        .set("showtitle")
        .source_locations(true);
    let document = asciidoc_html5::load_with(text, &options);

    let lines: Vec<&str> = text.lines().collect();
    let mut places = HashMap::new();
    walk(document.child_blocks(), false, &lines, &mut places);

    // The title of the document has no anchor. Its header runs to the first
    // block, less the blank lines.
    let header = lines
        .first()
        .filter(|line| line.starts_with("= "))
        .map(|_| {
            let mut end = document
                .child_blocks()
                .next()
                .map_or(lines.len(), |block| block.span().line() - 1);
            while end > 1 && lines.get(end - 1).is_some_and(|l| l.trim().is_empty()) {
                end -= 1;
            }
            (1, end)
        });

    let mut leaves: Vec<(usize, usize)> = places
        .values()
        .filter_map(|place| match *place {
            Place::Leaf { first, last, .. } | Place::Title { first, last } => Some((first, last)),
            Place::None => None,
        })
        .collect();
    leaves.extend(header);
    leaves.sort_unstable();
    leaves.dedup();
    let plan = Plan::new(changes, &leaves);

    let html = clean(&asciidoc_html5::convert_document_with(&document, &options));
    Rewriter {
        out: String::with_capacity(html.len() * 5 / 4),
        places: &places,
        plan: &plan,
        loose: plan.loose.iter().cloned().collect(),
        lines: &lines,
        dir,
        paint,
        title: header,
        leaf: None,
        pre: None,
        span_links: 0,
    }
    .run(&html)
}

/// What the pane does with a block that the HTML anchors.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    /// A block of its own. A code block also says so: its rows follow the
    /// lines of the source one for one.
    Leaf {
        first: usize,
        last: usize,
        code: Option<Code>,
    },
    /// A section: its title is the block, and it stands on the heading.
    Title { first: usize, last: usize },
    /// A container, whose children carry the lines, or a block inside a
    /// list, which is one block for the pane.
    None,
}

/// A code block: a listing or a literal.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Code {
    /// The block has the style `mermaid`: the page draws it.
    mermaid: bool,
}

/// The `<pre>` of a code block, written but not closed yet.
struct Pre {
    /// Where its tag stands in `out`.
    tag: std::ops::Range<usize>,
    attrs: Vec<(String, String)>,
    /// Where its text starts in the HTML.
    text: usize,
}

/// The kind of element that an anchor stands on.
///
/// A container and its first child can start on the same line, so the line
/// alone does not say which block an anchor belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Preamble,
    Section,
    List,
    Other,
}

/// Find the place of each block that the HTML anchors, by its first line and
/// its kind.
fn walk<'a>(
    blocks: impl Iterator<Item = &'a Block<'a>>,
    in_list: bool,
    lines: &[&str],
    places: &mut HashMap<(usize, Kind), Place>,
) {
    for block in blocks {
        let span = block.span();
        let first = span.line();
        let last = first + span.data().trim_end().lines().count().max(1) - 1;
        let context = block.raw_context();
        let children: Vec<&Block> = block.child_blocks().collect();

        let (kind, place) = match context.as_ref() {
            _ if in_list => (kind_of(&context), Place::None),
            "preamble" => (Kind::Preamble, Place::None),
            "section" => {
                // The title runs to the first child, less the blank lines.
                let mut end = children
                    .first()
                    .map_or(last, |child| child.span().line() - 1);
                while end > first && lines.get(end - 1).is_some_and(|l| l.trim().is_empty()) {
                    end -= 1;
                }
                (Kind::Section, Place::Title { first, last: end })
            }
            // A list is one block: its items carry no anchor of their own.
            "list" => (
                Kind::List,
                Place::Leaf {
                    first,
                    last,
                    code: None,
                },
            ),
            _ if !children.is_empty() => (Kind::Other, Place::None),
            "listing" | "literal" => (
                Kind::Other,
                Place::Leaf {
                    first,
                    last,
                    code: Some(Code {
                        mermaid: block.declared_style() == Some("mermaid"),
                    }),
                },
            ),
            _ => (
                Kind::Other,
                Place::Leaf {
                    first,
                    last,
                    code: None,
                },
            ),
        };
        // A block of its own wins over a container on the same line.
        let slot = places.entry((first, kind)).or_insert(place);
        if *slot == Place::None {
            *slot = place;
        }
        walk(
            children.into_iter(),
            in_list || context.as_ref() == "list",
            lines,
            places,
        );
    }
}

fn kind_of(context: &str) -> Kind {
    match context {
        "list" => Kind::List,
        "section" => Kind::Section,
        "preamble" => Kind::Preamble,
        _ => Kind::Other,
    }
}

/// Keep the tags and the attributes that a document needs, and nothing that
/// runs, styles the page, asks for input, or plays.
///
/// The attributes the pane reads (`data-lines`, `data-change`, `data-old`,
/// `data-code`) are not on the list, so a file cannot write them.
fn clean(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["class", "id", "data-source-line", "data-lang"])
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(None)
        .clean(html)
        .to_string()
}

/// Rewrites the clean HTML: the lines and the marks of each block, the
/// colors of the code, the images of the tree, and the links.
///
/// `ammonia` writes well formed HTML: every attribute in double quotes, and
/// `<` and `>` escaped in the text. So a scan over the tags is enough.
struct Rewriter<'a> {
    out: String,
    places: &'a HashMap<(usize, Kind), Place>,
    plan: &'a Plan,
    /// The runs of `plan.loose` not written yet.
    loose: VecDeque<Removed>,
    lines: &'a [&'a str],
    dir: &'a str,
    paint: &'a dyn Fn(&str, &str) -> Vec<Vec<Span>>,
    /// A title that waits for its heading: the one of the document, then the
    /// one of each section.
    title: Option<(usize, usize)>,
    /// The code block being read, and its last line.
    leaf: Option<(Code, usize)>,
    pre: Option<Pre>,
    /// How many `<a>` were written as a `<span>` and are still open.
    span_links: usize,
}

impl Rewriter<'_> {
    fn run(mut self, html: &str) -> String {
        let mut at = 0;
        while let Some(open) = html[at..].find('<').map(|i| at + i) {
            self.out.push_str(&html[at..open]);
            let close = tag_end(html, open);
            let tag = &html[open..close];
            at = close;

            if tag.starts_with("</") {
                at = self.end(html, tag, at);
            } else {
                at = self.start(html, tag, at);
            }
        }
        self.out.push_str(&html[at..]);
        self.markers(usize::MAX);
        self.out
    }

    /// Write an opening tag. Answers where the scan goes on, past the text
    /// of a code block that it painted.
    fn start(&mut self, html: &str, tag: &str, at: usize) -> usize {
        let (name, mut attrs) = parse(tag);

        if let Some(line) = take(&mut attrs, "data-source-line").and_then(|v| v.parse().ok()) {
            let kind = anchor_kind(&attrs);
            match self.places.get(&(line, kind)).copied() {
                Some(Place::Leaf { first, last, code }) => {
                    self.markers(first);
                    self.mark(&mut attrs, first, last);
                    self.leaf = code.map(|code| (code, last));
                }
                Some(Place::Title { first, last }) => self.title = Some((first, last)),
                Some(Place::None) | None => {}
            }
        }
        let heading =
            name.len() == 2 && name.starts_with('h') && name.as_bytes()[1].is_ascii_digit();
        if heading && let Some((first, last)) = self.title.take() {
            self.markers(first);
            self.mark(&mut attrs, first, last);
        }

        match name.as_str() {
            "pre" if self.leaf.is_some() => {
                add_class(&mut attrs, "md-code");
                if self.leaf.is_some_and(|(code, _)| code.mermaid) {
                    add_class(&mut attrs, "md-mermaid");
                }
                // The tag is written again once the rows are counted.
                let start = self.out.len();
                self.out.push_str(&write(&name, &attrs));
                self.pre = Some(Pre {
                    tag: start..self.out.len(),
                    attrs,
                    text: at,
                });
            }
            "code" if self.pre.is_some() => {
                let language = attrs
                    .iter()
                    .find(|(key, _)| key == "data-lang")
                    .map(|(_, value)| unescape(value));
                self.out.push_str(&write(&name, &attrs));
                match language.as_deref() {
                    // A `[source,mermaid]` block says so only here. The page
                    // draws it in place of its code.
                    Some("mermaid") => {
                        if let Some(pre) = self.pre.as_mut() {
                            add_class(&mut pre.attrs, "md-mermaid");
                        }
                    }
                    Some(language) => return self.code(html, at, language),
                    None => {}
                }
            }
            "img" => self.image(&attrs),
            "a" => self.link(&name, attrs),
            _ => self.out.push_str(&write(&name, &attrs)),
        }
        at
    }

    fn end(&mut self, html: &str, tag: &str, at: usize) -> usize {
        match tag {
            "</a>" if self.span_links > 0 => {
                self.span_links -= 1;
                self.out.push_str("</span>");
            }
            "</pre>" => {
                if let (Some(mut pre), Some((_, last))) = (self.pre.take(), self.leaf.take()) {
                    let rows = rows(&html[pre.text..at - tag.len()]);
                    let first = self.first_row(last, rows);
                    pre.attrs.push(("data-code".to_owned(), first.to_string()));
                    self.out.replace_range(pre.tag, &write("pre", &pre.attrs));
                }
                self.out.push_str(tag);
            }
            _ => self.out.push_str(tag),
        }
        at
    }

    /// Paint the code of a source block, up to its `</code>`.
    fn code(&mut self, html: &str, at: usize, language: &str) -> usize {
        let Some(end) = html[at..].find("</code>").map(|i| at + i) else {
            return at;
        };
        let inner = &html[at..end];
        // A callout is a tag in the code: the text is left as it is.
        if inner.contains('<') {
            return at;
        }
        let code = unescape(inner);
        let spans = (self.paint)(language, &code);
        for (number, line) in code.split('\n').enumerate() {
            if number > 0 {
                self.out.push('\n');
            }
            self.out
                .push_str(&painted(line, spans.get(number).map_or(&[], Vec::as_slice)));
        }
        end
    }

    /// The line of the source that the first row of a code block shows.
    fn first_row(&self, last: usize, rows: usize) -> usize {
        // A delimited block ends on its delimiter, which is not a row.
        let closed = self.lines.get(last - 1).is_some_and(|line| delimiter(line));
        let below = usize::from(closed);
        (last + 1).saturating_sub(rows + below).max(1)
    }

    /// An image of the tree is read from the object database, as in a
    /// Markdown file. One on another host is a link.
    fn image(&mut self, attrs: &[(String, String)]) {
        let get = |key: &str| {
            attrs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| unescape(value))
                .unwrap_or_default()
        };
        let (src, alt) = (get("src"), get("alt"));

        if let Some(path) = resolve(self.dir, &src) {
            let mut html = format!(
                "<img class=\"md-img\" data-path=\"{}\" alt=\"{}\" loading=\"lazy\"",
                escape(&path),
                escape(&alt)
            );
            for key in ["width", "height"] {
                let value = get(key);
                if !value.is_empty() {
                    html.push_str(&format!(" {key}=\"{}\"", escape(&value)));
                }
            }
            html.push('>');
            self.out.push_str(&html);
        } else if opens(&src) {
            self.out.push_str(&format!(
                "<a class=\"md-image\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                escape(&src),
                escape(&alt)
            ));
        } else {
            self.out.push_str(&format!(
                "<span class=\"md-image\" title=\"{}\">{}</span>",
                escape(&src),
                escape(&alt)
            ));
        }
    }

    /// Only a link that leaves the page for another host opens, as in a
    /// Markdown file. The target of any other may not be part of the change.
    fn link(&mut self, name: &str, mut attrs: Vec<(String, String)>) {
        let Some(href) = take(&mut attrs, "href") else {
            self.out.push_str(&write(name, &attrs));
            return;
        };
        let href = unescape(&href);
        if opens(&href) {
            attrs.push(("href".to_owned(), escape(&href)));
            attrs.push(("target".to_owned(), "_blank".to_owned()));
            attrs.push(("rel".to_owned(), "noopener noreferrer".to_owned()));
            self.out.push_str(&write(name, &attrs));
        } else {
            self.span_links += 1;
            self.out.push_str(&format!(
                "<span class=\"md-link\" title=\"{}\">",
                escape(&href)
            ));
        }
    }

    /// Put the lines of a block and its mark on its attributes.
    fn mark(&self, attrs: &mut Vec<(String, String)>, first: usize, last: usize) {
        attrs.push(("data-lines".to_owned(), format!("{first}-{last}")));
        if let Some((kind, old)) = self.plan.marks.get(&(first, last)) {
            attrs.push(("data-change".to_owned(), (*kind).to_owned()));
            if let Some((from, to)) = old {
                attrs.push(("data-old".to_owned(), format!("{from}-{to}")));
            }
        }
    }

    /// Write the marker of each run that no block owns and that comes
    /// before `line`.
    fn markers(&mut self, line: usize) {
        while self.loose.front().is_some_and(|run| run.after < line) {
            if let Some(run) = self.loose.pop_front() {
                self.out.push_str(&format!("<div {}></div>", run.attrs()));
            }
        }
    }
}

/// The kind of block an anchored element stands for, from its id and class.
fn anchor_kind(attrs: &[(String, String)]) -> Kind {
    let get = |key: &str| {
        attrs
            .iter()
            .find(|(name, _)| name == key)
            .map_or("", |(_, value)| value.as_str())
    };
    if get("id") == "preamble" {
        return Kind::Preamble;
    }
    let classes: Vec<&str> = get("class").split_whitespace().collect();
    let section = |class: &&str| {
        class
            .strip_prefix("sect")
            .is_some_and(|level| level.len() == 1 && level.as_bytes()[0].is_ascii_digit())
    };
    if classes.iter().any(section) {
        Kind::Section
    } else if classes.iter().any(|class| {
        matches!(
            *class,
            "ulist" | "olist" | "dlist" | "colist" | "hdlist" | "qlist"
        )
    }) {
        Kind::List
    } else {
        Kind::Other
    }
}

/// Whether a line delimits a block: `----`, `....`, `++++` and the like.
fn delimiter(line: &str) -> bool {
    let line = line.trim_end();
    if line.starts_with("```") {
        return true;
    }
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    line.len() >= 4 && "-.+_=*".contains(first) && chars.all(|c| c == first)
}

/// How many rows a code block shows: its text, less the tags in it.
fn rows(inner: &str) -> usize {
    inner.matches('\n').count() + 1
}

/// Where the tag that opens at `open` ends, past its `>`. A `>` inside a
/// quoted value does not end it.
fn tag_end(html: &str, open: usize) -> usize {
    let mut quoted = false;
    for (i, c) in html[open..].char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return open + i + 1,
            _ => {}
        }
    }
    html.len()
}

/// The name and the attributes of an opening tag. The values stay escaped.
fn parse(tag: &str) -> (String, Vec<(String, String)>) {
    let body = tag
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/');
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = body[..name_end].to_owned();
    let mut attrs = Vec::new();
    let mut rest = body[name_end..].trim_start();

    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_owned();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix("=\"") {
            let end = after.find('"').unwrap_or(after.len());
            value = after[..end].to_owned();
            rest = after.get(end + 1..).unwrap_or_default();
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
        rest = rest.trim_start();
    }
    (name, attrs)
}

fn write(name: &str, attrs: &[(String, String)]) -> String {
    let mut html = format!("<{name}");
    for (key, value) in attrs {
        html.push_str(&format!(" {key}=\"{value}\""));
    }
    html.push('>');
    html
}

fn take(attrs: &mut Vec<(String, String)>, key: &str) -> Option<String> {
    let index = attrs.iter().position(|(name, _)| name == key)?;
    Some(attrs.remove(index).1)
}

fn add_class(attrs: &mut Vec<(String, String)>, class: &str) {
    match attrs.iter_mut().find(|(name, _)| name == "class") {
        Some((_, value)) => {
            value.push(' ');
            value.push_str(class);
        }
        None => attrs.push(("class".to_owned(), class.to_owned())),
    }
}

/// Undo the escapes that `ammonia` writes in text and in a value.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> String {
        render(text, "docs", &Changes::default(), &|_, _| Vec::new())
    }

    #[test]
    fn each_block_carries_its_lines() {
        let html = plain("= Title\n\nA paragraph,\non two lines.\n\n== Part\n\nMore.\n");

        assert!(
            html.contains("data-lines=\"3-4\">\n<p>A paragraph,\non two lines.</p>"),
            "{html}"
        );
        assert!(
            html.contains("<h2 id=\"_part\" data-lines=\"6-6\">Part</h2>"),
            "{html}"
        );
        assert!(html.contains("data-lines=\"8-8\">\n<p>More.</p>"), "{html}");
        assert!(!html.contains("data-source-line"), "{html}");
    }

    #[test]
    fn the_title_of_the_document_carries_its_header() {
        let html = plain("= Title\n:toc!:\n\nText.\n");

        assert!(html.contains("<h1 data-lines=\"1-2\">Title</h1>"), "{html}");
    }

    #[test]
    fn a_list_is_one_block() {
        let html = plain("* one\n** two\n* three\n");

        assert!(
            html.contains("<div class=\"ulist\" data-lines=\"1-3\">"),
            "{html}"
        );
        assert_eq!(html.matches("data-lines").count(), 1, "{html}");
    }

    #[test]
    fn a_table_is_one_block_with_its_title() {
        let html = plain(".Sizes\n|===\n| Name | Size\n\n| a.c\n| 12\n|===\n");

        assert!(html.contains("data-lines=\"1-7\""), "{html}");
    }

    #[test]
    fn a_source_block_is_painted_and_knows_its_first_row() {
        let paint = |language: &str, code: &str| {
            assert_eq!((language, code), ("c", "int a;\nint <b>;"));
            vec![
                vec![Span {
                    start: 0,
                    end: 3,
                    cls: "tok-storage".to_owned(),
                }],
                Vec::new(),
            ]
        };
        let html = render(
            "Text.\n\n[source,c]\n----\nint a;\nint <b>;\n----\n",
            "",
            &Changes::default(),
            &paint,
        );

        assert!(html.contains("data-lines=\"3-7\""), "{html}");
        assert!(
            html.contains(
                "<pre class=\"highlight md-code\" data-code=\"5\"><code class=\"language-c\" \
                 data-lang=\"c\"><span class=\"tok-storage\">int</span> a;\nint &lt;b&gt;;</code></pre>"
            ),
            "{html}"
        );
    }

    #[test]
    fn a_mermaid_block_is_left_for_the_page_to_draw() {
        let paint = |_: &str, _: &str| -> Vec<Vec<Span>> { panic!("a diagram is not painted") };
        let text = "[mermaid]\n....\ngraph LR\n  A --> B\n....\n\n\
                    [source,mermaid]\n----\ngraph TD\n----\n";
        let html = render(text, "", &Changes::default(), &paint);

        assert!(
            html.contains(
                "<pre class=\"md-code md-mermaid\" data-code=\"3\">graph LR\n  A --&gt; B</pre>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<pre class=\"highlight md-code md-mermaid\" data-code=\"9\">\
                 <code class=\"language-mermaid\" data-lang=\"mermaid\">graph TD</code></pre>"
            ),
            "{html}"
        );
    }

    #[test]
    fn raw_html_cannot_run_or_fake_a_mark() {
        let html = plain(
            "++++\n<script>alert(1)</script>\n<p data-lines=\"1-1\" data-change=\"add\" \
             onclick=\"x()\" style=\"color: red\">Hi</p>\n++++\n",
        );

        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("style="), "{html}");
        assert!(!html.contains("data-change"), "{html}");
        assert!(!html.contains("data-lines=\"1-1\""), "{html}");
        assert!(html.contains("Hi"), "{html}");
    }

    #[test]
    fn an_include_is_a_link_and_reads_nothing() {
        let html = plain("include::/etc/passwd[]\n");

        assert!(!html.contains("root:"), "{html}");
        assert!(html.contains("<span class=\"md-link\""), "{html}");
    }

    #[test]
    fn an_image_of_the_tree_is_named_by_its_path() {
        let html =
            plain("image::img/flow.svg[The flow]\n\nimage::https://example.com/a.png[Far]\n");

        assert!(
            html.contains("<img class=\"md-img\" data-path=\"docs/img/flow.svg\" alt=\"The flow\""),
            "{html}"
        );
        assert!(
            html.contains(
                "<a class=\"md-image\" href=\"https://example.com/a.png\" target=\"_blank\" \
                 rel=\"noopener noreferrer\">Far</a>"
            ),
            "{html}"
        );
        assert!(!html.contains("src="), "{html}");
    }

    #[test]
    fn a_link_opens_only_toward_another_host() {
        let html = plain("See https://example.com[the site] and xref:other.adoc[the other].\n");

        assert!(
            html.contains(
                "<a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">the site</a>"
            ),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"md-link\" title=\"other.html\">the other</span>"),
            "{html}"
        );
    }

    #[test]
    fn the_blocks_the_change_touches_are_marked() {
        let text = "= Title\n\nOne.\n\n== Part\n\nTwo,\nthree.\n";
        let changes = Changes::listed(&[5, 8], &[(3, 4, 6)]);
        let html = render(text, "", &changes, &|_, _| Vec::new());

        assert!(
            html.contains("<h2 id=\"_part\" data-lines=\"5-5\" data-change=\"add\">"),
            "{html}"
        );
        assert!(
            html.contains("data-lines=\"7-8\" data-change=\"modify\">"),
            "{html}"
        );
        // No block took the place of the lines removed after line 3.
        let marker = html.find("class=\"md-removed\"").expect(&html);
        assert!(marker < html.find("<h2").unwrap(), "{html}");
    }

    #[test]
    fn lines_removed_between_two_blocks_leave_a_marker() {
        let changes = Changes::listed(&[], &[(2, 3, 5)]);
        let html = render("One.\n\nTwo.\n", "", &changes, &|_, _| Vec::new());

        let marker = "<div class=\"md-removed\" data-old=\"3-5\" title=\"3 lines removed\"></div>";
        let at = html.find(marker).expect(&html);
        assert!(at < html.find("Two.").unwrap(), "{html}");
    }
}

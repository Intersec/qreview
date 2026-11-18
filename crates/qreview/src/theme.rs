//! The color themes of the user.
//!
//! A theme is a CSS file in the themes directory of the user, but only a
//! strict subset of CSS: one `:root` block, its base, and hex colors for
//! the tokens of the stylesheet. The file is parsed into a map, and the page
//! builds its own rule from that map. The text of the file never reaches
//! the page, which holds the session token.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use cssparser::{ParseError, ParseErrorKind, Parser, ToCss, Token};
use serde::Serialize;

/// The values of `ui.theme` that name no file.
pub const BUILT_IN: [&str; 3] = ["system", "light", "dark"];

/// The color tokens of `web/src/styles/main.css`, without `--`.
///
/// A test holds this list in step with the stylesheet. Renaming a token
/// breaks the theme files that set it, so a rename goes in the changelog.
pub const TOKENS: &[&str] = &[
    "page",
    "pane",
    "ink",
    "ink-quiet",
    "ink-faint",
    "edge",
    "edge-soft",
    "pick",
    "pick-edge",
    "add-line",
    "add-word",
    "remove-line",
    "remove-word",
    "rebase-add-line",
    "rebase-add-word",
    "rebase-remove-line",
    "rebase-remove-word",
    "same-word",
    "context-bar",
    "context-ink",
    "draft",
    "draft-edge",
    "action",
    "agent",
    "agent-edge",
    "waiting",
    "waiting-edge",
    "waiting-ink",
    "online",
    "merge-bar",
    "merge-edge",
    "merge-ink",
    "add-ink",
    "remove-ink",
    "ok-ink",
    "error-ink",
    "tok-invalid",
    "tok-invalid-ink",
    "shadow",
    "shadow-strong",
    "veil",
    "tok-keyword",
    "tok-string",
    "tok-comment",
    "tok-constant",
    "tok-entity",
    "tok-support",
    "tok-variable",
];

/// What a theme file may hold, said once after the first error of a run.
pub const HINT: &str = "A theme file holds one \":root { }\" block and comments. Inside it: \
     \"color-scheme: light;\" or \"color-scheme: dark;\", then \"--<token>: #rrggbb;\" lines.";

/// A theme file larger than this is not a list of colors.
const MAX_SIZE: u64 = 64 * 1024;

/// The built-in palette that a theme starts from.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Theme {
    pub base: Base,
    /// Token without `--`, to a lowercase hex color.
    pub colors: BTreeMap<String, String>,
}

/// The first thing wrong in a theme file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// 1-based, when the problem has a place in the file.
    pub line: Option<u32>,
    pub what: String,
}

impl Problem {
    fn at(line: u32, what: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            what: what.into(),
        }
    }

    fn whole(what: impl Into<String>) -> Self {
        Self {
            line: None,
            what: what.into(),
        }
    }
}

/// A theme file left out, as the terminal says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    pub path: PathBuf,
    pub problem: Problem,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "theme {}", self.path.display())?;
        if let Some(line) = self.problem.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.problem.what)
    }
}

/// Read every theme of the directory. A file that is wrong is left out and
/// said, and the others still load: a theme never stops the tool.
pub fn load_dir(dir: &Path) -> (BTreeMap<String, Theme>, Vec<FileError>) {
    let mut themes = BTreeMap::new();
    let mut errors = Vec::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        return (themes, errors);
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        let name = match name_of(&path) {
            Ok(Some(name)) => name,
            Ok(None) => continue,
            Err(problem) => {
                errors.push(FileError { path, problem });
                continue;
            }
        };
        match read(&path) {
            Ok(theme) => {
                themes.insert(name, theme);
            }
            Err(problem) => errors.push(FileError { path, problem }),
        }
    }
    (themes, errors)
}

/// The theme that a file names, or `None` for a file that is no theme.
pub fn name_of(path: &Path) -> Result<Option<String>, Problem> {
    let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
        return Ok(None);
    };
    if file.starts_with('.') || !path.is_file() {
        return Ok(None);
    }
    let Some(name) = file.strip_suffix(".css") else {
        return Ok(None);
    };

    let valid = name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(Problem::whole(format!(
            "`{name}` is not a theme name: use lowercase letters, digits and `-`"
        )));
    }
    if BUILT_IN.contains(&name) {
        return Err(Problem::whole(format!(
            "`{name}` is a built-in theme, so no file may be named so"
        )));
    }
    Ok(Some(name.to_owned()))
}

fn read(path: &Path) -> Result<Theme, Problem> {
    use std::io::Read;

    let unreadable = |e: std::io::Error| Problem::whole(format!("cannot read the file: {e}"));
    // The bound holds on what is read, not on what the metadata says: a
    // file can grow, and some files report a size of zero.
    // Bytes first: the cut at the bound can fall inside a character, and
    // the size must be said before the encoding is checked.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_SIZE + 1).read_to_end(&mut bytes))
        .map_err(unreadable)?;
    if bytes.len() as u64 > MAX_SIZE {
        return Err(Problem::whole("the file is larger than 64 KiB"));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| Problem::whole("cannot read the file: it is not UTF-8 text"))?;
    parse(&text)
}

/// Parse the text of a theme file.
pub fn parse(text: &str) -> Result<Theme, Problem> {
    // An editor may save a byte order mark. A browser drops it, so do we.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser::new(text);
    parse_sheet(&mut parser).map_err(problem_of)
}

type Failure = ParseError<Problem>;

/// The line of the next token. Whitespace and comments are skipped first,
/// so the line is the one of the token and not of the end of the last one.
fn line_of(parser: &mut Parser) -> u32 {
    parser.skip_whitespace();
    parser.current_source_location().line + 1
}

fn fail(line: u32, what: impl Into<String>) -> Failure {
    custom(Problem::at(line, what))
}

fn custom(problem: Problem) -> Failure {
    ParseError {
        kind: ParseErrorKind::Custom(problem),
    }
}

fn problem_of(error: Failure) -> Problem {
    match error.kind {
        ParseErrorKind::Custom(problem) => problem,
        // Only the nesting limit of the parser lands here: every token the
        // theme reads is matched by hand.
        ParseErrorKind::Basic(_) => Problem::whole("this is not CSS that a theme can hold"),
    }
}

/// What came instead, as the error says it. Inside the block, the parser
/// reads the closing brace as the end, so `end` names what ended.
fn found(token: Option<&Token>, end: &str) -> String {
    match token {
        Some(token) => format!("`{}`", token.to_css_string()),
        None => format!("the end of the {end}"),
    }
}

fn parse_sheet<'i>(parser: &mut Parser<'i>) -> Result<Theme, Failure> {
    let line = line_of(parser);
    let first = parser.next().ok().cloned();
    if let Some(Token::AtKeyword(name)) = &first {
        return Err(fail(line, format!("`@{name}` is not allowed in a theme")));
    }
    let opened = first == Some(Token::Colon)
        && matches!(parser.next(), Ok(Token::Ident(root)) if root.eq_ignore_ascii_case("root"))
        && matches!(parser.next(), Ok(Token::CurlyBracketBlock));
    if !opened {
        let line = line_of(parser);
        return Err(fail(
            line,
            format!(
                "expected `:root {{`, found {}",
                found(first.as_ref(), "file")
            ),
        ));
    }

    let theme = parser.parse_nested_block(parse_block)?;

    let line = line_of(parser);
    if let Ok(token) = parser.next() {
        let token = token.clone();
        return Err(fail(
            line,
            format!(
                "expected nothing after the `:root` block, found {}",
                found(Some(&token), "file")
            ),
        ));
    }
    Ok(theme)
}

fn parse_block<'i>(parser: &mut Parser<'i>) -> Result<Theme, Failure> {
    let mut base = None;
    let mut colors = BTreeMap::new();
    // The line of each name, to say where it was set the first time.
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();

    loop {
        let line = line_of(parser);
        let name = match parser.next() {
            Err(_) => break,
            Ok(Token::Semicolon) => continue,
            Ok(Token::Ident(name)) => name.to_string(),
            Ok(Token::AtKeyword(name)) => {
                let what = format!("`@{name}` is not allowed in a theme");
                return Err(fail(line, what));
            }
            Ok(other) => {
                let what = format!(
                    "expected a property, found {}",
                    found(Some(other), "`:root` block")
                );
                return Err(fail(line, what));
            }
        };

        if !matches!(parser.next(), Ok(Token::Colon)) {
            return Err(fail(line, format!("expected `:` after `{name}`")));
        }

        let key = if name.eq_ignore_ascii_case("color-scheme") {
            "color-scheme".to_owned()
        } else {
            name.clone()
        };
        if let Some(first) = seen.insert(key.clone(), line) {
            let what = format!("`{key}` is set twice, first on line {first}");
            return Err(fail(line, what));
        }

        let value = parser.next().ok().cloned();
        if key == "color-scheme" {
            base = Some(match &value {
                Some(Token::Ident(v)) if v.eq_ignore_ascii_case("light") => Base::Light,
                Some(Token::Ident(v)) if v.eq_ignore_ascii_case("dark") => Base::Dark,
                other => {
                    let what = format!(
                        "`color-scheme` takes `light` or `dark`, found {}",
                        found(other.as_ref(), "`:root` block")
                    );
                    return Err(fail(line, what));
                }
            });
        } else if let Some(token) = key.strip_prefix("--") {
            if !TOKENS.contains(&token) {
                let what = format!(
                    "`--{token}` is not a color token. The tokens are: --{}",
                    TOKENS.join(", --")
                );
                return Err(fail(line, what));
            }
            let Some(color) = value.as_ref().and_then(hex) else {
                let what = format!(
                    "`--{token}` takes a hex color such as #1e1e2e, found {}",
                    found(value.as_ref(), "`:root` block")
                );
                return Err(fail(line, what));
            };
            colors.insert(token.to_owned(), color);
        } else {
            let what = format!("`{name}` is not a property a theme can set");
            return Err(fail(line, what));
        }

        match parser.next() {
            Err(_) | Ok(Token::Semicolon) => {}
            Ok(other) => {
                let what = format!(
                    "`{key}` takes one value, then `;`. Found {} after it",
                    found(Some(other), "`:root` block")
                );
                return Err(fail(line, what));
            }
        }
    }

    let Some(base) = base else {
        return Err(custom(Problem::whole(
            "`color-scheme: light;` or `color-scheme: dark;` is missing",
        )));
    };
    Ok(Theme { base, colors })
}

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`, in lowercase.
fn hex(token: &Token) -> Option<String> {
    let (Token::Hash(digits) | Token::IDHash(digits)) = token else {
        return None;
    };
    let ok = matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then(|| format!("#{}", digits.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_is_read() {
        let theme = parse(
            "/* A night of ink. */\n\
             :root {\n\
               color-scheme: dark;\n\
               --page: #282A36; /* the page */\n\
               --ink: #fff;\n\
               --pane: #1e1e2e80;\n\
               --edge: #abcd\n\
             }\n",
        )
        .unwrap();

        assert_eq!(theme.base, Base::Dark);
        let colors: Vec<_> = theme
            .colors
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        assert_eq!(
            colors,
            ["edge=#abcd", "ink=#fff", "page=#282a36", "pane=#1e1e2e80"]
        );
    }

    #[test]
    fn a_byte_order_mark_is_dropped() {
        let theme = parse("\u{feff}:root { color-scheme: dark; }").unwrap();

        assert_eq!(theme.base, Base::Dark);
    }

    #[test]
    fn a_theme_with_no_colors_is_its_base() {
        let theme = parse(":root { color-scheme: light }").unwrap();

        assert_eq!(theme.base, Base::Light);
        assert!(theme.colors.is_empty());
    }

    #[test]
    fn the_errors_are_pinned() {
        // One case per error, and the ways CSS could reach out of the page.
        let cases = [
            ("empty", ""),
            ("no root", "body { color-scheme: dark; }"),
            ("import", "@import url(x.css);"),
            ("at-rule inside", ":root {\n  @media print {}\n}"),
            (
                "second selector",
                ":root { color-scheme: dark; }\nbody { color: red; }",
            ),
            (
                "attribute selector",
                ":root { color-scheme: dark; }\n[data-x^='a'] { background: url(x); }",
            ),
            ("unknown property", ":root {\n  color: red;\n}"),
            ("unknown token", ":root {\n  --font: #fff;\n}"),
            (
                "url",
                ":root {\n  color-scheme: dark;\n  --page: url(x);\n}",
            ),
            ("image-set", ":root {\n  --page: image-set('x' 1x);\n}"),
            ("named color", ":root {\n  --page: red;\n}"),
            ("rgb", ":root {\n  --page: rgb(0 0 0);\n}"),
            ("short hex", ":root {\n  --page: #12;\n}"),
            ("not hex", ":root {\n  --page: #gggggg;\n}"),
            ("important", ":root {\n  --page: #fff !important;\n}"),
            ("two values", ":root {\n  --page: #fff #000;\n}"),
            (
                "escape",
                ":root {\n  color-scheme: dark;\n  --page: #fff;}body{x:y\n}",
            ),
            ("closing style", ":root {\n  --page: </style>;\n}"),
            ("bad scheme", ":root {\n  color-scheme: dim;\n}"),
            ("no scheme", ":root {\n  --page: #fff;\n}"),
            (
                "set twice",
                ":root {\n  color-scheme: dark;\n  --page: #fff;\n  --page: #000;\n}",
            ),
            ("no colon", ":root {\n  --page #fff;\n}"),
            ("empty value", ":root {\n  color-scheme: dark;\n  --page: }"),
        ];

        let report: String = cases
            .iter()
            .map(|(label, text)| {
                let problem = parse(text).unwrap_err();
                let line = problem.line.map_or("-".to_owned(), |l| l.to_string());
                format!("{label}\n  {line}: {}\n", problem.what)
            })
            .collect();
        insta::assert_snapshot!(report);
    }

    #[test]
    fn the_names_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Night.css", "light.css", "a b.css"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }

        let night = name_of(&dir.path().join("Night.css")).unwrap_err();
        assert!(
            night.what.contains("`Night` is not a theme name"),
            "{night:?}"
        );
        let light = name_of(&dir.path().join("light.css")).unwrap_err();
        assert!(light.what.contains("built-in"), "{light:?}");
        assert!(name_of(&dir.path().join("a b.css")).is_err());
    }

    #[test]
    fn load_dir_skips_dotfiles_subdirectories_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let good = ":root { color-scheme: dark; }";
        std::fs::write(dir.path().join("night.css"), good).unwrap();
        std::fs::write(dir.path().join(".night.css.swp"), "junk").unwrap();
        std::fs::write(dir.path().join(".hidden.css"), "junk").unwrap();
        std::fs::write(dir.path().join("README.md"), "junk").unwrap();
        std::fs::create_dir(dir.path().join("old.css")).unwrap();

        let (themes, errors) = load_dir(dir.path());

        assert_eq!(themes.keys().collect::<Vec<_>>(), ["night"]);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn load_dir_keeps_the_good_files_beside_a_broken_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("night.css"),
            ":root { color-scheme: dark; }",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("broken.css"),
            ":root {\n  color-scheme: dark;\n  --page: url(x);\n}",
        )
        .unwrap();

        let (themes, errors) = load_dir(dir.path());

        assert_eq!(themes.keys().collect::<Vec<_>>(), ["night"]);
        assert_eq!(errors.len(), 1);
        let said = errors[0].to_string();
        assert!(said.starts_with("theme "), "{said}");
        assert!(
            said.contains("broken.css:3: `--page` takes a hex color"),
            "{said}"
        );
    }

    #[test]
    fn a_file_too_large_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let big = format!(":root {{ color-scheme: dark; }}/*{}*/", "x".repeat(70_000));
        std::fs::write(dir.path().join("big.css"), big).unwrap();

        let (themes, errors) = load_dir(dir.path());

        assert!(themes.is_empty());
        assert!(errors[0].problem.what.contains("64 KiB"));
    }

    #[test]
    fn a_file_too_large_is_said_so_even_when_cut_inside_a_character() {
        let dir = tempfile::tempdir().unwrap();
        // "é" is two bytes, so the bound of 64 KiB falls inside one.
        let big = format!(":root {{ color-scheme: dark; }}/*{}*/", "é".repeat(40_000));
        std::fs::write(dir.path().join("big.css"), big).unwrap();

        let (_, errors) = load_dir(dir.path());

        assert!(errors[0].problem.what.contains("64 KiB"), "{errors:?}");
    }

    #[test]
    fn a_missing_directory_is_no_error() {
        let dir = tempfile::tempdir().unwrap();

        let (themes, errors) = load_dir(&dir.path().join("themes"));

        assert!(themes.is_empty());
        assert!(errors.is_empty());
    }

    /// The `--name:` declarations of the block that opens with `opening`.
    fn declared(css: &str, opening: &str) -> Vec<String> {
        let start = css.find(opening).expect(opening) + opening.len();
        let end = start + css[start..].find('}').unwrap();
        css[start..end]
            .lines()
            .filter_map(|l| l.trim().strip_prefix("--"))
            .filter_map(|l| l.split(':').next())
            .filter(|n| !["code-font", "code-size", "code-line", "tab-width"].contains(n))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_token_list_matches_the_stylesheet() {
        // Read at compile time: the binary still reads no file of its own.
        let css = include_str!("../../../web/src/styles/main.css");

        let light = declared(css, ":root {");
        assert_eq!(
            light, TOKENS,
            "TOKENS and the :root block of main.css differ"
        );

        // Same in both modes: the dark blocks inherit them from :root.
        const SAME: &[&str] = &[
            "add-ink",
            "remove-ink",
            "ok-ink",
            "error-ink",
            "tok-invalid",
            "tok-invalid-ink",
            "shadow",
            "shadow-strong",
            "veil",
        ];
        let mut light: Vec<String> = light
            .into_iter()
            .filter(|t| !SAME.contains(&t.as_str()))
            .collect();
        light.sort();
        for opening in [
            ":root:not([data-theme='light']) {",
            ":root[data-theme='dark'] {",
        ] {
            let mut dark = declared(css, opening);
            dark.sort();
            assert_eq!(dark, light, "{opening} does not set every color token");
        }
    }
}

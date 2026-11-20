//! What the user and the repository ask for.
//!
//! Three layers, the later one wins: the defaults built into the tool, the
//! configuration of the user, then the one of the repository. A repository
//! that declares its own file types therefore hands the map to every reader
//! with no setup.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::theme::{self, Theme};

/// The configuration, after the three layers are folded together.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub languages: HashMap<String, String>,
    pub gerrit: Gerrit,
    pub series: Series,
    pub ui: Ui,
    pub diff: Diff,
    pub update: Update,
    /// What the terminal must say about the themes: a file left out, a
    /// `ui.theme` that names no theme. A theme never stops the tool.
    #[serde(skip)]
    pub notes: Vec<String>,
}

/// Where to ask whether a newer qreview is out.
///
/// The default is the home of the project. An empty address asks nowhere,
/// which is the way to turn the check off, and a fork writes its own.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// A URL that answers with JSON holding `tag_name`, the way the
    /// releases API of GitHub does.
    pub url: Option<String>,
    /// Sent as `Authorization: Bearer`, for a fork that is not public.
    pub token: Option<String>,
}

/// The releases of qreview, where the tool is published.
const HOME: &str = "https://api.github.com/repos/Intersec/qreview/releases/latest";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Gerrit {
    pub enabled: bool,
    /// The branch a change targets, when `.gerrit-branch` does not say.
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub max_commits: usize,
    pub guess_max: usize,
    pub batch_size: usize,
    pub integration_branch: Option<String>,
    /// Show the tracked changes that are not committed as a change.
    pub worktree: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Ui {
    /// `unified` or `side-by-side`.
    pub diff: String,
    /// `system`, `light`, `dark`, or the name of a loaded theme: the one
    /// the tool really uses, after the fallback.
    pub theme: String,
    /// The themes of the user that loaded, by name. An output only:
    /// `config.json` cannot hold it.
    pub themes: BTreeMap<String, Theme>,
    /// The `ui.theme` of each file that sets one, the first file first.
    #[serde(skip)]
    pub choices: Vec<Choice>,
}

/// The `ui.theme` that one file asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub file: PathBuf,
    pub value: String,
}

/// What the reader asked of a diff. The panel writes these.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// Lines of unchanged code kept around a change.
    pub context: usize,
    /// Fold a long line instead of scrolling to its end.
    pub wrap: bool,
    /// Leave out what differs only by spacing.
    pub ignore_whitespace: bool,
    pub tab_width: usize,
    pub font_size: usize,
    /// Colour the code. Off is faster on a very large file.
    pub syntax: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            languages: HashMap::new(),
            gerrit: Gerrit {
                enabled: true,
                branch: None,
            },
            series: Series {
                max_commits: 50,
                guess_max: 10,
                batch_size: 5,
                integration_branch: None,
                worktree: true,
            },
            ui: Ui {
                diff: "unified".to_owned(),
                theme: "system".to_owned(),
                themes: BTreeMap::new(),
                choices: Vec::new(),
            },
            diff: Diff {
                // Three lines is what git gives, and it is too few to judge
                // a change. Gerrit offers ten, and that is the useful one.
                context: 10,
                wrap: false,
                ignore_whitespace: false,
                tab_width: 4,
                font_size: 12,
                syntax: true,
            },
            update: Update {
                url: Some(HOME.to_owned()),
                token: None,
            },
            notes: Vec::new(),
        }
    }
}

/// One layer, as it is written on disk. Every field is optional, because a
/// layer says only what it changes.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layer {
    #[serde(default)]
    pub languages: Option<HashMap<String, String>>,
    #[serde(default)]
    pub gerrit: Option<GerritLayer>,
    #[serde(default)]
    pub series: Option<SeriesLayer>,
    #[serde(default)]
    pub ui: Option<UiLayer>,
    #[serde(default)]
    pub diff: Option<DiffLayer>,
    #[serde(default)]
    pub update: Option<UpdateLayer>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateLayer {
    pub url: Option<String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GerritLayer {
    pub enabled: Option<bool>,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeriesLayer {
    pub max_commits: Option<usize>,
    pub guess_max: Option<usize>,
    pub batch_size: Option<usize>,
    pub integration_branch: Option<String>,
    pub worktree: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiLayer {
    pub diff: Option<String>,
    pub theme: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffLayer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_whitespace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<bool>,
}

impl Config {
    /// Fold one layer on top of this one.
    pub fn apply(&mut self, layer: Layer) {
        if let Some(languages) = layer.languages {
            for (ext, lang) in languages {
                self.languages
                    .insert(ext.trim_start_matches('.').to_lowercase(), lang);
            }
        }
        if let Some(gerrit) = layer.gerrit {
            self.gerrit.enabled = gerrit.enabled.unwrap_or(self.gerrit.enabled);
            self.gerrit.branch = gerrit.branch.or(self.gerrit.branch.take());
        }
        if let Some(series) = layer.series {
            self.series.max_commits = series.max_commits.unwrap_or(self.series.max_commits);
            self.series.guess_max = series.guess_max.unwrap_or(self.series.guess_max);
            self.series.batch_size = series.batch_size.unwrap_or(self.series.batch_size);
            self.series.integration_branch = series
                .integration_branch
                .or(self.series.integration_branch.take());
            self.series.worktree = series.worktree.unwrap_or(self.series.worktree);
        }
        if let Some(ui) = layer.ui {
            // ui.theme is picked by resolve_theme, from the choices.
            self.ui.diff = ui.diff.unwrap_or_else(|| self.ui.diff.clone());
        }
        if let Some(diff) = layer.diff {
            self.diff.context = diff.context.unwrap_or(self.diff.context).clamp(0, 2000);
            self.diff.wrap = diff.wrap.unwrap_or(self.diff.wrap);
            self.diff.ignore_whitespace = diff
                .ignore_whitespace
                .unwrap_or(self.diff.ignore_whitespace);
            self.diff.tab_width = diff.tab_width.unwrap_or(self.diff.tab_width).clamp(1, 16);
            self.diff.font_size = diff.font_size.unwrap_or(self.diff.font_size).clamp(8, 24);
            self.diff.syntax = diff.syntax.unwrap_or(self.diff.syntax);
        }
        if let Some(update) = layer.update {
            // An empty address is a choice, not an absence: it is how a
            // reader says to ask nobody.
            self.update.url = update.url.or(self.update.url.take());
            self.update.token = update.token.or(self.update.token.take());
        }
    }
}

/// Read the two files, in order, on top of the defaults.
///
/// A file that is not there is not an error. A file that is there and is
/// wrong is: a configuration that is silently ignored is worse than one that
/// refuses to start, because nobody notices.
pub fn load(repo_root: &Path) -> Result<Config> {
    load_from(user_path(), repo_root)
}

fn load_from(user: Option<PathBuf>, repo_root: &Path) -> Result<Config> {
    load_with(user, repo_root, None)
}

/// `loaded` is the themes directory when the caller already read it.
fn load_with(user: Option<PathBuf>, repo_root: &Path, loaded: Option<Loaded>) -> Result<Config> {
    let mut config = Config::default();
    let themes = user.as_deref().and_then(theme_dir_of);

    for path in [user, Some(repo_root.join(".qreview.json"))]
        .into_iter()
        .flatten()
    {
        if let Some(layer) = read(&path)? {
            if let Some(value) = layer.ui.as_ref().and_then(|ui| ui.theme.clone()) {
                config.ui.choices.push(Choice {
                    file: path.clone(),
                    value,
                });
            }
            config.apply(layer);
        }
    }

    if let Some(dir) = themes {
        let loaded = loaded.unwrap_or_else(|| theme::load_dir(&dir));
        config.use_themes(&dir, loaded);
    } else {
        config.ui.themes = theme::with_bundled(BTreeMap::new());
        config.resolve_theme(None);
    }
    Ok(config)
}

impl Config {
    /// Take the themes read from `dir`, and pick the theme to use.
    ///
    /// Only the user directory holds themes: a repository may name one, but
    /// it cannot bring colors of its own.
    pub fn use_themes(&mut self, dir: &Path, (themes, errors): Loaded) {
        self.ui.themes = theme::with_bundled(themes);
        self.notes = errors.iter().map(ToString::to_string).collect();
        if !errors.is_empty() {
            self.notes.push(theme::HINT.to_owned());
        }
        self.resolve_theme(Some(dir));
    }

    /// The last file whose `ui.theme` is a built-in or a loaded theme wins.
    /// Each file skipped on the way is said, with what the tool does instead.
    fn resolve_theme(&mut self, dir: Option<&Path>) {
        self.ui.theme = "system".to_owned();
        for (i, choice) in self.ui.choices.iter().enumerate().rev() {
            if known(&choice.value, &self.ui.themes) {
                self.ui.theme = choice.value.clone();
                return;
            }
            let instead = match i.checked_sub(1).map(|j| &self.ui.choices[j]) {
                Some(next) => format!("the ui.theme of {}", next.file.display()),
                None => "`system`".to_owned(),
            };
            self.notes.push(format!(
                "ui.theme in {} {} The tool falls back to {instead}.",
                choice.file.display(),
                unknown(&choice.value, &self.ui.themes, dir),
            ));
        }
    }
}

/// What `theme::load_dir` read: the themes, and the files left out.
pub type Loaded = (BTreeMap<String, Theme>, Vec<theme::FileError>);

fn known(value: &str, themes: &BTreeMap<String, Theme>) -> bool {
    theme::BUILT_IN.contains(&value) || themes.contains_key(value)
}

/// Why `value` is not a theme, with the themes that loaded.
fn unknown(value: &str, themes: &BTreeMap<String, Theme>, dir: Option<&Path>) -> String {
    format!(
        "names `{value}`, and no theme of that name loaded. {}",
        loaded(themes, dir)
    )
}

/// The themes that loaded, as a sentence: the files of the user, then the
/// themes that qreview carries.
fn loaded(themes: &BTreeMap<String, Theme>, dir: Option<&Path>) -> String {
    let names = |bundled: bool| {
        let names: Vec<&str> = themes
            .iter()
            .filter(|(_, theme)| theme.bundled == bundled)
            .map(|(name, _)| name.as_str())
            .collect();
        names.join(", ")
    };
    let user = match (names(false), dir) {
        (list, _) if !list.is_empty() => format!("Loaded themes: {list}."),
        (_, Some(dir)) => format!("No theme loaded from {}.", dir.display()),
        (_, None) => "No theme loaded.".to_owned(),
    };
    match names(true) {
        list if list.is_empty() => user,
        list => format!("{user} Bundled themes: {list}."),
    }
}

fn read(path: &Path) -> Result<Option<Layer>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(None);
    };

    let layer = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a readable configuration", path.display()))?;

    Ok(Some(layer))
}

/// Write what the panel changed into the file of the user, and read the
/// three layers back.
///
/// Only the fields the panel owns are written. A language map or a Gerrit
/// setting the user put there by hand stays where it is.
pub fn update(repo_root: &Path, patch: &serde_json::Value) -> Result<Config> {
    let path = user_path().context("no configuration directory for this user")?;
    update_at(&path, repo_root, patch)
}

/// A change from the panel that would leave the file unreadable, and why.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the preferences were not saved: {}", self.0)
    }
}

impl std::error::Error for Refused {}

fn update_at(path: &Path, repo_root: &Path, patch: &serde_json::Value) -> Result<Config> {
    let mut stored: serde_json::Value = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("{} is not a readable configuration", path.display()))?,
        Err(_) => serde_json::json!({}),
    };

    for section in ["diff", "ui"] {
        let Some(fields) = patch.get(section).and_then(|s| s.as_object()) else {
            continue;
        };
        let target = stored
            .as_object_mut()
            .context("the configuration is not an object")?
            .entry(section)
            .or_insert_with(|| serde_json::json!({}));

        if let Some(target) = target.as_object_mut() {
            for (name, value) in fields {
                // Each field alone first, so the error names it: a whole
                // file read from a value says what is wrong, not where.
                let one = serde_json::json!({ section: { name: value } });
                if let Err(error) = serde_json::from_value::<Layer>(one) {
                    return Err(Refused(format!("{section}.{name}: {error}")).into());
                }
                target.insert(name.clone(), value.clone());
            }
        }
    }

    // Read the result before writing it: a file that the next start
    // refuses must never reach the disk.
    if let Err(error) = Layer::deserialize(&stored) {
        return Err(Refused(error.to_string()).into());
    }

    // Read once, for the check and for the configuration sent back.
    let dir = theme_dir_of(path);
    let found = dir.as_deref().map(theme::load_dir);

    // Only a theme the panel asks for is checked. A stored one that failed
    // to load must not block a save of the font size.
    if let Some(wanted) = patch.pointer("/ui/theme").and_then(|t| t.as_str()) {
        let user = found.as_ref().map(|(themes, _)| themes.clone());
        let themes = theme::with_bundled(user.unwrap_or_default());
        if !known(wanted, &themes) {
            let why = unknown(wanted, &themes, dir.as_deref());
            return Err(Refused(format!("ui.theme {why}")).into());
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot make {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&stored)?;
    std::fs::write(path, format!("{text}\n"))
        .with_context(|| format!("cannot write {}", path.display()))?;

    load_with(Some(path.to_owned()), repo_root, found)
}

fn user_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };

    Some(base.join("qreview").join("config.json"))
}

/// Where a user grammar file may be dropped.
pub fn grammar_dir() -> Option<PathBuf> {
    Some(user_path()?.parent()?.join("grammars"))
}

/// Where the themes of the user are, beside the file of the user.
pub fn theme_dir() -> Option<PathBuf> {
    theme_dir_of(&user_path()?)
}

fn theme_dir_of(user: &Path) -> Option<PathBuf> {
    Some(user.parent()?.join("themes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(text: &str) -> Layer {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn the_defaults_stand_on_their_own() {
        let config = Config::default();

        assert!(config.gerrit.enabled);
        assert_eq!(config.series.guess_max, 10);
        assert_eq!(config.series.batch_size, 5);
        assert_eq!(config.ui.diff, "unified");
    }

    #[test]
    fn a_layer_changes_only_what_it_names() {
        let mut config = Config::default();
        config.apply(layer(r#"{"series":{"batchSize":10}}"#));

        assert_eq!(config.series.batch_size, 10);
        assert_eq!(config.series.guess_max, 10, "untouched");
        assert!(config.gerrit.enabled, "untouched");
    }

    #[test]
    fn the_later_layer_wins() {
        let mut config = Config::default();
        config.apply(layer(r#"{"gerrit":{"enabled":false}}"#));
        config.apply(layer(r#"{"gerrit":{"enabled":true}}"#));

        assert!(config.gerrit.enabled);
    }

    #[test]
    fn language_maps_are_merged_and_not_replaced() {
        let mut config = Config::default();
        config.apply(layer(r#"{"languages":{"aa":"c"}}"#));
        config.apply(layer(r#"{"languages":{".BB":"python"}}"#));

        assert_eq!(config.languages.get("aa").map(String::as_str), Some("c"));
        assert_eq!(
            config.languages.get("bb").map(String::as_str),
            Some("python"),
            "a leading dot and the case are both allowed"
        );
    }

    #[test]
    fn a_key_nobody_knows_is_refused_rather_than_ignored() {
        let error = serde_json::from_str::<Layer>(r#"{"seriez":{"batchSize":3}}"#).unwrap_err();

        assert!(error.to_string().contains("seriez"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_there_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();

        assert!(read(&dir.path().join("nothing.json")).unwrap().is_none());
    }

    #[test]
    fn a_file_that_is_wrong_names_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".qreview.json");
        std::fs::write(&path, "{ not json").unwrap();

        let error = read(&path).unwrap_err().to_string();
        assert!(error.contains(".qreview.json"), "{error}");
    }

    #[test]
    fn the_repository_layer_is_read_after_the_user_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".qreview.json"),
            r#"{"languages":{"zz":"yaml"},"ui":{"diff":"side-by-side"}}"#,
        )
        .unwrap();

        // No file of the user: the real one of the developer may hold anything.
        let config = load_from(None, dir.path()).unwrap();
        assert_eq!(config.languages.get("zz").map(String::as_str), Some("yaml"));
        assert_eq!(config.ui.diff, "side-by-side");
    }

    #[test]
    fn an_invalid_patch_leaves_the_file_as_it_was() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let path = home.path().join("config.json");
        let before = "{\n  \"ui\": {\n    \"theme\": \"dark\"\n  }\n}\n";
        std::fs::write(&path, before).unwrap();

        let error =
            update_at(&path, repo.path(), &serde_json::json!({"ui": {"theme": 5}})).unwrap_err();

        assert!(error.downcast_ref::<Refused>().is_some(), "{error:#}");
        assert!(error.to_string().contains("ui.theme"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn a_patch_with_an_unknown_key_is_refused_and_named() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let path = home.path().join("qreview").join("config.json");

        let error = update_at(
            &path,
            repo.path(),
            &serde_json::json!({"ui": {"thme": "dark"}}),
        )
        .unwrap_err();

        // A Refused is a 400. Any other error would be a 500.
        assert!(error.downcast_ref::<Refused>().is_some(), "{error:#}");
        assert!(error.to_string().contains("ui.thme: "), "{error:#}");
        assert!(!path.exists());
    }

    #[test]
    fn a_valid_patch_is_written_and_read_back() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let path = home.path().join("qreview").join("config.json");

        let config = update_at(
            &path,
            repo.path(),
            &serde_json::json!({"diff": {"context": 5}}),
        )
        .unwrap();

        assert_eq!(config.diff.context, 5);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\"context\": 5")
        );
    }

    /// A user directory with its `config.json` and its themes, and a
    /// repository with its `.qreview.json`.
    struct Setup {
        home: tempfile::TempDir,
        repo: tempfile::TempDir,
    }

    impl Setup {
        fn new(user: &str, repo: Option<&str>, themes: &[(&str, &str)]) -> Self {
            let home = tempfile::tempdir().unwrap();
            let repo_dir = tempfile::tempdir().unwrap();
            std::fs::write(home.path().join("config.json"), user).unwrap();
            if let Some(text) = repo {
                std::fs::write(repo_dir.path().join(".qreview.json"), text).unwrap();
            }
            std::fs::create_dir(home.path().join("themes")).unwrap();
            for (name, text) in themes {
                std::fs::write(home.path().join("themes").join(name), text).unwrap();
            }
            Self {
                home,
                repo: repo_dir,
            }
        }

        fn user(&self) -> PathBuf {
            self.home.path().join("config.json")
        }

        fn load(&self) -> Config {
            load_from(Some(self.user()), self.repo.path()).unwrap()
        }
    }

    const MIDNIGHT: (&str, &str) = (
        "midnight.css",
        ":root { color-scheme: dark; --page: #000010; }",
    );
    const PARCHMENT: (&str, &str) = (
        "parchment.css",
        ":root { color-scheme: light; --page: #fafaf0; }",
    );

    #[test]
    fn ui_theme_may_name_a_loaded_theme() {
        let setup = Setup::new(r#"{"ui":{"theme":"midnight"}}"#, None, &[MIDNIGHT]);

        let config = setup.load();

        assert_eq!(config.ui.theme, "midnight");
        assert_eq!(config.ui.themes["midnight"].colors["page"], "#000010");
        assert!(config.notes.is_empty(), "{:?}", config.notes);
    }

    #[test]
    fn a_repository_may_pick_a_theme_of_the_user() {
        let setup = Setup::new(
            r#"{"ui":{"theme":"parchment"}}"#,
            Some(r#"{"ui":{"theme":"midnight"}}"#),
            &[MIDNIGHT, PARCHMENT],
        );

        let config = setup.load();

        assert_eq!(config.ui.theme, "midnight");
        assert!(config.notes.is_empty(), "{:?}", config.notes);
    }

    #[test]
    fn an_unknown_name_in_the_repository_falls_back_to_the_user() {
        let setup = Setup::new(
            r#"{"ui":{"theme":"parchment"}}"#,
            Some(r#"{"ui":{"theme":"gone"}}"#),
            &[MIDNIGHT, PARCHMENT],
        );

        let config = setup.load();

        assert_eq!(config.ui.theme, "parchment");
        assert_eq!(config.notes.len(), 1);
        let note = &config.notes[0];
        assert!(note.contains(".qreview.json names `gone`"), "{note}");
        assert!(
            note.contains("Loaded themes: midnight, parchment."),
            "{note}"
        );
        assert!(note.contains("falls back to the ui.theme of"), "{note}");
        assert!(note.contains("config.json"), "{note}");
    }

    #[test]
    fn when_both_files_fail_the_tool_uses_system() {
        let setup = Setup::new(
            r#"{"ui":{"theme":"lost"}}"#,
            Some(r#"{"ui":{"theme":"gone"}}"#),
            &[],
        );

        let config = setup.load();

        assert_eq!(config.ui.theme, "system");
        assert_eq!(config.notes.len(), 2);
        assert!(
            config.notes[1].contains("names `lost`"),
            "{:?}",
            config.notes
        );
        assert!(config.notes[1].ends_with("falls back to `system`."));
        assert!(config.notes[1].contains("No theme loaded from"));
    }

    #[test]
    fn a_broken_file_counts_as_not_loaded() {
        let broken = (
            "midnight.css",
            ":root {\n  color-scheme: dark;\n  --page: red;\n}",
        );
        let setup = Setup::new(r#"{"ui":{"theme":"midnight"}}"#, None, &[broken]);

        let config = setup.load();

        assert_eq!(config.ui.theme, "system");
        assert!(
            config.notes[0].contains("midnight.css:3:"),
            "{:?}",
            config.notes
        );
        assert_eq!(config.notes[1], theme::HINT);
        assert!(
            config.notes[2].contains("names `midnight`"),
            "{:?}",
            config.notes
        );
    }

    #[test]
    fn a_repository_cannot_bring_colors() {
        let setup = Setup::new("{}", Some(r#"{"ui":{"theme":"x"}}"#), &[]);
        let css = ":root { color-scheme: dark; }";
        std::fs::write(setup.repo.path().join("x.css"), css).unwrap();
        std::fs::create_dir(setup.repo.path().join("themes")).unwrap();
        std::fs::write(setup.repo.path().join("themes").join("x.css"), css).unwrap();

        let config = setup.load();

        assert_eq!(config.ui.theme, "system");
        assert!(config.ui.themes.values().all(|t| t.bundled));
    }

    #[test]
    fn a_file_of_the_user_replaces_a_bundled_theme() {
        let mine = (
            "nord.css",
            ":root { color-scheme: light; --page: #fafaf0; }",
        );
        let setup = Setup::new(r#"{"ui":{"theme":"nord"}}"#, None, &[mine]);

        let config = setup.load();

        assert_eq!(config.ui.theme, "nord");
        let nord = &config.ui.themes["nord"];
        assert!(!nord.bundled);
        assert_eq!(nord.colors.keys().collect::<Vec<_>>(), ["page"]);
    }

    #[test]
    fn a_broken_file_of_the_user_leaves_the_bundled_theme() {
        let broken = (
            "nord.css",
            ":root {\n  color-scheme: dark;\n  --page: red;\n}",
        );
        let setup = Setup::new(r#"{"ui":{"theme":"nord"}}"#, None, &[broken]);

        let config = setup.load();

        assert_eq!(config.ui.theme, "nord");
        assert!(config.ui.themes["nord"].bundled);
        assert!(
            config.notes[0].contains("nord.css:3:"),
            "{:?}",
            config.notes
        );
    }

    #[test]
    fn a_save_may_name_a_bundled_theme() {
        let setup = Setup::new("{}", None, &[]);

        let config = update_at(
            &setup.user(),
            setup.repo.path(),
            &serde_json::json!({"ui": {"theme": "dracula"}}),
        )
        .unwrap();

        assert_eq!(config.ui.theme, "dracula");
    }

    #[test]
    fn a_note_lists_the_themes_of_the_user_then_the_bundled_ones() {
        let setup = Setup::new(r#"{"ui":{"theme":"gone"}}"#, None, &[MIDNIGHT]);

        let note = &setup.load().notes[0];

        assert!(
            note.contains("Loaded themes: midnight. Bundled themes: "),
            "{note}"
        );
        assert!(note.contains("dracula"), "{note}");
    }

    #[test]
    fn a_built_in_in_the_repository_wins_over_a_theme_of_the_user() {
        let setup = Setup::new(
            r#"{"ui":{"theme":"midnight"}}"#,
            Some(r#"{"ui":{"theme":"dark"}}"#),
            &[MIDNIGHT],
        );

        assert_eq!(setup.load().ui.theme, "dark");
    }

    #[test]
    fn a_save_that_names_no_loaded_theme_is_refused_before_writing() {
        let before = r#"{"ui":{"theme":"midnight"}}"#;
        let setup = Setup::new(before, None, &[MIDNIGHT]);

        let error = update_at(
            &setup.user(),
            setup.repo.path(),
            &serde_json::json!({"ui": {"theme": "gone"}}),
        )
        .unwrap_err();

        assert!(error.downcast_ref::<Refused>().is_some(), "{error:#}");
        assert!(
            error.to_string().contains("Loaded themes: midnight."),
            "{error}"
        );
        assert_eq!(std::fs::read_to_string(setup.user()).unwrap(), before);
    }

    #[test]
    fn a_save_without_a_theme_keeps_the_one_in_the_file() {
        // The theme did not load, so the page shows system. A save of the
        // font size must not write that fallback over the choice of the user.
        let setup = Setup::new(r#"{"ui":{"theme":"midnight"}}"#, None, &[]);

        let config = update_at(
            &setup.user(),
            setup.repo.path(),
            &serde_json::json!({"diff": {"fontSize": 14}}),
        )
        .unwrap();

        assert_eq!(config.ui.theme, "system");
        let stored = std::fs::read_to_string(setup.user()).unwrap();
        assert!(stored.contains(r#""theme": "midnight""#), "{stored}");
    }

    #[test]
    fn the_folded_ui_is_pinned() {
        let setup = Setup::new(
            r#"{"ui":{"theme":"midnight"}}"#,
            None,
            &[MIDNIGHT, PARCHMENT],
        );

        // The bundled themes are pinned by their own files, not here.
        let mut ui = setup.load().ui;
        ui.themes.retain(|_, theme| !theme.bundled);
        let ui = serde_json::to_string_pretty(&ui).unwrap();

        insta::assert_snapshot!(ui);
    }
}

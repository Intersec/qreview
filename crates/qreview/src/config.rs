//! What the user and the repository ask for.
//!
//! Three layers, the later one wins: the defaults built into the tool, the
//! configuration of the user, then the one of the repository. A repository
//! that declares its own file types therefore hands the map to every reader
//! with no setup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Ui {
    /// `unified` or `side-by-side`.
    pub diff: String,
    /// `system`, `light` or `dark`.
    pub theme: String,
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
            },
            ui: Ui {
                diff: "unified".to_owned(),
                theme: "system".to_owned(),
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
        }
        if let Some(ui) = layer.ui {
            self.ui.diff = ui.diff.unwrap_or_else(|| self.ui.diff.clone());
            self.ui.theme = ui.theme.unwrap_or_else(|| self.ui.theme.clone());
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
    let mut config = Config::default();

    for path in [user_path(), Some(repo_root.join(".qreview.json"))]
        .into_iter()
        .flatten()
    {
        if let Some(layer) = read(&path)? {
            config.apply(layer);
        }
    }
    Ok(config)
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
    let mut stored: serde_json::Value = match std::fs::read_to_string(&path) {
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
                target.insert(name.clone(), value.clone());
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot make {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&stored)?;
    std::fs::write(&path, format!("{text}\n"))
        .with_context(|| format!("cannot write {}", path.display()))?;

    load(repo_root)
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

        let config = load(dir.path()).unwrap();
        assert_eq!(config.languages.get("zz").map(String::as_str), Some("yaml"));
        assert_eq!(config.ui.diff, "side-by-side");
    }
}

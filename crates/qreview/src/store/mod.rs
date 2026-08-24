//! Where the comments live.
//!
//! Under the state directory of the user, keyed by repository and then by
//! change. The data survives a `git clean`, a worktree removal and a
//! reclone, and it never enters the working tree, so it can never be
//! committed by accident.

pub mod model;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub use model::{Anchor, ChangeFile, Comment, Scope, Side};

pub struct Store {
    changes: PathBuf,
}

impl Store {
    /// The store of a repository, under the state directory of the user.
    pub fn open(repo_id: &str) -> Result<Self> {
        let base = state_home().join("qreview").join("repos").join(repo_id);

        Ok(Self::at(&base))
    }

    /// The same, rooted anywhere. The tests own their directory.
    pub fn at(base: &Path) -> Self {
        Self {
            changes: base.join("changes"),
        }
    }

    fn path_of(&self, key: &str) -> PathBuf {
        self.changes.join(format!("{}.json", safe(key)))
    }

    /// Read the review of a change. An empty one when nothing is stored.
    pub fn load(&self, key: &str, subject: &str) -> Result<ChangeFile> {
        let path = self.path_of(key);
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(ChangeFile::new(key, subject));
        };

        let mut file: ChangeFile = serde_json::from_str(&text).with_context(|| {
            format!(
                "{} is not readable. It is left as it is: repair it by hand \
                 rather than lose the review",
                path.display()
            )
        })?;

        if file.version > model::VERSION {
            bail!(
                "{} was written by a newer qreview (format {}, this one reads {})",
                path.display(),
                file.version,
                model::VERSION
            );
        }

        // The migration of an older file. Every format so far reads as the
        // current one, so the stamp is the whole of it: the next write then
        // says what the file holds, and an older qreview refuses it rather
        // than dropping what it cannot read.
        file.version = model::VERSION;

        Ok(file)
    }

    /// Write the review of a change.
    ///
    /// The write is atomic: a full file appears, or the old one stays. A
    /// half-written review is worse than none, because nobody notices.
    pub fn save(&self, file: &ChangeFile) -> Result<()> {
        fs::create_dir_all(&self.changes)
            .with_context(|| format!("cannot make {}", self.changes.display()))?;

        let path = self.path_of(&file.key);
        let temp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(file)?;

        let mut handle =
            fs::File::create(&temp).with_context(|| format!("cannot write {}", temp.display()))?;
        handle.write_all(text.as_bytes())?;
        handle.write_all(b"\n")?;
        handle.sync_all()?;
        drop(handle);

        fs::rename(&temp, &path)
            .with_context(|| format!("cannot put {} in place", path.display()))?;

        Ok(())
    }

    /// The keys of every change this repository has a review for.
    pub fn keys(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.changes) else {
            return Vec::new();
        };

        let mut keys: Vec<_> = entries
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".json").map(str::to_owned)
            })
            .collect();

        keys.sort();
        keys
    }
}

/// A key is a Change-Id or `sha-<hash>`, but a corrupt one must never write
/// outside the store.
fn safe(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn state_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local").join("state"),
        None => PathBuf::from(".qreview-state"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::{Scope, Side};

    fn comment(id: &str, body: &str) -> Comment {
        Comment {
            id: id.to_owned(),
            patch_set: 1,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
            scope: Scope::Line,
            body: body.to_owned(),
            anchor: Some(Anchor {
                file: "src/a.blk".to_owned(),
                side: Side::New,
                start_line: Some(42),
                end_line: Some(42),
                start_char: None,
                end_char: None,
                blob: Some("b7a1".to_owned()),
                line_hash: Some("sha256:9c1f".to_owned()),
                context: vec!["one".to_owned(), "two".to_owned()],
            }),
        }
    }

    #[test]
    fn a_change_with_no_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());

        let file = store.load("I8f3a", "a subject").unwrap();
        assert!(file.comments.is_empty());
        assert_eq!(file.subject, "a subject");
    }

    #[test]
    fn what_is_written_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());

        let mut file = ChangeFile::new("I8f3a", "a subject");
        file.comments.push(comment("c-1", "this loop never ends"));
        store.save(&file).unwrap();

        let back = store.load("I8f3a", "another subject").unwrap();
        assert_eq!(back, file, "the stored subject wins over the one passed in");
        assert_eq!(back.comments[0].body, "this loop never ends");
    }

    #[test]
    fn a_write_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        store.save(&ChangeFile::new("I8f3a", "s")).unwrap();

        let names: Vec<_> = fs::read_dir(dir.path().join("changes"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(names, ["I8f3a.json"]);
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());

        let mut file = ChangeFile::new("I8f3a", "s");
        file.comments.push(comment("c-1", "one"));
        store.save(&file).unwrap();

        file.comments.push(comment("c-2", "two"));
        store.save(&file).unwrap();

        assert_eq!(store.load("I8f3a", "s").unwrap().comments.len(), 2);
    }

    #[test]
    fn a_corrupt_file_is_an_error_and_stays_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        store.save(&ChangeFile::new("I8f3a", "s")).unwrap();

        let path = dir.path().join("changes").join("I8f3a.json");
        fs::write(&path, "{ this is not json").unwrap();

        let error = store.load("I8f3a", "s").unwrap_err().to_string();
        assert!(error.contains("repair it by hand"), "{error}");
        assert!(path.exists(), "the file must never be deleted");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ this is not json");
    }

    #[test]
    fn a_newer_format_is_refused_rather_than_guessed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let path = dir.path().join("changes");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("I8f3a.json"),
            r#"{"version":99,"key":"I8f3a","subject":"s","comments":[]}"#,
        )
        .unwrap();

        let error = store.load("I8f3a", "s").unwrap_err().to_string();
        assert!(error.contains("newer qreview"), "{error}");
    }

    #[test]
    fn a_file_of_an_older_format_is_read_and_stamped() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let path = dir.path().join("changes");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("I8f3a.json"),
            r#"{"version":1,"key":"I8f3a","subject":"s","comments":[{"id":"c1",
               "patchSet":1,"createdAt":"","updatedAt":"","scope":"line",
               "body":"a remark","anchor":{"file":"a.c","side":"new",
               "startLine":3,"endLine":3}}]}"#,
        )
        .unwrap();

        let file = store.load("I8f3a", "s").unwrap();

        assert_eq!(file.comments.len(), 1, "the comment of the older file");
        assert_eq!(file.comments[0].anchor.as_ref().unwrap().start_char, None);
        assert_eq!(
            file.version,
            model::VERSION,
            "a write must say what the file holds"
        );
    }

    #[test]
    fn the_keys_of_the_store_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        store.save(&ChangeFile::new("Ibbb", "s")).unwrap();
        store.save(&ChangeFile::new("Iaaa", "s")).unwrap();

        assert_eq!(store.keys(), ["Iaaa", "Ibbb"]);
    }

    #[test]
    fn a_key_can_never_write_outside_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        store.save(&ChangeFile::new("../../escape", "s")).unwrap();

        let names: Vec<_> = fs::read_dir(dir.path().join("changes"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["______escape.json"]);
    }
}

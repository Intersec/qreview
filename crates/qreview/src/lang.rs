//! Which language a file is written in.
//!
//! The map is data, not code. Bundled grammars claim the common extensions on
//! their own, so this only carries the ones they do not.

use std::collections::HashMap;

/// The extensions no bundled grammar claims.
///
/// A code base with its own file types adds them in `.qreview.json` at its
/// top level, so every reader of that repository gets the map with no setup.
const DEFAULTS: &[(&str, &str)] = &[
    ("blk", "c"),
    ("blkk", "c"),
    ("pxc", "c"),
    // No Cython grammar is bundled, measured on the set we ship. Python is
    // the closest thing that exists, and it reads well enough. A user who
    // wants better drops a grammar file in the configuration directory.
    ("pyx", "python"),
    ("pxd", "python"),
    ("pxi", "python"),
    ("iop", "d"),
    ("tpl", "jinja2"),
];

/// The map from extension to language name.
#[derive(Clone, Debug)]
pub struct Languages {
    map: HashMap<String, String>,
}

impl Default for Languages {
    fn default() -> Self {
        Self::new()
    }
}

impl Languages {
    pub fn new() -> Self {
        let map = DEFAULTS
            .iter()
            .map(|(ext, lang)| ((*ext).to_owned(), (*lang).to_owned()))
            .collect();

        Self { map }
    }

    /// Add the map of the user or of the repository. A user entry wins.
    pub fn extend(&mut self, user: &HashMap<String, String>) {
        for (ext, lang) in user {
            self.map
                .insert(ext.trim_start_matches('.').to_lowercase(), lang.clone());
        }
    }

    /// The language of a path, or `None` when nothing claims it.
    ///
    /// `None` is not a failure. It asks the highlighter to decide, from the
    /// extension it knows or from the first line of the file.
    pub fn of(&self, path: &str) -> Option<&str> {
        let name = path.rsplit('/').next().unwrap_or(path);
        let ext = name.rsplit_once('.')?.1.to_lowercase();

        self.map.get(&ext).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_extensions_resolve() {
        let lang = Languages::new();

        assert_eq!(lang.of("src/net.blk"), Some("c"));
        assert_eq!(lang.of("src/thing.pxc"), Some("c"));
        assert_eq!(lang.of("src/fast.pyx"), Some("python"));
        assert_eq!(lang.of("iface/api.iop"), Some("d"));
        assert_eq!(lang.of("page.tpl"), Some("jinja2"));
    }

    #[test]
    fn an_extension_nothing_claims_is_left_to_the_highlighter() {
        let lang = Languages::new();

        assert_eq!(lang.of("src/main.rs"), None);
        assert_eq!(lang.of("Makefile"), None);
        assert_eq!(lang.of("src/no.extension/README"), None);
    }

    #[test]
    fn the_case_of_the_extension_does_not_matter() {
        let lang = Languages::new();

        assert_eq!(lang.of("SRC/NET.BLK"), Some("c"));
    }

    #[test]
    fn a_user_entry_wins_and_a_leading_dot_is_allowed() {
        let mut lang = Languages::new();
        let user = HashMap::from([
            (".blk".to_owned(), "objective-c".to_owned()),
            ("zz".to_owned(), "yaml".to_owned()),
        ]);
        lang.extend(&user);

        assert_eq!(lang.of("a.blk"), Some("objective-c"));
        assert_eq!(lang.of("a.zz"), Some("yaml"));
    }

    #[test]
    fn a_dot_in_a_directory_name_is_not_an_extension() {
        let lang = Languages::new();

        assert_eq!(lang.of("some.dir/file"), None);
    }
}

#[cfg(test)]
mod bundled {
    use super::*;

    /// Every language the map names must exist in the grammar set we ship.
    /// A map entry that resolves to nothing is a file shown as plain text.
    #[test]
    fn every_language_of_the_map_is_bundled() {
        let set = two_face::syntax::extra_newlines();
        let lang = Languages::new();

        for ext in ["blk", "pxc", "pyx", "iop", "tpl"] {
            let name = lang.of(&format!("file.{ext}")).unwrap();
            assert!(
                set.find_syntax_by_token(name).is_some(),
                "no grammar for {name}, mapped from .{ext}"
            );
        }
    }
}

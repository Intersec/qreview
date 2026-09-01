//! What a commit is called, and who else reaches it.

use crate::git::exec::Git;

/// The commits the tags point at, and the name of the first tag on each.
///
/// One call for the whole repository. `git tag --points-at` reads every tag
/// to answer about one commit, so asking per commit reads them all again for
/// every commit of a walk.
pub async fn tags_by_commit(git: &Git) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Ok(text) = git
        .text(&[
            "for-each-ref",
            "--format=%(objectname) %(*objectname) %(refname:short)",
            "refs/tags",
        ])
        .await
    else {
        return out;
    };

    for line in text.lines() {
        let mut parts = line.splitn(3, ' ');
        let object = parts.next().unwrap_or("");
        // An annotated tag names a tag object. The commit is the peeled one.
        let peeled = parts.next().unwrap_or("");
        let Some(name) = parts.next().map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };

        let commit = match peeled.is_empty() {
            true => object,
            false => peeled,
        };
        // `for-each-ref` sorts by name, so the first one wins.
        out.entry(commit.to_owned())
            .or_insert_with(|| name.to_owned());
    }
    out
}

/// The first remote-tracking ref that reaches the commit, when one does.
pub async fn is_on_a_remote(git: &Git, hash: &str) -> Option<String> {
    let out = git
        .text(&[
            "for-each-ref",
            "--count=1",
            "--format=%(refname:short)",
            &format!("--contains={hash}"),
            "refs/remotes",
        ])
        .await
        .ok()?;

    let name = out.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// What the commit is called, for a person to read.
pub async fn name_of(git: &Git, hash: &str) -> String {
    let named = git
        .text(&["name-rev", "--name-only", "--always", hash])
        .await
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();

    if named.is_empty() || named == "undefined" {
        return hash[..hash.len().min(12)].to_owned();
    }
    named
}

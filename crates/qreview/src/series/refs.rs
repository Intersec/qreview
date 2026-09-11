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

/// Every remote-tracking ref that reaches the commit.
pub async fn remotes_holding(git: &Git, hash: &str) -> Vec<String> {
    let Ok(out) = git
        .text(&[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("--contains={hash}"),
            "refs/remotes",
        ])
        .await
    else {
        return Vec::new();
    };

    out.lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The remote branch a commit sits on, when one holds it and does not hold
/// the head of the series.
///
/// `on_head` is what `remotes_holding` answers for the head. A ref that
/// reaches the head reaches the whole series, so it says nothing about where
/// the series starts: a branch pushed for review is exactly that ref. Gerrit
/// bounds a push with the branch heads and never with the ref being pushed,
/// and this is the same rule.
pub async fn is_on_a_remote(git: &Git, hash: &str, on_head: &[String]) -> Option<String> {
    remotes_holding(git, hash)
        .await
        .into_iter()
        .find(|name| !on_head.iter().any(|held| held == name))
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

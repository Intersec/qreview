//! What a commit is called, and who else reaches it.

use crate::git::exec::Git;

/// The tags that point at a commit.
pub async fn tags_at(git: &Git, hash: &str) -> Vec<String> {
    let Ok(out) = git.text(&["tag", "--points-at", hash]).await else {
        return Vec::new();
    };
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
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

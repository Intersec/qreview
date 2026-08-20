//! What identifies a repository.
//!
//! The comment store is keyed by this, so it must be the same for two clones
//! of the same project and for two worktrees of the same clone.

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::git::exec::Git;
use crate::model::RepoInfo;

/// Read the identity of a repository.
pub async fn info(git: &Git) -> Result<RepoInfo> {
    let remote = remote_url(git).await;
    let canonical = remote.as_deref().map(canonical);

    // With no remote, the path is all there is. It is weaker: a second clone
    // is a second identity.
    let seed = canonical.clone().unwrap_or_else(|| {
        git.root()
            .canonicalize()
            .unwrap_or_else(|_| git.root().to_path_buf())
            .to_string_lossy()
            .into_owned()
    });

    Ok(RepoInfo {
        root: git.root().to_string_lossy().into_owned(),
        remote: canonical,
        id: id_of(&seed),
    })
}

async fn remote_url(git: &Git) -> Option<String> {
    for name in ["origin", "gerrit"] {
        if let Ok(url) = git.text(&["remote", "get-url", name]).await {
            let url = url.trim().to_owned();
            if !url.is_empty() {
                return Some(url);
            }
        }
    }

    // Any remote is better than none.
    let list = git.text(&["remote"]).await.ok()?;
    let first = list.lines().next()?.trim().to_owned();
    let url = git.text(&["remote", "get-url", &first]).await.ok()?;

    Some(url.trim().to_owned()).filter(|u| !u.is_empty())
}

/// The form two spellings of the same remote share.
///
/// The user, the port, the scheme, and a trailing `.git` all vary between
/// clones of one project, so none of them belongs in the identity.
pub fn canonical(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .trim();

    // Drop a user, with or without a password.
    let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);

    // `host:port/path`, `host:path` from the scp-like form, or `host/path`.
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path.to_owned()),
        None => match rest.split_once(':') {
            Some((host, path)) => (host, path.to_owned()),
            None => (rest, String::new()),
        },
    };

    let host = host.split_once(':').map(|(h, _)| h).unwrap_or(host);
    let path = path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git");

    format!("{}/{}", host.to_lowercase(), path)
}

fn id_of(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());

    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit};

    #[test]
    fn the_spellings_of_one_remote_agree() {
        let forms = [
            "ssh://review.example.com:29418/myproject",
            "ssh://user@review.example.com:29418/myproject.git",
            "review.example.com:myproject.git",
            "https://review.example.com/myproject",
            "ssh://review.example.com/myproject/",
        ];

        for form in forms {
            assert_eq!(canonical(form), "review.example.com/myproject", "{form}");
        }
    }

    #[test]
    fn two_projects_do_not_collide() {
        assert_ne!(
            canonical("ssh://review.example.com/one"),
            canonical("ssh://review.example.com/two")
        );
        assert_ne!(
            canonical("ssh://a.example.com/project"),
            canonical("ssh://b.example.com/project")
        );
    }

    #[test]
    fn a_nested_path_is_kept_whole() {
        assert_eq!(
            canonical("ssh://review.example.com:29418/group/sub/myproject.git"),
            "review.example.com/group/sub/myproject"
        );
    }

    #[tokio::test]
    async fn a_repository_with_a_remote_is_keyed_by_it() {
        let repo = build_repo(&[commit("first").file("a", "1\n")]).await;
        repo.remote(
            "origin",
            "ssh://user@review.example.com:29418/myproject.git",
        )
        .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let info = info(&git).await.unwrap();

        assert_eq!(info.remote.as_deref(), Some("review.example.com/myproject"));
        assert_eq!(info.id.len(), 16);
    }

    #[tokio::test]
    async fn a_repository_without_a_remote_falls_back_to_its_path() {
        let repo = build_repo(&[commit("first").file("a", "1\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let info = info(&git).await.unwrap();

        assert_eq!(info.remote, None);
        assert_eq!(info.id.len(), 16);
    }

    #[tokio::test]
    async fn two_worktrees_of_one_clone_share_the_identity() {
        let repo = build_repo(&[commit("first").file("a", "1\n")]).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        let tree = repo.path().join("..").join("second-worktree");
        repo.git(&["worktree", "add", tree.to_str().unwrap(), "-b", "other"])
            .await;

        let one = info(&Git::discover(repo.path()).await.unwrap())
            .await
            .unwrap();
        let two = info(&Git::discover(&tree).await.unwrap()).await.unwrap();

        assert_eq!(one.id, two.id);
        repo.git(&["worktree", "remove", "--force", tree.to_str().unwrap()])
            .await;
    }
}

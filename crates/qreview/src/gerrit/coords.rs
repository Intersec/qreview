//! Where the Gerrit server is, read from the repository.

use crate::git::commit;
use crate::git::exec::Git;

/// The default ssh port of a Gerrit server.
const PORT: u16 = 29418;

/// Everything the ssh query needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coordinates {
    pub host: String,
    pub port: u16,
    pub project: String,
    pub branch: Option<String>,
}

/// Read the host, the port and the project from a remote URL.
///
/// Only an `ssh://` remote can be a Gerrit: the ssh API is the only one this
/// tool speaks, and an https remote says nothing about where it lives.
pub fn from_remote(url: &str) -> Option<Coordinates> {
    let rest = url.trim().strip_prefix("ssh://")?;
    let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
    let (authority, path) = rest.split_once('/')?;

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().unwrap_or(PORT)),
        None => (authority, PORT),
    };

    let project = path.trim_end_matches('/').trim_end_matches(".git");
    if host.is_empty() || project.is_empty() {
        return None;
    }

    Some(Coordinates {
        host: host.to_owned(),
        port,
        project: project.to_owned(),
        branch: None,
    })
}

/// The coordinates of a repository, with the target branch filled in.
///
/// The branch comes from the `.gerrit-branch` file of the reviewed commit,
/// then from the configuration, then from the upstream branch name.
pub async fn of_repo(git: &Git, rev: &str, configured: Option<&str>) -> Option<Coordinates> {
    let url = git.text(&["remote", "get-url", "origin"]).await.ok()?;
    let mut coords = from_remote(url.trim())?;

    coords.branch = match commit::gerrit_branch(git, rev).await {
        Some(branch) => Some(branch),
        None => match configured {
            Some(branch) => Some(branch.to_owned()),
            None => upstream_branch(git).await,
        },
    };
    Some(coords)
}

async fn upstream_branch(git: &Git) -> Option<String> {
    let out = git
        .text(&[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .await
        .ok()?;
    let name = out.trim();

    name.split_once('/').map(|(_, branch)| branch.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_remote_gives_the_host_the_port_and_the_project() {
        let coords = from_remote("ssh://review.example.com:29418/myproject").unwrap();

        assert_eq!(coords.host, "review.example.com");
        assert_eq!(coords.port, 29418);
        assert_eq!(coords.project, "myproject");
    }

    #[test]
    fn a_user_and_a_git_suffix_are_dropped() {
        let coords = from_remote("ssh://someone@review.example.com:29418/group/sub.git").unwrap();

        assert_eq!(coords.host, "review.example.com");
        assert_eq!(coords.project, "group/sub");
    }

    #[test]
    fn a_missing_port_is_the_gerrit_default() {
        assert_eq!(
            from_remote("ssh://review.example.com/p").unwrap().port,
            29418
        );
    }

    #[test]
    fn a_remote_that_is_not_ssh_is_not_a_gerrit() {
        assert!(from_remote("https://review.example.com/myproject").is_none());
        assert!(from_remote("review.example.com:myproject.git").is_none());
        assert!(from_remote("/home/someone/a-local-clone").is_none());
    }

    #[test]
    fn a_remote_with_no_project_is_refused() {
        assert!(from_remote("ssh://review.example.com:29418/").is_none());
    }
}

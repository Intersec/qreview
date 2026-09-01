//! Is a newer qreview out?
//!
//! The tool asks the address the configuration names, which is the home of
//! the project unless a reader says otherwise. An empty address asks
//! nobody, and that is how the check is turned off.
//!
//! Every failure is silence. A machine with no network, a server that is
//! down, an address that wants a token, curl that is not installed: none of
//! them is a reason to say anything to a reader who came to read a diff.
//!
//! `curl` and not a crate: the tool already runs `git` and `ssh` as child
//! processes, curl is on every machine that has git, it costs no dependency
//! and no bytes in the binary, and it brings the certificates of the system
//! with it.

use serde::Serialize;
use tokio::process::Command;

use crate::config::Update;

/// How long the whole thing may take. The interface asks for this after it
/// has painted, so nothing waits on it, but a hung server must not hold a
/// connection for the life of the run.
const SECONDS: &str = "3";

/// What the interface shows beside the version it runs.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// The tag of the newest release, when one was found.
    pub latest: Option<String>,
    /// The page of that release, when the answer names it.
    pub url: Option<String>,
    /// True when `latest` is newer than what is running.
    pub newer: bool,
}

/// Ask, and answer with what the interface should show.
pub async fn check(config: &Update, running: &str) -> Release {
    let Some(url) = config.url.as_deref().filter(|url| !url.is_empty()) else {
        return Release::default();
    };
    let Some(body) = fetch(url, config.token.as_deref()).await else {
        return Release::default();
    };
    let Some((latest, page)) = read(&body) else {
        return Release::default();
    };

    Release {
        newer: newer(running, &latest),
        latest: Some(latest),
        url: page,
    }
}

async fn fetch(url: &str, token: Option<&str>) -> Option<String> {
    let mut call = Command::new("curl");
    call.args(["--fail", "--silent", "--show-error", "--max-time", SECONDS]);

    if let Some(token) = token.filter(|token| !token.is_empty()) {
        call.arg("--header")
            .arg(format!("Authorization: Bearer {token}"));
    }
    let started = crate::trace::start();
    let out = call.arg(url).output().await.ok()?;
    // The token is a header, never a part of the address.
    crate::trace::since(started, || format!("curl {url}"));

    match out.status.success() {
        true => String::from_utf8(out.stdout).ok(),
        false => None,
    }
}

/// The tag and the page of the release, out of the answer.
///
/// `tag_name` and `html_url` are what the releases API of GitHub answers
/// with. Any address that says the same two things works.
fn read(body: &str) -> Option<(String, Option<String>)> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = json.get("tag_name")?.as_str()?.trim().to_owned();

    if tag.is_empty() {
        return None;
    }
    let page = json
        .get("html_url")
        .and_then(|value| value.as_str())
        .map(str::to_owned);

    Some((tag, page))
}

/// Is `found` a later version than `running`?
///
/// Both are read as numbers, so 0.10.0 is later than 0.9.0. Anything that
/// does not read as three numbers says nothing: a tag nobody can compare is
/// not a reason to tell a reader they are behind.
pub fn newer(running: &str, found: &str) -> bool {
    match (parts(running), parts(found)) {
        (Some(here), Some(there)) => there > here,
        _ => false,
    }
}

fn parts(version: &str) -> Option<[u64; 3]> {
    let mut out = [0u64; 3];
    let text = version.trim().trim_start_matches('v');
    let mut fields = text.split('.');

    for slot in &mut out {
        *slot = fields.next()?.parse().ok()?;
    }
    match fields.next() {
        None => Some(out),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_version_is_later() {
        assert!(newer("0.5.0", "0.5.1"));
        assert!(newer("0.5.0", "v0.6.0"));
        assert!(newer("0.9.0", "0.10.0"), "read as numbers, not as text");
        assert!(newer("1.9.9", "2.0.0"));
    }

    #[test]
    fn the_same_version_or_an_older_one_is_not() {
        assert!(!newer("0.5.0", "0.5.0"));
        assert!(!newer("0.5.0", "v0.5.0"));
        assert!(!newer("0.5.1", "0.5.0"));
        assert!(!newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn a_tag_that_is_not_a_version_says_nothing() {
        assert!(!newer("0.5.0", "nightly"));
        assert!(!newer("0.5.0", "v0.5"));
        assert!(!newer("0.5.0", "0.5.0.1"));
        assert!(!newer("0.5.0", ""));
    }

    #[test]
    fn the_tag_and_the_page_are_read_from_the_answer() {
        let answer = r#"{"tag_name":"v0.6.0","name":"qreview v0.6.0",
            "html_url":"https://example.com/Intersec/qreview/releases/tag/v0.6.0"}"#;

        assert_eq!(
            read(answer),
            Some((
                "v0.6.0".to_owned(),
                Some("https://example.com/Intersec/qreview/releases/tag/v0.6.0".to_owned())
            ))
        );
    }

    #[test]
    fn an_answer_that_says_nothing_useful_is_dropped() {
        assert_eq!(read("not json"), None);
        assert_eq!(read("{}"), None);
        assert_eq!(read(r#"{"tag_name":""}"#), None);
        assert_eq!(
            read(r#"{"tag_name":"v1.0.0"}"#),
            Some(("v1.0.0".to_owned(), None))
        );
    }

    #[tokio::test]
    async fn no_address_asks_nothing() {
        for quiet in [None, Some(String::new())] {
            let config = Update {
                url: quiet,
                token: None,
            };

            assert_eq!(check(&config, "0.5.0").await, Release::default());
        }
    }
}

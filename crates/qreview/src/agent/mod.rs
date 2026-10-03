//! The commands of an agent that works on the series in a terminal.
//!
//! They write as the agent, through the server that runs on the
//! repository, so the server stays the only writer of the store and the
//! browser hears of every write. None of them starts a server.

pub mod address;
pub mod client;
pub mod socket;

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use address::Address;
use client::Client;

use crate::git::exec::Git;
use crate::store::Store;

/// The directory of the store of the repository that holds `cwd`.
pub async fn store_dir(cwd: &Path) -> Result<std::path::PathBuf> {
    let git = Git::discover(cwd).await?;
    let repo = crate::repo::info(&git).await?;

    Ok(Store::open(&repo.id)?.dir().to_path_buf())
}

/// The server that runs on the repository, when one answers.
///
/// A file whose server does not answer was left by a crash, and is no
/// server at all.
pub async fn running(dir: &Path) -> Option<Client> {
    let client = Client::new(address::read(dir)?);

    client.get("/api/events").await.ok().map(|_| client)
}

/// The server of the repository, or an error that says what to run.
pub async fn connect(cwd: &Path) -> Result<Client> {
    let dir = store_dir(cwd).await?;

    running(&dir)
        .await
        .context("no qreview server runs on this repository. Run `qreview` first")
}

/// A body, or standard input when it is `-`, so a long answer needs no
/// quoting.
pub fn body_of(text: &str) -> Result<String> {
    if text != "-" {
        return Ok(text.to_owned());
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut out)
        .context("cannot read the body from standard input")?;
    Ok(out)
}

/// Where a remark of the agent goes.
#[derive(Debug, PartialEq, Eq)]
pub struct Place {
    pub file: String,
    /// `old` or `new`, and the lines. None for a remark about the file.
    pub lines: Option<(String, usize, usize)>,
}

/// Read `<file>`, `<file>:<side>:<line>` or `<file>:<side>:<from>-<to>`.
///
/// A path may hold a colon, so the side and the line are read from the
/// right, and a tail that is not a side and a line is part of the path.
pub fn parse_place(text: &str) -> Result<Place> {
    let mut parts = text.rsplitn(3, ':');
    let (last, side, file) = (parts.next(), parts.next(), parts.next());

    if let (Some(lines), Some(side @ ("old" | "new")), Some(file)) = (last, side, file) {
        let (from, to) = match lines.split_once('-') {
            Some((from, to)) => (from, to),
            None => (lines, lines),
        };
        let (Ok(from), Ok(to)) = (from.parse::<usize>(), to.parse::<usize>()) else {
            bail!("{text}: the line must be a number, or a range like 12-14");
        };
        if from == 0 || to < from {
            bail!("{text}: lines count from 1, and a range goes down the file");
        }
        return Ok(Place {
            file: file.to_owned(),
            lines: Some((side.to_owned(), from, to)),
        });
    }
    if text.is_empty() {
        bail!("name a file, as <file> or <file>:<side>:<line>");
    }
    Ok(Place {
        file: text.to_owned(),
        lines: None,
    })
}

/// Write a remark as the agent.
pub async fn comment(
    client: &Client,
    place: &Place,
    body: &str,
    key: Option<&str>,
) -> Result<Value> {
    let key = match key {
        Some(key) => key.to_owned(),
        None => change_with(client, &place.file).await?,
    };
    let mut new = json!({ "body": body, "author": "agent", "file": place.file });
    match &place.lines {
        Some((side, from, to)) => {
            new["scope"] = json!(if from == to { "line" } else { "range" });
            new["side"] = json!(side);
            new["startLine"] = json!(from);
            new["endLine"] = json!(to);
        }
        None => new["scope"] = json!("file"),
    }

    client
        .post(&format!("/api/changes/{}/comments", segment(&key)), &new)
        .await
}

/// The newest change of the series that touches the file.
async fn change_with(client: &Client, file: &str) -> Result<String> {
    let session = client.get("/api/session").await?;
    let changes = session["series"]["changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    for change in changes {
        let Some(key) = change["key"].as_str() else {
            continue;
        };
        let files = client
            .get(&format!("/api/changes/{}/files", segment(key)))
            .await?;
        let touches = files.as_array().into_iter().flatten().any(|entry| {
            entry["path"].as_str() == Some(file) || entry["oldPath"].as_str() == Some(file)
        });
        if touches {
            return Ok(key.to_owned());
        }
    }
    bail!("no change of the series touches {file}. Name one with --key")
}

/// Reply to a thread as the agent.
pub async fn reply(
    client: &Client,
    id: &str,
    body: &str,
    done: bool,
    blocked: bool,
) -> Result<Value> {
    let key = key_of(client, id).await?;
    let mut new = json!({
        "scope": "change",
        "parent": id,
        "author": "agent",
        "body": body,
        "blocked": blocked,
    });
    if done {
        new["done"] = json!(true);
    }

    client
        .post(&format!("/api/changes/{}/comments", segment(&key)), &new)
        .await
}

/// The change that holds a comment.
async fn key_of(client: &Client, id: &str) -> Result<String> {
    let all = client.get("/api/comments").await?;

    all.as_array()
        .into_iter()
        .flatten()
        .find(|change| {
            change["comments"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["id"].as_str() == Some(id))
        })
        .and_then(|change| change["key"].as_str().map(str::to_owned))
        .with_context(|| format!("no comment {id} in this series"))
}

/// Read the repository again, as the ⟳ button does.
pub async fn refresh(client: &Client) -> Result<()> {
    client
        .post("/api/session/refresh?author=agent", &json!({}))
        .await
        .map(|_| ())
}

/// Wait for the next writes of the reader.
///
/// Answers with the events, each with the comment it names, and the number
/// to pass as `after` next time. The flag is false when the time ran out
/// with nothing to report.
pub async fn wait(client: &Client, after: Option<u64>, timeout: Duration) -> Result<(Value, bool)> {
    let deadline = Instant::now() + timeout;
    let mut after = match after {
        Some(after) => after,
        None => client.get("/api/events").await?["next"]
            .as_u64()
            .unwrap_or_default(),
    };

    loop {
        let batch = client.get(&format!("/api/events?after={after}")).await?;
        let next = batch["next"].as_u64().unwrap_or(after);
        if batch["reset"].as_bool() == Some(true) {
            return Ok((json!({ "events": [], "next": next, "reset": true }), true));
        }

        let theirs: Vec<Value> = batch["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["author"].as_str() == Some("reader"))
            .cloned()
            .collect();
        if !theirs.is_empty() {
            let events = with_comments(client, theirs).await?;
            return Ok((json!({ "events": events, "next": next }), true));
        }

        after = next;
        if Instant::now() >= deadline {
            return Ok((json!({ "events": [], "next": after }), false));
        }
    }
}

/// Each event with the comment it names, so the agent reads the remark
/// without a second call.
async fn with_comments(client: &Client, events: Vec<Value>) -> Result<Vec<Value>> {
    let all = client.get("/api/comments").await?;
    let find = |id: &str| -> Option<Value> {
        all.as_array()?
            .iter()
            .flat_map(|change| change["comments"].as_array().into_iter().flatten())
            .find(|c| c["id"].as_str() == Some(id))
            .cloned()
    };

    Ok(events
        .into_iter()
        .map(|mut event| {
            if let Some(found) = event["id"].as_str().and_then(find) {
                event["comment"] = found;
            }
            event
        })
        .collect())
}

/// A key as one segment of a path. A Change-Id and `sha-<hash>` need no
/// escape; anything else is refused rather than sent somewhere else.
fn segment(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Write the address of a server that starts.
pub fn announce(dir: &Path, port: u16, token: &str, socket: Option<&Path>) -> Result<Address> {
    let address = Address {
        pid: std::process::id(),
        port,
        token: token.to_owned(),
        socket: socket.map(|path| path.display().to_string()),
    };
    address::write(dir, &address)?;

    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(file: &str, side: &str, from: usize, to: usize) -> Place {
        Place {
            file: file.to_owned(),
            lines: Some((side.to_owned(), from, to)),
        }
    }

    #[test]
    fn a_place_names_a_file_a_side_and_a_line() {
        assert_eq!(
            parse_place("src/a.c:new:12").unwrap(),
            line("src/a.c", "new", 12, 12)
        );
        assert_eq!(
            parse_place("src/a.c:old:3-5").unwrap(),
            line("src/a.c", "old", 3, 5)
        );
    }

    #[test]
    fn a_place_with_no_line_is_the_file() {
        assert_eq!(
            parse_place("src/a.c").unwrap(),
            Place {
                file: "src/a.c".to_owned(),
                lines: None
            }
        );
    }

    #[test]
    fn a_colon_in_a_path_stays_in_the_path() {
        assert_eq!(
            parse_place("a:b.c:new:2").unwrap(),
            line("a:b.c", "new", 2, 2)
        );
        assert_eq!(parse_place("a:b.c").unwrap().file, "a:b.c");
    }

    #[test]
    fn a_line_that_is_not_one_is_refused() {
        assert!(parse_place("a.c:new:x").is_err());
        assert!(parse_place("a.c:new:0").is_err());
        assert!(parse_place("a.c:new:5-3").is_err());
        assert!(parse_place("").is_err());
    }

    #[test]
    fn a_key_never_leaves_its_segment() {
        assert_eq!(segment("I8f3a"), "I8f3a");
        assert_eq!(segment("sha-4a91"), "sha-4a91");
        assert_eq!(segment("../x?y"), "xy");
    }
}

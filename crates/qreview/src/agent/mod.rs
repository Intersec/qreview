//! The commands of an agent that works on the series in a terminal.
//!
//! They write as the agent, through the server that runs on the
//! repository, so the server stays the only writer of the store and the
//! browser hears of every write. None of them starts a server.

pub mod address;
pub mod client;
pub mod socket;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use address::Address;
use client::Client;

use crate::git::exec::Git;
use crate::store::Store;

/// The procedure that an agent follows, printed by `qreview skill`.
///
/// It ships in the binary, so it names the commands of this release, and
/// the skill that an agent installs only points at it.
pub const SKILL: &str = include_str!("skill.md");

/// Where the server of a worktree says how to reach it.
///
/// The worktrees of a clone share one store, but each runs a server of its
/// own. So the address file and the socket carry the worktree in their
/// name, and the store is the directory they sit in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    /// The directory of the store of the repository.
    pub dir: PathBuf,
    pub repo: String,
    pub worktree: String,
}

/// The home of the worktree that holds `cwd`.
pub async fn home(cwd: &Path) -> Result<Home> {
    let git = Git::discover(cwd).await?;
    let repo = crate::repo::info(&git).await?;

    Ok(Home {
        dir: Store::open(&repo.id)?.dir().to_path_buf(),
        worktree: crate::repo::worktree_id(git.root()),
        repo: repo.id,
    })
}

/// The server that runs on the worktree, when one answers.
///
/// A file whose server does not answer was left by a crash, and is no
/// server at all.
pub async fn running(home: &Home) -> Option<Client> {
    let client = Client::new(address::read(home)?);

    client.get("/api/events").await.ok().map(|_| client)
}

/// Fail when this process cannot write the store of the worktree.
///
/// A server that cannot write it would lose every comment. A sandbox that
/// keeps qreview in is the usual cause, so the error says where to start it.
pub fn writable(home: &Home) -> Result<()> {
    Store::at(&home.dir).lock().map(drop).context(
        "start qreview outside the sandbox, if a sandbox runs this command. \
         qreview cannot write its store, and would lose every comment",
    )
}

/// The server of the worktree, or an error that says what to run.
pub async fn connect(cwd: &Path) -> Result<Client> {
    let home = home(cwd).await?;

    running(&home)
        .await
        .context("no qreview server runs on this worktree. Run `qreview` first")
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

/// Declare `to` the next version of `from`, and move the threads onto it.
pub async fn migrate(
    client: &Client,
    from: &str,
    to: Option<&str>,
    done: &[String],
    blocked: &[String],
) -> Result<Value> {
    let mut asked = Vec::new();
    for item in blocked {
        let Some((id, body)) = item.split_once('=') else {
            bail!("--blocked takes <id>=<text>, not {item}");
        };
        asked.push(json!({ "id": id, "body": body }));
    }
    let done: Vec<&str> = done
        .iter()
        .flat_map(|ids| ids.split(','))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();

    client
        .post(
            "/api/migrate",
            &json!({ "from": from, "to": to, "done": done, "blocked": asked }),
        )
        .await
}

/// Read the repository again, as the ⟳ button does.
pub async fn refresh(client: &Client) -> Result<()> {
    client
        .post("/api/session/refresh?author=agent", &json!({}))
        .await
        .map(|_| ())
}

/// The kinds of event that ask something of the agent. Every other write is
/// for the page alone: a remark being written, changed or deleted costs the
/// agent nothing until the reader asks.
const REQUESTS: &[&str] = &["ask", "fix"];

/// The requests among the events of a batch.
fn requests(events: &Value) -> Vec<Value> {
    events
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| {
            e["kind"]
                .as_str()
                .is_some_and(|kind| REQUESTS.contains(&kind))
        })
        .cloned()
        .collect()
}

/// How often a wait looks for a server that stopped.
const RETRY: Duration = Duration::from_secs(1);

/// Wait for the next request of the reader.
///
/// Answers with the requests, each with the thread it names, and the number
/// to pass as `after` next time. The flag is false when the time ran out
/// with nothing to report.
///
/// A server that stops does not end the wait: the user may start it again,
/// and the agent then stays attached with no word from it. A new server
/// numbers its events again, so the wait answers `reset` then.
pub async fn wait(home: &Home, after: Option<u64>, timeout: Duration) -> Result<(Value, bool)> {
    let deadline = Instant::now() + timeout;
    let mut client = reach(home, deadline).await?;
    let mut after = match after {
        Some(after) => after,
        None => client.get("/api/events").await?["next"]
            .as_u64()
            .unwrap_or_default(),
    };

    loop {
        let Ok(batch) = client
            .get(&format!("/api/events?after={after}&who=agent"))
            .await
        else {
            let back = reach(home, deadline).await?;
            if back.address() != client.address() {
                let next = back.get("/api/events").await?["next"]
                    .as_u64()
                    .unwrap_or_default();
                return Ok((json!({ "events": [], "next": next, "reset": true }), true));
            }
            client = back;
            continue;
        };
        let next = batch["next"].as_u64().unwrap_or(after);
        if batch["reset"].as_bool() == Some(true) {
            return Ok((json!({ "events": [], "next": next, "reset": true }), true));
        }

        let asked = requests(&batch["events"]);
        if !asked.is_empty() {
            let events = with_threads(&client, asked).await?;
            return Ok((json!({ "events": events, "next": next }), true));
        }

        after = next;
        if Instant::now() >= deadline {
            return Ok((json!({ "events": [], "next": after }), false));
        }
    }
}

/// The server of the worktree, as soon as one answers before the deadline.
async fn reach(home: &Home, deadline: Instant) -> Result<Client> {
    loop {
        if let Some(client) = running(home).await {
            return Ok(client);
        }
        if Instant::now() >= deadline {
            bail!("no qreview server runs on this worktree. Run `qreview` first");
        }
        tokio::time::sleep(RETRY).await;
    }
}

/// Each request with the thread it names, the remark first and the replies
/// in order, so the agent answers without a second call. A thread deleted
/// since the request carries none.
async fn with_threads(client: &Client, events: Vec<Value>) -> Result<Vec<Value>> {
    let all = client.get("/api/comments").await?;
    let comments: Vec<&Value> = all
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|change| change["comments"].as_array().into_iter().flatten())
        .collect();

    Ok(events
        .into_iter()
        .map(|mut event| {
            if let Some(id) = event["id"].as_str() {
                event["thread"] = Value::Array(thread_of(&comments, id));
            }
            event
        })
        .collect())
}

/// The remark `id` and its replies, oldest first.
fn thread_of(comments: &[&Value], id: &str) -> Vec<Value> {
    let mut thread: Vec<Value> = comments
        .iter()
        .filter(|c| c["id"].as_str() == Some(id) || c["parent"].as_str() == Some(id))
        .map(|c| (*c).clone())
        .collect();
    thread.sort_by(|a, b| {
        let first = |c: &Value| c["parent"].is_string();
        first(a)
            .cmp(&first(b))
            .then_with(|| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()))
    });
    thread
}

/// A key as one segment of a path. A Change-Id and `sha-<hash>` need no
/// escape; anything else is refused rather than sent somewhere else.
fn segment(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Write the address of a server that starts.
pub fn announce(home: &Home, port: u16, token: &str, socket: Option<&Path>) -> Result<Address> {
    let address = Address {
        pid: std::process::id(),
        port,
        token: token.to_owned(),
        socket: socket.map(|path| path.display().to_string()),
    };
    address::write(home, &address)?;

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
    fn only_a_request_wakes_the_agent() {
        let events = json!([
            { "kind": "comment", "author": "reader", "id": "c-1" },
            { "kind": "deleted", "author": "reader", "id": "c-1" },
            { "kind": "ask", "author": "reader", "id": "c-2" },
            { "kind": "reply", "author": "agent", "id": "c-3" },
        ]);

        let asked = requests(&events);

        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0]["id"], "c-2");
    }

    #[test]
    fn a_thread_is_the_remark_then_its_replies_in_order() {
        let remark = json!({ "id": "c-1", "parent": null, "createdAt": "3" });
        let later = json!({ "id": "c-3", "parent": "c-1", "createdAt": "2" });
        let earlier = json!({ "id": "c-2", "parent": "c-1", "createdAt": "1" });
        let other = json!({ "id": "c-4", "parent": null, "createdAt": "0" });

        let thread = thread_of(&[&later, &other, &remark, &earlier], "c-1");

        let ids: Vec<_> = thread.iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["c-1", "c-2", "c-3"]);
        assert!(
            thread_of(&[&other], "c-gone").is_empty(),
            "a deleted thread"
        );
    }

    #[test]
    fn a_store_that_cannot_be_written_stops_the_server() {
        let dir = tempfile::tempdir().unwrap();
        let mut home = Home {
            dir: dir.path().join("repos").join("4f59"),
            repo: "4f59".to_owned(),
            worktree: "a1b2".to_owned(),
        };
        writable(&home).unwrap();

        // A file where a directory must be fails as a read-only file system
        // does, and holds even for root.
        std::fs::write(dir.path().join("file"), "").unwrap();
        home.dir = dir.path().join("file").join("4f59");
        let error = writable(&home).unwrap_err();

        assert!(
            format!("{error:#}").contains("outside the sandbox"),
            "{error:#}"
        );
    }

    #[test]
    fn a_key_never_leaves_its_segment() {
        assert_eq!(segment("I8f3a"), "I8f3a");
        assert_eq!(segment("sha-4a91"), "sha-4a91");
        assert_eq!(segment("../x?y"), "xy");
    }
}

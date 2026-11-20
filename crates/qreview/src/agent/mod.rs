//! The commands of an agent that works on the series in a terminal.
//!
//! They write as the agent, through the server that runs on the
//! repository, so the server stays the only writer of the store and the
//! browser hears of every write. None of them starts a server.

pub mod address;
pub mod client;
pub mod shared;
pub mod socket;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use address::Address;
use client::Client;

use crate::comments::NOT_ON_GERRIT;
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
    /// The git directory of the worktree, where the server also says how
    /// to reach it. See `shared`.
    pub git_dir: Option<PathBuf>,
}

/// The home of the worktree that holds `cwd`.
pub async fn home(cwd: &Path) -> Result<Home> {
    let git = Git::discover(cwd).await?;
    let repo = crate::repo::info(&git).await?;

    let git_dir = git
        .text(&["rev-parse", "--absolute-git-dir"])
        .await
        .ok()
        .map(|dir| PathBuf::from(dir.trim()));

    Ok(Home {
        dir: Store::open(&repo.id)?.dir().to_path_buf(),
        worktree: crate::repo::worktree_id(git.root()),
        repo: repo.id,
        git_dir,
    })
}

/// The server that runs on the worktree, when one answers.
///
/// A file whose server does not answer was left by a crash, and is no
/// server at all.
pub async fn running(home: &Home) -> Option<Client> {
    // The git directory first: in a container, it is the only one shared
    // with the machine of the server.
    let copy = home.git_dir.as_deref().and_then(shared::read);
    for found in copy.into_iter().chain(address::read(home)) {
        let client = Client::new(found);
        if client.get("/api/events").await.is_ok() {
            return Some(client);
        }
    }
    None
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
///
/// On the change of a colleague, a remark is for its author, so it is
/// staged for Gerrit unasked. The agent forgets a flag, mostly when another
/// skill writes the review. Where Gerrit does not hold the version, the
/// remark stays in qreview.
pub async fn comment(
    client: &Client,
    place: &Place,
    body: &str,
    key: Option<&str>,
    gerrit: bool,
) -> Result<Value> {
    let session = client.get("/api/session").await?;
    let key = match key {
        Some(key) => key.to_owned(),
        None => change_with(client, &session, &place.file).await?,
    };
    let colleague = session["series"]["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|change| change["key"].as_str() == Some(&key) && change["own"] == json!(false));
    let mut new = json!({
        "body": body,
        "author": "agent",
        "file": place.file,
    });
    match &place.lines {
        Some((side, from, to)) => {
            new["scope"] = json!(if from == to { "line" } else { "range" });
            new["side"] = json!(side);
            new["startLine"] = json!(from);
            new["endLine"] = json!(to);
        }
        None => new["scope"] = json!("file"),
    }
    let path = format!("/api/changes/{}/comments", segment(&key));
    if gerrit || colleague {
        let mut staged = new.clone();
        staged["gerrit"] = json!({});
        match client.post(&path, &staged).await {
            Err(error) if !gerrit && error.to_string().starts_with(NOT_ON_GERRIT) => {}
            written => return written,
        }
    }
    client.post(&path, &new).await
}

/// The newest change of the series that touches the file.
async fn change_with(client: &Client, session: &Value, file: &str) -> Result<String> {
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
    if let Some(id) = id.strip_prefix(GERRIT) {
        if done {
            bail!(
                "a thread of Gerrit is done when a reply resolves it there. Run `qreview stage gerrit:{id} --resolve --body Done`"
            );
        }
        return on_gerrit(client, id, body, None, blocked).await;
    }
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

/// Write a staged remark at the place of the thread that holds `id`.
///
/// The thread is the talk between the user and the agent, and stays in
/// qreview. The new remark is what the author of the change reads. See
/// section 5.8 of the design.
pub async fn stage(client: &Client, id: &str, body: &str, resolve: bool) -> Result<Value> {
    if let Some(id) = id.strip_prefix(GERRIT) {
        return on_gerrit(client, id, body, Some(resolve), false).await;
    }
    if resolve {
        bail!("--resolve answers a thread of Gerrit, and {id} is a thread of qreview");
    }
    let (key, comments) = change_of(client, id).await?;
    let find = |id: &str| comments.iter().find(|c| c["id"].as_str() == Some(id));
    let comment = find(id).with_context(|| format!("no comment {id} in this series"))?;
    // A thread is one remark and its replies: the place is the remark's.
    let root = match comment["parent"].as_str() {
        Some(parent) => {
            find(parent).with_context(|| format!("no comment {parent} in this series"))?
        }
        None => comment,
    };

    if root["gerrit"]["state"].as_str() == Some("sent") {
        bail!("the thread {id} is a draft on Gerrit already. Change it there");
    }
    let route = format!("/api/changes/{}/comments", segment(&key));
    let reply = json!({
        "scope": root["scope"],
        "parent": root["id"],
        "author": "agent",
        "body": body,
    });
    // A staged thread stays on its patch set after an amend, so its version
    // does not matter: the reply is the new text.
    if root["gerrit"]["state"].as_str() == Some("staged") {
        return client.post(&route, &reply).await;
    }

    let session = client.get("/api/session").await?;
    let current = session["series"]["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|change| change["key"].as_str() == Some(&key))
        .and_then(|change| change["commit"].as_str());
    if current != root["commit"].as_str() {
        bail!(
            "the thread {id} is on an older version of the change. Write the remark on the \
             current one with `qreview comment <place> --gerrit`"
        );
    }

    // One thread is one remark on Gerrit: the thread is staged, and its
    // newest reply is the text that goes out. Staged first, so a version
    // that Gerrit does not hold writes no reply.
    let root_id = root["id"].as_str().unwrap_or_default();
    client
        .patch(
            &format!("{route}/{}", segment(root_id)),
            &json!({ "gerrit": {} }),
        )
        .await?;
    client.post(&route, &reply).await
}

/// How a thread of Gerrit is named on the command line.
const GERRIT: &str = "gerrit:";

/// Reply to the thread of Gerrit that holds the comment `id`. With a
/// stage, the reply goes out in the thread at the next Send of the user.
async fn on_gerrit(
    client: &Client,
    id: &str,
    body: &str,
    stage: Option<bool>,
    blocked: bool,
) -> Result<Value> {
    let (key, thread) = gerrit_thread_of(client, id).await?;
    let mut new = json!({
        "scope": "line",
        "parent": format!("{GERRIT}{thread}"),
        "author": "agent",
        "body": body,
        "blocked": blocked,
    });
    if let Some(resolve) = stage {
        new["gerrit"] = json!({ "resolve": resolve });
    }

    client
        .post(&format!("/api/changes/{}/comments", segment(&key)), &new)
        .await
}

/// The change, and the thread, that hold a comment of Gerrit.
async fn gerrit_thread_of(client: &Client, id: &str) -> Result<(String, String)> {
    let all = client.get("/api/gerrit/threads").await?;

    for change in all.as_array().into_iter().flatten() {
        for thread in change["threads"].as_array().into_iter().flatten() {
            let holds = thread["comments"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["id"].as_str() == Some(id));
            if let (true, Some(key), Some(root)) =
                (holds, change["key"].as_str(), thread["id"].as_str())
            {
                return Ok((key.to_owned(), root.to_owned()));
            }
        }
    }
    bail!("no thread of Gerrit holds {id}. If Gerrit answers, run `qreview refresh` and try again")
}

/// The change that holds a comment, and every comment of that change.
async fn change_of(client: &Client, id: &str) -> Result<(String, Vec<Value>)> {
    let all = client.get("/api/comments").await?;

    all.as_array()
        .into_iter()
        .flatten()
        .find_map(|change| {
            let comments = change["comments"].as_array()?;
            comments
                .iter()
                .any(|c| c["id"].as_str() == Some(id))
                .then(|| Some((change["key"].as_str()?.to_owned(), comments.clone())))?
        })
        .with_context(|| format!("no comment {id} in this series"))
}

/// The change that holds a comment.
async fn key_of(client: &Client, id: &str) -> Result<String> {
    Ok(change_of(client, id).await?.0)
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
/// with nothing to report. Without a timeout, the wait never runs out.
///
/// A server that stops does not end the wait: the user may start it again,
/// and the agent then stays attached with no word from it. A new server
/// numbers its events again, so the wait answers `reset` then.
pub async fn wait(
    home: &Home,
    after: Option<u64>,
    timeout: Option<Duration>,
    name: Option<&str>,
) -> Result<(Value, bool)> {
    // Sent on every wait: a server that restarts learns it again.
    let named = name.map_or(String::new(), |name| {
        format!("&agent={}", crate::gerrit::rest::encode(name))
    });
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut client = reach(home, deadline).await?;
    let mut after = match after {
        Some(after) => after,
        None => client.get("/api/events").await?["next"]
            .as_u64()
            .unwrap_or_default(),
    };

    loop {
        // The server holds a wait for half a minute: a shorter time limit
        // says how long it may hold this one.
        let held = deadline
            .map(|deadline| {
                let left = deadline.saturating_duration_since(Instant::now());
                format!("&wait={}", left.as_millis())
            })
            .unwrap_or_default();
        let Ok(batch) = client
            .get(&format!("/api/events?after={after}&who=agent{held}{named}"))
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
        if past(deadline) {
            return Ok((json!({ "events": [], "next": after }), false));
        }
    }
}

/// The server of the worktree, as soon as one answers before the deadline.
async fn reach(home: &Home, deadline: Option<Instant>) -> Result<Client> {
    loop {
        if let Some(client) = running(home).await {
            return Ok(client);
        }
        if past(deadline) {
            bail!("no qreview server runs on this worktree. Run `qreview` first");
        }
        tokio::time::sleep(RETRY).await;
    }
}

fn past(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
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

    // The threads of Gerrit only when one is asked about: REST may be slow.
    let asks_gerrit = events
        .iter()
        .any(|e| e["id"].as_str().is_some_and(|id| id.starts_with(GERRIT)));
    let gerrit = match asks_gerrit {
        true => client.get("/api/gerrit/threads").await?,
        false => Value::Null,
    };

    Ok(events
        .into_iter()
        .map(|mut event| {
            let thread = match event["id"].as_str() {
                Some(id) => match id.strip_prefix(GERRIT) {
                    Some(id) => gerrit_thread(&gerrit, id),
                    None => thread_of(&comments, id),
                },
                None => return event,
            };
            event["thread"] = Value::Array(thread);
            event
        })
        .collect())
}

/// A thread of Gerrit as the agent reads it: the comments of Gerrit, each
/// with the name of its author, then the replies written here.
fn gerrit_thread(all: &Value, id: &str) -> Vec<Value> {
    let Some(thread) = all
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|change| change["threads"].as_array().into_iter().flatten())
        .find(|thread| thread["id"].as_str() == Some(id))
    else {
        return Vec::new();
    };
    let mut out: Vec<Value> = thread["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            json!({
                "id": format!("{GERRIT}{}", c["id"].as_str().unwrap_or_default()),
                "author": "gerrit",
                "name": c["author"],
                "body": c["body"],
            })
        })
        .collect();
    out.extend(thread["replies"].as_array().into_iter().flatten().cloned());
    out
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

/// Write the copy of the address in the git directory, naming the socket
/// there. None when the worktree has no git directory to write.
pub fn announce_shared(home: &Home, address: &Address, socket: &Path) -> Result<Option<Address>> {
    let Some(git_dir) = &home.git_dir else {
        return Ok(None);
    };
    let copy = Address {
        socket: Some(socket.display().to_string()),
        ..address.clone()
    };
    shared::write(git_dir, &copy)?;
    Ok(Some(copy))
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
    fn a_key_never_leaves_its_segment() {
        assert_eq!(segment("I8f3a"), "I8f3a");
        assert_eq!(segment("sha-4a91"), "sha-4a91");
        assert_eq!(segment("../x?y"), "xy");
    }

    #[test]
    fn a_thread_of_gerrit_reads_its_reviewers_then_the_replies_here() {
        let all = json!([{
            "key": "I1",
            "threads": [{
                "id": "g1",
                "comments": [
                    { "id": "g1", "author": "Jane", "body": "Why?" },
                    { "id": "g2", "author": "Bob", "body": "Indeed." }
                ],
                "replies": [{ "id": "c-1", "author": "reader", "body": "Answer this." }]
            }]
        }]);

        let thread = gerrit_thread(&all, "g1");

        let said: Vec<(&str, &str)> = thread
            .iter()
            .map(|c| (c["id"].as_str().unwrap(), c["body"].as_str().unwrap()))
            .collect();
        assert_eq!(
            said,
            [
                ("gerrit:g1", "Why?"),
                ("gerrit:g2", "Indeed."),
                ("c-1", "Answer this.")
            ]
        );
        assert_eq!(thread[0]["name"], "Jane");
        assert!(gerrit_thread(&all, "gone").is_empty());
    }
}

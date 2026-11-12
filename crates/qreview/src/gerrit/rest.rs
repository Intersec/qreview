//! The REST API of Gerrit, for what ssh cannot do.
//!
//! The ssh query gives no comment id, no thread link and no resolved state,
//! and ssh cannot write a draft. Everything else stays on ssh. See section
//! 6.6 of the design.
//!
//! `curl` and not a crate, as for the release check: the authorization
//! reaches it on standard input, in a `--config -` file, and never in an
//! argument that any local user reads in `ps`.
//!
//! qreview holds no secret of its own. The command of `gerrit.auth.command`
//! gives the authorization, and qreview keeps it in memory.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

/// How long a call may take before the review goes on without it.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// How long the command of `gerrit.auth.command` may take with no
/// terminal. An OIDC refresh asks the network.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// The same in the terminal of qreview, where a person may log in.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// An authorization that expires this soon is asked again: a call must not
/// start with a token that dies on the way.
const MARGIN: Duration = Duration::from_secs(30);

/// The line that Gerrit puts before every JSON answer, against a page of
/// another site that would read it as a script.
const GUARD: &str = ")]}'";

/// The variable that tells the command which Gerrit asks.
pub const URL_VAR: &str = "QREVIEW_GERRIT_URL";

/// Set when the command runs in the terminal of qreview, where it may ask
/// the user.
pub const INTERACTIVE_VAR: &str = "QREVIEW_INTERACTIVE";

/// The command of `gerrit.auth.command`, and what it last gave.
pub struct Auth {
    command: Vec<String>,
    /// qreview runs in a terminal: the command gets it, and may ask.
    interactive: bool,
    held: Mutex<Option<Held>>,
}

struct Held {
    /// The value of the `Authorization` header.
    authorization: String,
    until: Option<Instant>,
}

impl Auth {
    pub fn new(command: Vec<String>, interactive: bool) -> Self {
        Self {
            command,
            interactive,
            held: Mutex::new(None),
        }
    }

    /// The authorization, from memory while it lasts. `again` runs the
    /// command even so: Gerrit refused the one in memory.
    async fn get(&self, base: &str, again: bool) -> Result<String> {
        let mut held = self.held.lock().await;
        if !again
            && let Some(kept) = held.as_ref()
            && kept
                .until
                .is_none_or(|until| Instant::now() + MARGIN < until)
        {
            return Ok(kept.authorization.clone());
        }
        let given = run_command(&self.command, base, self.interactive).await?;
        let authorization = given.authorization.clone();
        *held = Some(given);
        Ok(authorization)
    }
}

// A derived Debug would print the token in a panic or a trace line.
impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

/// Run the command, and read the authorization it prints. `interactive`:
/// the command gets the terminal of qreview.
async fn run_command(command: &[String], base: &str, interactive: bool) -> Result<Held> {
    let (program, args) = command
        .split_first()
        .context("gerrit.auth.command is empty")?;
    let mut child = Command::new(program);
    child
        .args(args)
        .env(URL_VAR, base)
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    let limit = match interactive {
        true => {
            child
                .env(INTERACTIVE_VAR, "1")
                .stdin(Stdio::inherit())
                .stderr(Stdio::inherit());
            LOGIN_TIMEOUT
        }
        // No terminal to ask in: the command answers from what it keeps, or
        // fails.
        false => {
            child
                .env_remove(INTERACTIVE_VAR)
                .stdin(Stdio::null())
                .stderr(Stdio::piped());
            COMMAND_TIMEOUT
        }
    };

    // Not `output()`: it pipes standard error, and the question of the
    // command would never reach the terminal.
    let _lent = interactive.then(crate::status::lend);
    let started = crate::trace::start();
    let ran = tokio::time::timeout(limit, async { child.spawn()?.wait_with_output().await }).await;
    // The program only: its output is the secret.
    crate::trace::since(started, || format!("gerrit.auth.command {program}"));
    let out = match ran {
        Ok(Ok(out)) => out,
        Ok(Err(error)) => bail!("cannot run gerrit.auth.command {program}: {error}"),
        Err(_) => bail!(
            "gerrit.auth.command {program} did not answer in {}s",
            limit.as_secs()
        ),
    };
    if !out.status.success() {
        if interactive {
            bail!("gerrit.auth.command {program} failed");
        }
        let said = String::from_utf8_lossy(&out.stderr);
        let said = said.trim().lines().next().unwrap_or_default();
        bail!("gerrit.auth.command {program} failed: {said}");
    }
    read_answer(&out.stdout).with_context(|| format!("gerrit.auth.command {program}"))
}

/// `{"authorization": "Bearer …", "expiresIn": 3600}`. The error never
/// quotes the output: it holds the secret.
fn read_answer(out: &[u8]) -> Result<Held> {
    let answer: Value = serde_json::from_slice(out)
        .map_err(|_| anyhow!("printed no JSON object with `authorization`"))?;
    let authorization = answer["authorization"]
        .as_str()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .context("printed no `authorization`")?;
    // A line break would end the header, and start another one.
    if authorization.chars().any(char::is_control) {
        bail!("printed an `authorization` with a control character");
    }
    let until = match &answer["expiresIn"] {
        Value::Null => None,
        seconds => Some(
            Instant::now()
                + Duration::from_secs(
                    seconds
                        .as_u64()
                        .context("printed an `expiresIn` that is no number of seconds")?,
                ),
        ),
    };
    Ok(Held {
        authorization: authorization.to_owned(),
        until,
    })
}

/// Why a call gave nothing.
#[derive(Debug)]
pub enum Failed {
    /// The server refused the authorization, or asks for one, or the
    /// command gave none.
    Refused(anyhow::Error),
    /// Nothing answered: no route, no curl, no time left.
    Unreachable(anyhow::Error),
    /// The server answered with an error about this call alone.
    Answer(anyhow::Error),
}

impl Failed {
    pub fn error(self) -> anyhow::Error {
        match self {
            Self::Refused(error) | Self::Unreachable(error) | Self::Answer(error) => error,
        }
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(error) | Self::Unreachable(error) | Self::Answer(error) => {
                write!(f, "{error:#}")
            }
        }
    }
}

/// A client of one Gerrit server.
#[derive(Clone, Debug)]
pub struct Rest {
    /// The address of the server, such as `https://review.example.com`.
    base: String,
    /// None to read as an anonymous reader.
    auth: Option<Arc<Auth>>,
    /// `curl`, unless a test names its own.
    curl: PathBuf,
}

impl Rest {
    pub fn new(base: &str, auth: Option<Auth>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            auth: auth.map(Arc::new),
            curl: PathBuf::from("curl"),
        }
    }

    /// The same, with another program in place of `curl`.
    pub fn with_curl(mut self, curl: &Path) -> Self {
        self.curl = curl.to_owned();
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn has_auth(&self) -> bool {
        self.auth.is_some()
    }

    /// The address of a route. `/a/` asks for the authorization; without
    /// it the server answers an anonymous reader.
    fn url(&self, route: &str) -> String {
        let route = route.trim_start_matches('/');
        match self.auth {
            Some(_) => format!("{}/a/{route}", self.base),
            None => format!("{}/{route}", self.base),
        }
    }

    /// Call a route, and read the JSON it answers.
    pub async fn call(
        &self,
        method: &str,
        route: &str,
        body: Option<&Value>,
    ) -> Result<Value, Failed> {
        // A token in memory can die before its time: one refusal runs the
        // command again, once.
        let mut again = false;
        loop {
            let authorization = match &self.auth {
                Some(auth) => Some(auth.get(&self.base, again).await.map_err(Failed::Refused)?),
                None => None,
            };
            match self
                .call_once(method, route, body, authorization.as_deref())
                .await
            {
                Err(Failed::Refused(_)) if authorization.is_some() && !again => again = true,
                answer => return answer,
            }
        }
    }

    async fn call_once(
        &self,
        method: &str,
        route: &str,
        body: Option<&Value>,
        authorization: Option<&str>,
    ) -> Result<Value, Failed> {
        let url = self.url(route);
        let config = config(method, &url, body, authorization);

        let mut command = Command::new(&self.curl);
        command
            .args(args(TIMEOUT))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let started = crate::trace::start();
        let answered = tokio::time::timeout(TIMEOUT + Duration::from_secs(1), async {
            let mut child = command.spawn().context("cannot run curl")?;
            let mut stdin = child.stdin.take().context("curl has no standard input")?;
            stdin.write_all(config.as_bytes()).await?;
            drop(stdin);
            anyhow::Ok(child.wait_with_output().await?)
        })
        .await;
        // The address only: the authorization is in the configuration.
        crate::trace::since(started, || format!("curl {method} {url}"));

        let out = match answered {
            Ok(Ok(out)) => out,
            Ok(Err(error)) => return Err(Failed::Unreachable(error)),
            Err(_) => {
                return Err(Failed::Unreachable(anyhow!(
                    "{} did not answer in {}s",
                    self.base,
                    TIMEOUT.as_secs()
                )));
            }
        };

        let text = String::from_utf8_lossy(&out.stdout);
        let text = text.trim_end();
        let (body, status) = text.rsplit_once('\n').unwrap_or(("", text));
        let status: u16 = status.trim().parse().unwrap_or(0);

        match status {
            200..=299 => parse(body).map_err(Failed::Answer),
            401 | 403 if authorization.is_some() => Err(Failed::Refused(anyhow!(
                "Gerrit at {} refused the authorization of gerrit.auth.command. \
                 Refresh to ask the command again",
                self.base
            ))),
            401 | 403 => Err(Failed::Refused(anyhow!(
                "Gerrit at {} asks who you are. Set gerrit.auth.command",
                self.base
            ))),
            0 => Err(Failed::Unreachable(anyhow!(
                "cannot reach {}: {}",
                self.base,
                String::from_utf8_lossy(&out.stderr).trim()
            ))),
            _ => Err(Failed::Answer(anyhow!(
                "Gerrit answered {status} to {method} {route}: {}",
                body.trim().lines().next().unwrap_or_default()
            ))),
        }
    }
}

/// What curl reads on its standard input. It holds the authorization, so it
/// never goes anywhere else.
fn config(method: &str, url: &str, body: Option<&Value>, authorization: Option<&str>) -> String {
    let mut out = format!(
        "url = \"{}\"\nrequest = \"{}\"\n",
        quote(url),
        quote(method)
    );
    if let Some(authorization) = authorization {
        out.push_str(&format!(
            "header = \"Authorization: {}\"\n",
            quote(authorization)
        ));
    }
    if let Some(body) = body {
        out.push_str("header = \"Content-Type: application/json; charset=UTF-8\"\n");
        out.push_str(&format!("data-binary = \"{}\"\n", quote(&body.to_string())));
    }
    out
}

/// The arguments of curl. Nothing secret goes here.
fn args(limit: Duration) -> Vec<String> {
    vec![
        "--silent".to_owned(),
        "--show-error".to_owned(),
        // A secret never crosses the network in clear, even after a redirect.
        "--proto".to_owned(),
        "=https".to_owned(),
        "--max-time".to_owned(),
        limit.as_secs().to_string(),
        "--write-out".to_owned(),
        "\\n%{http_code}".to_owned(),
        "--config".to_owned(),
        "-".to_owned(),
    ]
}

/// A string inside double quotes of a curl configuration file.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// The JSON of an answer, without the guard line before it.
fn parse(body: &str) -> Result<Value> {
    let body = body.trim_start();
    let body = body.strip_prefix(GUARD).unwrap_or(body).trim();
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(body).context("Gerrit answered something that is not JSON")
}

/// The address of the REST API, from the web address of a change.
///
/// `https://review.example.com/c/myproject/+/12321` and the older
/// `https://review.example.com/12321` both give
/// `https://review.example.com`. An address that is not `https` gives
/// nothing: the authorization would cross the network in clear.
pub fn base_of(change_url: &str) -> Option<String> {
    let url = change_url.trim().trim_end_matches('/');
    if !url.starts_with("https://") {
        return None;
    }
    let base = match url.find("/c/") {
        Some(at) => &url[..at],
        None => {
            let (base, last) = url.rsplit_once('/')?;
            if last.is_empty() || !last.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            base
        }
    };
    (base.len() > "https://".len()).then(|| base.to_owned())
}

/// The address the configuration names, checked the same way.
pub fn configured(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    if !url.starts_with("https://") || url.len() <= "https://".len() {
        bail!("gerrit.url must be an https address, not {url}");
    }
    Ok(url.to_owned())
}

/// A project name as one segment of a route: `group/myproject` is
/// `group%2Fmyproject`.
pub fn encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_comes_from_the_page_of_a_change() {
        assert_eq!(
            base_of("https://review.example.com/c/myproject/+/12321").as_deref(),
            Some("https://review.example.com")
        );
        assert_eq!(
            base_of("https://review.example.com/12321").as_deref(),
            Some("https://review.example.com")
        );
        assert_eq!(
            base_of("https://example.com/gerrit/c/group/myproject/+/7").as_deref(),
            Some("https://example.com/gerrit")
        );
    }

    #[test]
    fn an_address_in_clear_is_refused() {
        assert_eq!(base_of("http://review.example.com/c/myproject/+/1"), None);
        assert!(configured("http://review.example.com").is_err());
        assert_eq!(
            configured("https://review.example.com/").unwrap(),
            "https://review.example.com"
        );
    }

    #[test]
    fn a_project_in_a_group_is_one_segment() {
        assert_eq!(encode("group/my-project_1.x"), "group%2Fmy-project_1.x");
    }

    #[test]
    fn the_authorization_goes_in_the_configuration_and_nowhere_else() {
        let rest = Rest::new(
            "https://review.example.com",
            Some(Auth::new(vec!["x".into()], false)),
        );
        let body = serde_json::json!({ "message": "two\nlines" });
        let config = config(
            "PUT",
            &rest.url("changes/1/drafts"),
            Some(&body),
            Some("Bearer s3\"cr\\et"),
        );

        assert!(
            config.contains(r#"header = "Authorization: Bearer s3\"cr\\et""#),
            "{config}"
        );
        assert!(config.contains(r#"url = "https://review.example.com/a/changes/1/drafts""#));
        assert!(config.contains(r#"data-binary = "{\"message\":\"two\\nlines\"}""#));
        assert!(
            !args(TIMEOUT).join(" ").contains("s3"),
            "no secret in the arguments"
        );
    }

    #[test]
    fn an_anonymous_reader_asks_without_the_a() {
        let rest = Rest::new("https://review.example.com", None);
        assert_eq!(
            rest.url("changes/1/comments"),
            "https://review.example.com/changes/1/comments"
        );
        assert!(!config("GET", "x", None, None).contains("Authorization"));
    }

    #[test]
    fn the_command_prints_the_authorization_and_how_long_it_lasts() {
        let held = read_answer(br#"{"authorization":" Bearer abc ","expiresIn":60}"#).unwrap();
        assert_eq!(held.authorization, "Bearer abc");
        assert!(held.until.is_some());
        let held = read_answer(br#"{"authorization":"Basic bWU6czNjcmV0"}"#).unwrap();
        assert!(held.until.is_none());

        for wrong in [
            &br#"Bearer s3cret"#[..],
            br#"{"token":"s3cret"}"#,
            br#"{"authorization":""}"#,
            br#"{"authorization":"Bearer s3cret\r\nX-Evil: 1"}"#,
            br#"{"authorization":"Bearer s3cret","expiresIn":"soon"}"#,
        ] {
            let error = read_answer(wrong).err().unwrap();
            assert!(!format!("{error:#}").contains("s3cret"), "{error:#}");
        }
    }

    #[test]
    fn the_guard_line_is_dropped() {
        assert_eq!(parse(")]}'\n{\"a\":1}").unwrap()["a"], 1);
        assert_eq!(parse("").unwrap(), Value::Null);
    }

    /// A curl that keeps what it was given and answers `body` with `status`.
    fn fake_curl(dir: &Path, body: &str, status: u16) -> PathBuf {
        let path = dir.join(format!("curl-{status}"));
        let reply = dir.join(format!("body-{status}"));
        std::fs::write(&reply, body).unwrap();
        script(
            &path,
            &format!(
                "#!/bin/sh\necho \"$@\" > \"{args}\"\ncat > \"{stdin}\"\ncat \"{reply}\"\nprintf '\\n%s' {status}\n",
                args = dir.join("args").display(),
                stdin = dir.join("stdin").display(),
                reply = reply.display(),
            ),
        );
        path
    }

    /// Write a script that a test runs.
    ///
    /// Another process writes it. A file that this process holds open for
    /// writing when another thread forks cannot be run: `ETXTBSY`.
    fn script(path: &Path, text: &str) {
        let source = path.with_extension("txt");
        std::fs::write(&source, text).unwrap();
        let done = std::process::Command::new("install")
            .arg("-m755")
            .arg(&source)
            .arg(path)
            .status()
            .unwrap();
        assert!(done.success());
    }

    /// A command of `gerrit.auth.command` that writes each run to `runs`,
    /// and gives `token-<run>`.
    fn command(dir: &Path, expires_in: Option<u64>) -> Auth {
        let path = dir.join("auth");
        let runs = dir.join("runs");
        let expires = expires_in.map_or(String::new(), |s| format!(",\"expiresIn\":{s}"));
        script(
            &path,
            &format!(
                "#!/bin/sh\necho \"$QREVIEW_GERRIT_URL\" >> '{runs}'\nn=$(wc -l < '{runs}')\nprintf '{{\"authorization\":\"Bearer token-%s\"{expires}}}' $n\n",
                runs = runs.display(),
            ),
        );
        Auth::new(vec![path.display().to_string()], false)
    }

    fn runs(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("runs"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[tokio::test]
    async fn the_authorization_comes_on_standard_input_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let curl = fake_curl(
            dir.path(),
            r#")]}'{"username":"me","email":"me@example.com"}"#,
            200,
        );
        let rest = Rest::new(
            "https://review.example.com",
            Some(command(dir.path(), None)),
        )
        .with_curl(&curl);

        let answer = rest.call("GET", "accounts/self", None).await.unwrap();
        rest.call("GET", "accounts/self", None).await.unwrap();

        assert_eq!(answer["username"], "me");
        let given = std::fs::read_to_string(dir.path().join("stdin")).unwrap();
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(given.contains("Authorization: Bearer token-1"), "{given}");
        assert!(!args.contains("token"), "{args}");
        assert_eq!(runs(dir.path()), ["https://review.example.com"], "run once");
    }

    #[tokio::test]
    async fn an_authorization_that_expires_is_asked_again() {
        let dir = tempfile::tempdir().unwrap();
        let curl = fake_curl(dir.path(), "{}", 200);
        let rest = Rest::new(
            "https://review.example.com",
            Some(command(dir.path(), Some(1))),
        )
        .with_curl(&curl);

        rest.call("GET", "x", None).await.unwrap();
        rest.call("GET", "x", None).await.unwrap();

        assert_eq!(runs(dir.path()).len(), 2);
    }

    #[tokio::test]
    async fn a_refusal_runs_the_command_once_more_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let curl = fake_curl(dir.path(), "Unauthorized", 401);
        let rest = Rest::new(
            "https://review.example.com",
            Some(command(dir.path(), None)),
        )
        .with_curl(&curl);

        let error = rest.call("GET", "x", None).await.unwrap_err();

        assert!(matches!(error, Failed::Refused(_)), "{error}");
        assert!(error.to_string().contains("Refresh"), "{error}");
        assert_eq!(runs(dir.path()).len(), 2);
    }

    #[tokio::test]
    async fn only_a_terminal_lets_the_command_ask_the_user() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth");
        let seen = dir.path().join("seen");
        script(
            &path,
            &format!(
                "#!/bin/sh\necho \"[$QREVIEW_INTERACTIVE]\" >> '{seen}'\necho '{{\"authorization\":\"Bearer t\"}}'\n",
                seen = seen.display()
            ),
        );
        let command = vec![path.display().to_string()];

        for interactive in [true, false] {
            Auth::new(command.clone(), interactive)
                .get("https://review.example.com", false)
                .await
                .unwrap();
        }

        let seen = std::fs::read_to_string(&seen).unwrap();
        assert_eq!(seen, "[1]\n[]\n");
    }

    #[tokio::test]
    async fn the_terminal_is_the_standard_error_of_a_command_that_asks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth");
        let seen = dir.path().join("seen");
        script(
            &path,
            &format!(
                "#!/bin/sh\nreadlink /proc/self/fd/2 > '{seen}'\necho '{{\"authorization\":\"Bearer t\"}}'\n",
                seen = seen.display()
            ),
        );
        let command = vec![path.display().to_string()];
        let ours = std::fs::read_link("/proc/self/fd/2").unwrap();

        for (interactive, inherited) in [(true, true), (false, false)] {
            Auth::new(command.clone(), interactive)
                .get("https://review.example.com", false)
                .await
                .unwrap();
            let theirs = std::fs::read_to_string(&seen).unwrap();
            assert_eq!(
                theirs.trim() == ours.to_string_lossy(),
                inherited,
                "{theirs} {ours:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_command_that_fails_says_why_and_nothing_more() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth");
        script(
            &path,
            "#!/bin/sh\necho 'not logged in, run oidc-login' >&2\necho s3cret\nexit 1\n",
        );
        let rest = Rest::new(
            "https://review.example.com",
            Some(Auth::new(vec![path.display().to_string()], false)),
        )
        .with_curl(&fake_curl(dir.path(), "{}", 200));

        let error = rest.call("GET", "x", None).await.unwrap_err();

        assert!(matches!(error, Failed::Refused(_)), "{error}");
        let error = error.to_string();
        assert!(error.contains("run oidc-login"), "{error}");
        assert!(!error.contains("s3cret"), "{error}");
    }

    #[tokio::test]
    async fn a_refusal_is_told_apart_from_another_error() {
        let dir = tempfile::tempdir().unwrap();
        let refused = Rest::new("https://review.example.com", None).with_curl(&fake_curl(
            dir.path(),
            "Unauthorized",
            401,
        ));
        let error = refused
            .call("GET", "accounts/self", None)
            .await
            .unwrap_err();
        assert!(matches!(error, Failed::Refused(_)), "{error}");
        assert!(error.to_string().contains("gerrit.auth.command"), "{error}");

        let missing = Rest::new("https://review.example.com", None).with_curl(&fake_curl(
            dir.path(),
            "Not found",
            404,
        ));
        let error = missing.call("GET", "changes/9", None).await.unwrap_err();
        assert!(matches!(error, Failed::Answer(_)), "{error}");
    }

    #[tokio::test]
    async fn no_curl_is_no_gerrit() {
        let rest =
            Rest::new("https://review.example.com", None).with_curl(Path::new("/nonexistent/curl"));

        let error = rest.call("GET", "x", None).await.unwrap_err();
        assert!(matches!(error, Failed::Unreachable(_)), "{error}");
    }
}

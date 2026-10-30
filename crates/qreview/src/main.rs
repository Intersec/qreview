//! qreview — local code review for a Git series.
//!
//! The binary is a thin shell: it reads the arguments, opens the session,
//! and serves the interface. Everything else lives in the library.

mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::net::TcpListener;

use qreview::agent;
use qreview::api::{self, AppState, auth};
use qreview::config;
use qreview::git::exec::Git;
use qreview::highlight::Highlighter;
use qreview::lang::Languages;
use qreview::report::{self, ChangeFiles};
use qreview::series::Options;
use qreview::session::Session;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Before the first git call, so the whole start is timed.
    qreview::trace::enable_from_env();
    if cli.trace {
        qreview::trace::enable();
    }

    let cwd = std::env::current_dir().context("cannot read the working directory")?;
    if let Some(code) = agent_command(&cli, &cwd).await? {
        std::process::exit(code);
    }

    let root = Git::discover(&cwd).await?.root().to_path_buf();
    let home = agent::home(&cwd).await?;

    // An agent that starts qreview twice gets the review that is open, not
    // a second server and a second tab.
    if cli.command.is_none()
        && let Some(server) = agent::running(&home).await
    {
        println!(
            "qreview already runs on this worktree, at {}",
            server.address().url()
        );
        return Ok(());
    }
    let config = config::load(&root)?;

    let mut opts = Options::new();
    opts.rev = cli.rev.clone();
    opts.base = cli.base.clone();
    opts.prevs = cli.prev.clone();
    opts.max_commits = config.series.max_commits;
    opts.guess_max = config.series.guess_max;
    opts.batch_size = config.series.batch_size;
    opts.integration_branch = config
        .series
        .integration_branch
        .clone()
        .or_else(|| config.gerrit.branch.clone());
    // The option only ever switches Gerrit off, never on.
    opts.gerrit = config.gerrit.enabled && !cli.no_gerrit;
    opts.worktree = config.series.worktree && !cli.no_worktree;

    let mut langs = Languages::new();
    langs.extend(&config.languages);

    let highlighter = match config::grammar_dir().filter(|dir| dir.is_dir()) {
        Some(dir) => Highlighter::with_grammars(&dir),
        None => Highlighter::new(),
    };

    let session = Session::with(&cwd, &opts, langs, std::sync::Arc::new(highlighter), None).await?;

    match cli.command {
        Some(cli::Command::Export { key, json: true }) => {
            let value = qreview::export::json(&session, key.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(());
        }
        Some(cli::Command::Export { key, json: false }) => {
            let text = match key {
                Some(key) => qreview::export::change(&session, &key).await?,
                None => qreview::export::series(&session).await?,
            };
            print!("{text}");
            return Ok(());
        }
        Some(cli::Command::List) => {
            print!("{}", list(&session));
            return Ok(());
        }
        Some(_) => unreachable!("the commands of the agent ran above"),
        None => {}
    }

    // The series and its files cost a diff per change, and the reader who
    // opens the browser never reads them. `--verbose` still prints them.
    if cli.verbose {
        print!("{}", text_report(&session).await?);
        println!();
    }

    let token = auth::new_token();
    let state = AppState::new(session, token.clone()).with_config(config.clone(), root.clone());

    // While the reader reads the change the page opens on, the server reads
    // the file list of the others. See `api::read_ahead`.
    api::read_ahead(state.clone());

    serve(cli.port, state, cli.no_open, &home).await
}

/// Run a command of the agent. None when the command is another one.
async fn agent_command(cli: &cli::Cli, cwd: &std::path::Path) -> Result<Option<i32>> {
    let print = |value: &serde_json::Value| {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    };

    match &cli.command {
        Some(cli::Command::Comment { place, body, key }) => {
            let client = agent::connect(cwd).await?;
            let place = agent::parse_place(place)?;
            let body = agent::body_of(body)?;
            print(&agent::comment(&client, &place, &body, key.as_deref()).await?);
        }
        Some(cli::Command::Reply {
            id,
            body,
            done,
            blocked,
        }) => {
            let client = agent::connect(cwd).await?;
            let body = agent::body_of(body)?;
            print(&agent::reply(&client, id, &body, *done, *blocked).await?);
        }
        Some(cli::Command::Wait { after, timeout }) => {
            let home = agent::home(cwd).await?;
            let timeout = std::time::Duration::from_secs(*timeout);
            let (events, any) = agent::wait(&home, *after, timeout).await?;
            print(&events);
            return Ok(Some(if any { 0 } else { 1 }));
        }
        Some(cli::Command::Refresh) => {
            agent::refresh(&agent::connect(cwd).await?).await?;
        }
        Some(cli::Command::Migrate {
            from,
            to,
            done,
            blocked,
        }) => {
            let client = agent::connect(cwd).await?;
            print(&agent::migrate(&client, from, to.as_deref(), done, blocked).await?);
        }
        Some(cli::Command::Skill) => print!("{}", agent::SKILL),
        _ => return Ok(None),
    }
    Ok(Some(0))
}

/// The reviews this repository has stored, whether the change is in the
/// series being read or not.
fn list(session: &Session) -> String {
    let keys = session.store.keys();
    if keys.is_empty() {
        return "No review is stored for this repository.\n".to_owned();
    }

    let mut out = String::new();
    for key in keys {
        let Ok(file) = session.comments(&key, "") else {
            out.push_str(&format!("{key}  (unreadable)\n"));
            continue;
        };
        out.push_str(&format!(
            "{key}  {} comment{}  {}\n",
            file.comments.len(),
            if file.comments.len() == 1 { "" } else { "s" },
            file.subject
        ));
    }
    out
}

/// The series as text, for `--verbose`.
///
/// It costs one diff per change, so nothing calls it unless the reader asks.
async fn text_report(session: &Session) -> Result<String> {
    let mut files = Vec::new();

    for change in &session.series.changes {
        files.push(ChangeFiles {
            key: change.key.clone(),
            files: session
                .files(
                    &change.commit,
                    &qreview::session::Against::Parent,
                    &qreview::diff::How::default(),
                )
                .await?,
        });
    }
    Ok(report::render(&session.series, &files))
}

async fn serve(port: u16, state: AppState, no_open: bool, home: &agent::Home) -> Result<()> {
    let events = state.events.clone();
    let session = state.session.clone();
    let token = state.token.clone();
    let token = token.as_str();
    let app = api::app(state);

    // The loopback address only. Never another interface.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .context("cannot listen on the loopback address")?;
    let addr = listener.local_addr()?;

    let url = format!("http://{addr}/?t={token}");
    println!("qreview is at {url}");
    println!("Press Ctrl-C to stop.");

    // The way in for an agent in a sandbox. The review works without it,
    // so a socket that cannot be made is only said.
    let unix = match unix_listener(home) {
        Ok(found) => Some(found),
        Err(error) => {
            eprintln!("qreview: an agent in a sandbox will not reach this server: {error:#}");
            None
        }
    };
    let on_socket = unix.as_ref().map(|(_, path)| path.clone());
    let socket_task = unix.map(|(listener, _)| {
        let app = app.clone();
        tokio::spawn(async move { axum::serve(listener, app).await })
    });

    // The commands of an agent find the server here. A review still works
    // without them, so a file that cannot be written is only said.
    let announced = match agent::announce(home, addr.port(), token, on_socket.as_deref()) {
        Ok(address) => Some(address),
        Err(error) => {
            eprintln!("qreview: the commands of an agent will not find this server: {error:#}");
            None
        }
    };

    if !no_open {
        open_browser(&url);
    }
    let page_task = tokio::spawn(watch_parties(events.clone(), session, url.clone()));

    // On Ctrl-C the waits on the events answer at once, so the requests in
    // flight end. A request that still hangs gets a few seconds, no more:
    // the reader asked to stop.
    let (stopping, mut stopped) = tokio::sync::watch::channel(false);
    let signal = async move {
        let _ = tokio::signal::ctrl_c().await;
        events.close();
        let _ = stopping.send(true);
    };
    let deadline = async move {
        let _ = stopped.wait_for(|stop| *stop).await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    };
    let served = tokio::select! {
        served = axum::serve(listener, app).with_graceful_shutdown(signal) => {
            served.context("the server stopped with an error")
        }
        () = deadline => Ok(()),
    };

    if let Some(address) = announced {
        agent::address::remove(home, &address);
    }
    page_task.abort();
    qreview::status::clear();
    if let Some(task) = socket_task {
        task.abort();
    }
    if let Some(path) = on_socket {
        agent::socket::clear(&path);
    }
    served
}

/// Say in the terminal when the page of the review opens, closes, and
/// comes back, and when the agent connects and disconnects. The server goes
/// on without either one. On a terminal, a status line under the messages
/// says how things stand now.
async fn watch_parties(
    events: std::sync::Arc<qreview::events::Events>,
    session: std::sync::Arc<tokio::sync::RwLock<qreview::session::Session>>,
    url: String,
) {
    let mut terminal = qreview::status::Terminal::new();
    let mut page = None;
    let mut agent = false;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tick.tick().await;
        let now = events.page_open();
        match (page, now) {
            (Some(true), Some(false)) => {
                terminal.say(&format!(
                    "The review page closed. To open it again, go to {url}"
                ));
                terminal.say("Press Ctrl-C to stop.");
            }
            (None, Some(true)) => terminal.say("The review page is open."),
            (Some(false), Some(true)) => terminal.say("The review page is open again."),
            _ => {}
        }
        page = now;

        let listens = events.agent_listens();
        if listens && !agent {
            terminal.say("The agent is connected.");
        } else if agent && !listens {
            terminal
                .say("The agent disconnected. To attach it again, ask it to run `qreview wait`.");
        }
        agent = listens;

        let state = qreview::status::Snapshot {
            page,
            agent: agent_state(&events, &session),
        };
        terminal.draw(&state);
    }
}

/// The agent as the status line names it. A change is named by its
/// subject; while a refresh holds the session, by its key.
fn agent_state(
    events: &qreview::events::Events,
    session: &tokio::sync::RwLock<qreview::session::Session>,
) -> qreview::status::Agent {
    use qreview::events::TaskKind;
    use qreview::status::Agent;

    if let Some(task) = events.agent_task() {
        if task.kind == TaskKind::Fix && task.key.is_empty() {
            return Agent::FixingSeries;
        }
        let subject = session
            .try_read()
            .ok()
            .and_then(|session| {
                session
                    .series
                    .changes
                    .iter()
                    .find(|change| change.key == task.key)
                    .map(|change| change.subject.clone())
            })
            .unwrap_or(task.key);
        return match task.kind {
            TaskKind::Fix => Agent::Fixing(subject),
            TaskKind::Answer => Agent::Answering(subject),
        };
    }
    if events.agent_listens() {
        Agent::Connected
    } else if events.agent_left() {
        Agent::Left
    } else {
        Agent::Absent
    }
}

/// A Unix socket for the server, in the private directory of the user.
fn unix_listener(home: &agent::Home) -> Result<(tokio::net::UnixListener, std::path::PathBuf)> {
    let path = agent::socket::path_for(home)?;
    // A server that crashed left its socket behind, and a bind on it fails.
    // A server that runs has been asked already, before this one started.
    agent::socket::clear(&path);
    let listener = tokio::net::UnixListener::bind(&path)
        .with_context(|| format!("cannot listen on {}", path.display()))?;
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;

    Ok((listener, path))
}

/// Show the review in the browser of the user.
///
/// Linux is the only target, so `xdg-open` is what a desktop provides. A
/// failure is not worth stopping for: the URL is printed above it.
fn open_browser(url: &str) {
    let started = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    if started.is_err() {
        eprintln!("qreview: no browser opened. Open the address above by hand.");
    }
}

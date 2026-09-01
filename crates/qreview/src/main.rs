//! qreview — local code review for a Git series.
//!
//! The binary is a thin shell: it reads the arguments, opens the session,
//! and serves the interface. Everything else lives in the library.

mod cli;

use anyhow::{Context, Result};
use axum::Router;
use clap::Parser;
use tokio::net::TcpListener;

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

    let cwd = std::env::current_dir().context("cannot read the working directory")?;
    let root = Git::discover(&cwd).await?.root().to_path_buf();
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
        Some(cli::Command::Export { key }) => {
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

    serve(cli.port, api::app(state), &token, cli.no_open).await
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

async fn serve(port: u16, app: Router, token: &str, no_open: bool) -> Result<()> {
    // The loopback address only. Never another interface.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .context("cannot listen on the loopback address")?;
    let addr = listener.local_addr()?;

    let url = format!("http://{addr}/?t={token}");
    println!("qreview is at {url}");
    println!("Press Ctrl-C to stop.");

    if !no_open {
        open_browser(&url);
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("the server stopped with an error")
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

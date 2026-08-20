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
use qreview::lang::Languages;
use qreview::report::{self, ChangeFiles};
use qreview::series::Options;
use qreview::session::Session;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let cwd = std::env::current_dir().context("cannot read the working directory")?;
    let mut opts = Options::new();
    opts.rev = cli.rev.clone();
    opts.base = cli.base.clone();

    let session = Session::open(&cwd, &opts, Languages::new()).await?;
    print!("{}", text_report(&session).await?);

    let token = auth::new_token();
    let state = AppState::new(session, token.clone());

    serve(cli.port, api::app(state), &token, cli.no_open).await
}

/// The series as text. The interface arrives in M2, see `roadmap/plan.md`.
async fn text_report(session: &Session) -> Result<String> {
    let mut files = Vec::new();

    for change in &session.series.changes {
        files.push(ChangeFiles {
            key: change.key.clone(),
            files: session.files(&change.commit).await?,
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
    println!();
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

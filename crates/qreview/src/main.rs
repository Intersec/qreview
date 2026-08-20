//! qreview — local code review for a Git series.
//!
//! The binary is a thin shell: it reads the arguments, opens the session,
//! and serves the interface. Everything else lives in the library.

mod cli;

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use clap::Parser;
use tokio::net::TcpListener;

use qreview::api::{self, AppState, auth};
use qreview::assets;
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

    serve(cli.port, api::router(state), &token).await
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

async fn serve(port: u16, app: Router, token: &str) -> Result<()> {
    let app = app.fallback(assets_route);

    // The loopback address only. Never another interface.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .context("cannot listen on the loopback address")?;
    let addr = listener.local_addr()?;

    println!();
    println!("qreview is at http://{addr}/?t={token}");
    println!("Press Ctrl-C to stop.");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("the server stopped with an error")
}

async fn assets_route(uri: Uri) -> Response {
    match assets::get(uri.path()) {
        Some((body, mime)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime)],
            Body::from(body),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "the interface is not built").into_response(),
    }
}

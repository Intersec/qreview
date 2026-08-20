//! qreview — local code review for a Git series.
//!
//! This is the scaffold. It serves the interface and nothing else. The git
//! model, the API, and the comment store arrive in M1 and M2. See
//! `roadmap/plan.md`.

mod assets;
mod cli;

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use clap::Parser;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let app = Router::new().fallback(serve);

    // The loopback address only. Never another interface.
    let listener = TcpListener::bind(("127.0.0.1", cli.port))
        .await
        .context("cannot listen on the loopback address")?;
    let addr = listener.local_addr()?;

    println!("qreview is at http://{addr}/");
    println!("Press Ctrl-C to stop.");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("the server stopped with an error")
}

async fn serve(uri: Uri) -> Response {
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

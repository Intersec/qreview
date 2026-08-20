//! Who may talk to the server.
//!
//! The server binds the loopback address, which stops another machine. It
//! does not stop another user on this one, or a page in another tab. A token
//! does.

use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::AppState;

/// The cookie the first page load leaves behind.
pub const COOKIE: &str = "qreview_token";

#[derive(Deserialize)]
pub struct TokenQuery {
    t: Option<String>,
}

/// A random token, made once per run.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system has no randomness");

    hex::encode(bytes)
}

/// Let the request through when it carries the token, as a query parameter
/// or as the cookie the first load set.
pub async fn guard(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    request: Request,
    next: Next,
) -> Response {
    let from_query = query.t.as_deref() == Some(state.token.as_str());
    let from_cookie = cookie(&request) == Some(state.token.as_str());

    if !from_query && !from_cookie {
        return (
            StatusCode::UNAUTHORIZED,
            "this page needs the token qreview printed",
        )
            .into_response();
    }

    let mut response = next.run(request).await;

    // Hand the cookie over once, so the address bar stays clean afterwards.
    if from_query && !from_cookie {
        let value = format!(
            "{COOKIE}={}; Path=/; SameSite=Strict; HttpOnly",
            state.token
        );
        if let Ok(value) = value.parse() {
            response.headers_mut().insert(header::SET_COOKIE, value);
        }
    }
    response
}

fn cookie(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value)
}

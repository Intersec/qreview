//! The HTTP routes.
//!
//! Every answer is JSON, and every error carries the same shape, so the
//! interface never has to guess what came back.

pub mod auth;

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::git::merge::Base;
use crate::model::{ChangeSummary, FileDiff, FileEntry, Series};
use crate::session::Session;

#[derive(Clone)]
pub struct AppState {
    pub session: Arc<RwLock<Session>>,
    pub token: Arc<String>,
}

impl AppState {
    pub fn new(session: Session, token: String) -> Self {
        Self {
            session: Arc::new(RwLock::new(session)),
            token: Arc::new(token),
        }
    }
}

/// The whole application: the routes, the interface, and the token guard.
///
/// The interface is inside the guard, not beside it. The first page load is
/// what carries the token and takes the cookie back, so a page served
/// outside the guard would never get one, and every API call would fail.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/session", get(session))
        .route("/api/series/extend", post(extend))
        .route("/api/changes/{key}", get(change))
        .route("/api/changes/{key}/files", get(files))
        .route("/api/changes/{key}/diff", get(diff))
        .route("/api/changes/{key}/mergelist", get(mergelist))
        .fallback(interface)
        .layer(middleware::from_fn_with_state(state.clone(), auth::guard))
        .with_state(state)
}

/// The interface, out of the binary.
async fn interface(uri: axum::http::Uri) -> Response {
    match crate::assets::get(uri.path()) {
        Some((body, mime)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, mime)],
            axum::body::Body::from(body),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "the interface is not built").into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionBody {
    version: &'static str,
    series: Series,
}

async fn session(State(state): State<AppState>) -> Json<SessionBody> {
    let session = state.session.read().await;

    Json(SessionBody {
        version: env!("CARGO_PKG_VERSION"),
        series: session.series.clone(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtendBody {
    /// How many commits to load. The batch size when it is missing.
    count: Option<usize>,
}

async fn extend(
    State(state): State<AppState>,
    body: Option<Json<ExtendBody>>,
) -> Result<Json<Series>, ApiError> {
    let count = body.and_then(|b| b.count).unwrap_or(5);
    let mut session = state.session.write().await;

    session.extend(count).await?;

    Ok(Json(session.series.clone()))
}

async fn change(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ChangeSummary>, ApiError> {
    let session = state.session.read().await;

    session
        .series
        .changes
        .iter()
        .find(|c| c.key == key)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))
}

/// Which side a merge is read against. Absent means the default: the first
/// parent for a change, the auto-merge for a merge.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BaseQuery {
    base: Option<String>,
}

impl BaseQuery {
    fn choice(&self) -> Result<Option<Base>, ApiError> {
        match self.base.as_deref() {
            None | Some("") | Some("default") => Ok(None),
            Some("automerge") => Ok(Some(Base::AutoMerge)),
            Some("parent1") => Ok(Some(Base::Parent(1))),
            Some("parent2") => Ok(Some(Base::Parent(2))),
            Some(other) => Err(ApiError::bad_request(format!(
                "base {other} is not one of automerge, parent1, parent2"
            ))),
        }
    }
}

async fn files(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(base): Query<BaseQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let session = state.session.read().await;
    let commit = session
        .commit_of(&key)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))?;

    Ok(Json(session.files(&commit, base.choice()?).await?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeListItem {
    commit: String,
    subject: String,
    author: String,
    date: String,
}

async fn mergelist(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Vec<MergeListItem>>, ApiError> {
    let session = state.session.read().await;
    let commit = session
        .commit_of(&key)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))?;

    let list = session
        .merge_list(&commit)
        .await?
        .into_iter()
        .map(|c| MergeListItem {
            commit: c.hash,
            subject: c.subject,
            author: c.author,
            date: c.date,
        })
        .collect();

    Ok(Json(list))
}

#[derive(Deserialize)]
struct DiffQuery {
    file: String,
    base: Option<String>,
}

async fn diff(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<FileDiff>, ApiError> {
    let session = state.session.read().await;
    let commit = session
        .commit_of(&key)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))?;

    let base = BaseQuery {
        base: query.base.clone(),
    }
    .choice()?;

    session
        .diff(&commit, &query.file, base)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("{} is not in the change", query.file)))
}

/// One error shape for every route.
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "notFound",
            message,
        }
    }

    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "badRequest",
            message,
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "failed",
            message: error.to_string(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorFields<'a>,
}

#[derive(Serialize)]
struct ErrorFields<'a> {
    code: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorFields {
                code: self.code,
                message: &self.message,
            },
        };

        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use crate::lang::Languages;
    use crate::series::Options;
    use crate::testutil::{Repo, build_repo, commit};

    const TOKEN: &str = "0123456789abcdef";

    async fn server(repo: &Repo) -> Router {
        let session = Session::open(repo.path(), &Options::new(), Languages::new())
            .await
            .unwrap();

        app(AppState::new(session, TOKEN.to_owned()))
    }

    async fn fixture() -> Repo {
        build_repo(&[
            commit("first: start").file("src/a.blk", "one\ntwo\n"),
            commit("second: go on")
                .file("src/a.blk", "one\nTWO\n")
                .change_id("I8f3ac21"),
        ])
        .await
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn get_with_cookie(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header(header::COOKIE, format!("{}={token}", auth::COOKIE))
            .body(Body::empty())
            .unwrap()
    }

    async fn json(app: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

        (status, value)
    }

    #[tokio::test]
    async fn a_request_without_the_token_is_refused() {
        let repo = fixture().await;
        let response = server(&repo)
            .await
            .oneshot(get("/api/session"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_token_is_refused() {
        let repo = fixture().await;
        let response = server(&repo)
            .await
            .oneshot(get("/api/session?t=wrong"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn the_token_in_the_query_hands_over_a_cookie() {
        let repo = fixture().await;
        let response = server(&repo)
            .await
            .oneshot(get(&format!("/api/session?t={TOKEN}")))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("the first load must set the cookie")
            .to_str()
            .unwrap();
        assert!(cookie.contains(TOKEN), "{cookie}");
        assert!(cookie.contains("HttpOnly"), "{cookie}");
    }

    #[tokio::test]
    async fn the_cookie_alone_is_enough_afterwards() {
        let repo = fixture().await;
        let response = server(&repo)
            .await
            .oneshot(get_with_cookie("/api/session", TOKEN))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get(header::SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn the_session_route_answers_with_the_series() {
        let repo = fixture().await;
        let (status, body) =
            json(server(&repo).await, get_with_cookie("/api/session", TOKEN)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["series"]["changes"][0]["subject"], "second: go on");
        assert_eq!(body["series"]["changes"][0]["key"], "I8f3ac21");
        assert_eq!(body["series"]["changes"][0]["isMerge"], false);
        assert!(body["series"]["boundary"]["kind"].is_string());
    }

    #[tokio::test]
    async fn the_file_list_carries_the_counts_and_the_language() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/files", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["path"], "src/a.blk");
        assert_eq!(body[0]["language"], "c");
        assert_eq!(body[0]["added"], 1);
        assert_eq!(body[0]["removed"], 1);
    }

    #[tokio::test]
    async fn the_diff_route_answers_with_the_rows() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/diff?file=src/a.blk", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["path"], "src/a.blk",
            "the file fields are flattened in"
        );
        let rows = &body["hunks"][0]["rows"];
        assert_eq!(rows[0]["kind"], "context");
        assert_eq!(rows[1]["kind"], "remove");
        assert_eq!(rows[2]["text"], "TWO");
    }

    #[tokio::test]
    async fn the_rows_arrive_already_highlighted() {
        let repo = build_repo(&[
            commit("first").file("src/net.blk", "int a = 1;\n/* note */\n"),
            commit("second")
                .file("src/net.blk", "int a = 2;\n/* note */\n")
                .change_id("Icolors"),
        ])
        .await;

        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/Icolors/diff?file=src/net.blk", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let rows = body["hunks"][0]["rows"].as_array().unwrap();
        let added = rows.iter().find(|r| r["kind"] == "add").unwrap();
        let classes: Vec<_> = added["tokens"]
            .as_array()
            .expect("an added row carries its spans")
            .iter()
            .map(|t| t["cls"].as_str().unwrap().to_owned())
            .collect();

        // .blk is C by the map alone. No grammar knows the extension.
        assert!(
            classes
                .iter()
                .any(|c| c.starts_with("storage") || c.starts_with("keyword")),
            "{classes:?}"
        );
        assert!(
            classes.iter().any(|c| c.starts_with("constant")),
            "the number is a constant: {classes:?}"
        );

        let context = rows.iter().find(|r| r["kind"] == "context").unwrap();
        let comment: Vec<_> = context["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["cls"].as_str().unwrap().to_owned())
            .collect();
        assert!(
            comment.iter().any(|c| c.starts_with("comment")),
            "{comment:?}"
        );
    }

    #[tokio::test]
    async fn a_file_the_change_does_not_touch_is_a_named_error() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/diff?file=nope.txt", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "notFound");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("nope.txt")
        );
    }

    #[tokio::test]
    async fn an_unknown_change_is_a_named_error() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/Inope/files", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "notFound");
    }

    async fn merged() -> Repo {
        build_repo(&[
            commit("base").file("f", "a\nb\nc\n"),
            commit("side work")
                .on_branch("side")
                .file("f", "a\nB2\nc\n")
                .file("only-side.txt", "side\n"),
            commit("main work")
                .on_branch("main")
                .file("f", "a\nB1\nc\n"),
            crate::testutil::merge("Merge side into main")
                .from("side")
                .file("f", "a\nRESOLVED\nc\n"),
        ])
        .await
    }

    #[tokio::test]
    async fn the_merge_under_the_boundary_can_be_opened() {
        let repo = merged().await;
        let server = server(&repo).await;
        let (_, session) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;

        let merge_commit = session["series"]["boundary"]["commit"].as_str().unwrap();
        assert_eq!(session["series"]["boundary"]["kind"], "merge");

        let (status, files) = json(
            server,
            get_with_cookie(&format!("/api/changes/{merge_commit}/files"), TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let paths: Vec<_> = files
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["f"], "the auto-merge shows the resolution alone");
    }

    #[tokio::test]
    async fn the_base_selector_changes_what_the_merge_shows() {
        let repo = merged().await;
        let server = server(&repo).await;
        let (_, session) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        let m = session["series"]["boundary"]["commit"]
            .as_str()
            .unwrap()
            .to_owned();

        let (_, first) = json(
            server.clone(),
            get_with_cookie(&format!("/api/changes/{m}/files?base=parent1"), TOKEN),
        )
        .await;
        let mut paths: Vec<_> = first
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        paths.sort_unstable();
        assert_eq!(paths, ["f", "only-side.txt"], "the whole branch");

        let (status, body) = json(
            server,
            get_with_cookie(&format!("/api/changes/{m}/files?base=nonsense"), TOKEN),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "badRequest");
    }

    #[tokio::test]
    async fn the_merge_list_names_the_commits_it_brings_in() {
        let repo = merged().await;
        let server = server(&repo).await;
        let (_, session) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        let m = session["series"]["boundary"]["commit"]
            .as_str()
            .unwrap()
            .to_owned();

        let (status, body) = json(
            server,
            get_with_cookie(&format!("/api/changes/{m}/mergelist"), TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["subject"], "side work");
    }

    #[tokio::test]
    async fn extend_appends_and_never_reorders() {
        let commits: Vec<_> = (1..=9)
            .map(|i| commit(&format!("change {i}")).file("a", &format!("{i}\n")))
            .collect();
        let repo = build_repo(&commits).await;

        let mut opts = Options::new();
        opts.guess_max = 3;
        let session = Session::open(repo.path(), &opts, Languages::new())
            .await
            .unwrap();
        let server = app(AppState::new(session, TOKEN.to_owned()));

        let before = json(server.clone(), get_with_cookie("/api/session", TOKEN))
            .await
            .1;
        assert_eq!(before["series"]["changes"].as_array().unwrap().len(), 3);

        let request = Request::builder()
            .method("POST")
            .uri("/api/series/extend")
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"count":2}"#))
            .unwrap();
        let (status, after) = json(server, request).await;

        assert_eq!(status, StatusCode::OK);
        let changes = after["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 5);
        assert_eq!(changes[0]["subject"], "change 9", "the head does not move");
        assert_eq!(changes[4]["subject"], "change 5");
    }
}

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

use crate::anchor::{self, Placed};
use crate::comments::{EditComment, NewComment};
use crate::git::merge::Base;
use crate::model::{ChangeSummary, FileDiff, FileEntry, Series};
use crate::patchset::PatchSet;
use crate::session::Against;
use crate::session::Session;
use crate::store::model::{ChangeFile, Comment};

#[derive(Clone)]
pub struct AppState {
    pub session: Arc<RwLock<Session>>,
    pub token: Arc<String>,
    /// What the configuration asks the interface to start with.
    pub ui: Arc<crate::config::Ui>,
}

impl AppState {
    pub fn new(session: Session, token: String) -> Self {
        Self {
            session: Arc::new(RwLock::new(session)),
            token: Arc::new(token),
            ui: Arc::new(crate::config::Config::default().ui),
        }
    }

    pub fn with_ui(mut self, ui: crate::config::Ui) -> Self {
        self.ui = Arc::new(ui);
        self
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
        .route("/api/changes/{key}", get(change).patch(mark_change))
        .route("/api/changes/{key}/files", get(files))
        .route("/api/changes/{key}/diff", get(diff))
        .route("/api/changes/{key}/mergelist", get(mergelist))
        .route("/api/export", get(export))
        .route("/api/changes/{key}/lines", get(lines))
        .route("/api/changes/{key}/patchsets", get(patchsets))
        .route(
            "/api/changes/{key}/patchsets/{number}/fetch",
            post(fetch_patch_set),
        )
        .route(
            "/api/changes/{key}/comments",
            get(comments).post(add_comment),
        )
        .route(
            "/api/changes/{key}/comments/{id}",
            axum::routing::patch(edit_comment).delete(delete_comment),
        )
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
    ui: crate::config::Ui,
}

async fn session(State(state): State<AppState>) -> Json<SessionBody> {
    let session = state.session.read().await;

    Json(SessionBody {
        version: env!("CARGO_PKG_VERSION"),
        series: session.series.clone(),
        ui: (*state.ui).clone(),
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
struct ViewQuery {
    /// Which patch set to read. The last one when it is missing.
    ps: Option<usize>,
    /// What to read it against.
    base: Option<String>,
    /// `ignore` leaves out the lines that differ only by whitespace.
    #[serde(default)]
    ws: Option<String>,
}

/// `ws=ignore` asks git to leave whitespace out of the comparison.
fn ignore_ws(value: Option<&str>) -> bool {
    value == Some("ignore")
}

/// A base is a word, or `ps:<n>` to read one patch set against another.
fn parse_base(value: Option<&str>) -> Result<Option<Against>, ApiError> {
    match value {
        None | Some("") | Some("default") | Some("parent") => Ok(None),
        Some("automerge") => Ok(Some(Against::Merge(Base::AutoMerge))),
        Some("parent1") => Ok(Some(Against::Merge(Base::Parent(1)))),
        Some("parent2") => Ok(Some(Against::Merge(Base::Parent(2)))),
        Some(other) => match other
            .strip_prefix("ps:")
            .and_then(|n| n.parse::<usize>().ok())
        {
            // The number is turned into a commit by the caller, which is the
            // only place that knows the patch sets of this change.
            Some(number) => Ok(Some(Against::Tree(format!("ps:{number}")))),
            None => Err(ApiError::bad_request(format!(
                "base {other} is not one of parent, automerge, parent1, parent2, ps:<n>"
            ))),
        },
    }
}

/// Turn a `ps:<n>` placeholder into the commit of that patch set.
async fn resolve_base(
    session: &Session,
    key: &str,
    against: Option<Against>,
) -> Result<Against, ApiError> {
    let Some(against) = against else {
        return Ok(Against::Parent);
    };

    if let Against::Tree(name) = &against
        && let Some(number) = name
            .strip_prefix("ps:")
            .and_then(|n| n.parse::<usize>().ok())
    {
        let commit = session
            .target_of(key, Some(number))
            .await
            .map_err(|e| ApiError::not_found(e.to_string()))?;
        return Ok(Against::Tree(commit));
    }
    Ok(against)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkBody {
    reviewed: bool,
}

/// Mark a change read, or unread.
async fn mark_change(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<MarkBody>,
) -> Result<Json<ChangeSummary>, ApiError> {
    let mut session = state.session.write().await;
    session.mark_reviewed(&key, body.reviewed)?;

    session
        .series
        .changes
        .iter()
        .find(|c| c.key == key)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))
}

async fn files(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(view): Query<ViewQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let session = state.session.read().await;
    let commit = target(&session, &key, view.ps).await?;
    let against = resolve_base(&session, &key, parse_base(view.base.as_deref())?).await?;

    Ok(Json(
        session
            .files(&commit, &against, ignore_ws(view.ws.as_deref()))
            .await?,
    ))
}

/// The commit to read: a patch set when one is named, the change otherwise.
async fn target(session: &Session, key: &str, ps: Option<usize>) -> Result<String, ApiError> {
    if ps.is_some() {
        return session
            .target_of(key, ps)
            .await
            .map_err(|e| ApiError::not_found(e.to_string()));
    }
    session
        .commit_of(key)
        .ok_or_else(|| ApiError::not_found(format!("no change {key} in the series")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinesQuery {
    file: String,
    from: usize,
    to: usize,
    ps: Option<usize>,
}

/// The lines between two hunks, so the reader can open the context.
async fn lines(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<LinesQuery>,
) -> Result<Json<Vec<crate::model::Row>>, ApiError> {
    if query.to < query.from || query.to - query.from > 2000 {
        return Err(ApiError::bad_request(
            "ask for a run of 2000 lines or fewer".to_owned(),
        ));
    }

    let session = state.session.read().await;
    let commit = target(&session, &key, query.ps).await?;

    Ok(Json(
        session
            .lines(&commit, &query.file, query.from, query.to)
            .await?,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchSets {
    sets: Vec<PatchSet>,
    /// What Gerrit calls this change, when the server knows it.
    gerrit: Option<GerritChange>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GerritChange {
    number: u64,
    url: String,
    branch: String,
    status: String,
}

async fn patchsets(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<PatchSets>, ApiError> {
    let session = state.session.read().await;
    let sets = session.patch_sets(&key).await?;
    let gerrit = session
        .gerrit_change(&key)
        .await
        .map(|change| GerritChange {
            number: change.number,
            url: change.url,
            branch: change.branch,
            status: change.status,
        });

    Ok(Json(PatchSets { sets, gerrit }))
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
    ps: Option<usize>,
    base: Option<String>,
    ws: Option<String>,
}

async fn diff(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<FileDiff>, ApiError> {
    let session = state.session.read().await;
    let commit = target(&session, &key, query.ps).await?;
    let against = resolve_base(&session, &key, parse_base(query.base.as_deref())?).await?;

    session
        .diff(
            &commit,
            &query.file,
            &against,
            ignore_ws(query.ws.as_deref()),
        )
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("{} is not in the change", query.file)))
}

/// The review of a change, and where each comment lands in the patch set
/// being read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Review {
    #[serde(flatten)]
    file: ChangeFile,
    /// One entry per comment. A comment whose place is gone is marked lost
    /// and is never dropped.
    placed: Vec<Placed>,
}

async fn fetch_patch_set(
    State(state): State<AppState>,
    Path((key, number)): Path<(String, usize)>,
) -> Result<Json<PatchSet>, ApiError> {
    let session = state.session.read().await;

    Ok(Json(session.fetch_patch_set(&key, number).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportQuery {
    /// `change` or `series`. The whole series by default.
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

async fn export(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Result<String, ApiError> {
    let session = state.session.read().await;
    let opts = crate::export::Options {};

    match (query.scope.as_deref(), query.key) {
        (Some("change"), Some(key)) => Ok(crate::export::change(&session, &key, opts).await?),
        (Some("change"), None) => Err(ApiError::bad_request(
            "a change export needs the key of the change".to_owned(),
        )),
        _ => Ok(crate::export::series(&session, opts).await?),
    }
}

async fn comments(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(view): Query<ViewQuery>,
) -> Result<Json<Review>, ApiError> {
    let session = state.session.read().await;
    let subject = session
        .series
        .changes
        .iter()
        .find(|c| c.key == key)
        .map(|c| c.subject.clone())
        .unwrap_or_default();

    let file = session.comments(&key, &subject)?;
    let rev = target(&session, &key, view.ps).await?;
    let placed = anchor::place_all(&session.git, &file.comments, &rev).await;

    Ok(Json(Review { file, placed }))
}

async fn add_comment(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(new): Json<NewComment>,
) -> Result<(StatusCode, Json<Comment>), ApiError> {
    let session = state.session.read().await;
    let comment = session.add_comment(&key, new).await?;

    Ok((StatusCode::CREATED, Json(comment)))
}

async fn edit_comment(
    State(state): State<AppState>,
    Path((key, id)): Path<(String, String)>,
    Json(edit): Json<EditComment>,
) -> Result<Json<Comment>, ApiError> {
    let session = state.session.read().await;

    Ok(Json(session.edit_comment(&key, &id, edit)?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Deleted {
    deleted: usize,
}

async fn delete_comment(
    State(state): State<AppState>,
    Path((key, id)): Path<(String, String)>,
) -> Result<Json<Deleted>, ApiError> {
    let session = state.session.read().await;
    let deleted = session.delete_comment(&key, &id)?;

    Ok(Json(Deleted { deleted }))
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

    /// A server whose comment store is a temporary directory. A test must
    /// never write into the state directory of the person running it.
    async fn server(repo: &Repo) -> Router {
        let session = session_of(repo, Options::new()).await;

        app(AppState::new(session, TOKEN.to_owned()))
    }

    async fn session_of(repo: &Repo, opts: Options) -> Session {
        let store = crate::store::Store::at(repo.path().join(".qreview-test").as_path());
        // No test talks to a server. A fixture with an ssh remote would
        // otherwise wait on a real Gerrit that is not there.
        let opts = Options {
            gerrit: false,
            ..opts
        };

        Session::with(
            repo.path(),
            &opts,
            Languages::new(),
            std::sync::Arc::new(crate::highlight::Highlighter::new()),
            Some(store),
        )
        .await
        .unwrap()
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

    fn post(uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    fn send(method: &str, uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    const LINE_COMMENT: &str = r#"{"scope":"line","file":"src/a.blk","side":"new","startLine":2,"body":"this reads wrong"}"#;

    #[tokio::test]
    async fn a_line_comment_records_where_it_sits() {
        let repo = fixture().await;
        let server = server(&repo).await;

        let (status, body) = json(
            server.clone(),
            post("/api/changes/I8f3ac21/comments", LINE_COMMENT),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["body"], "this reads wrong");
        assert_eq!(body["anchor"]["file"], "src/a.blk");
        assert_eq!(body["anchor"]["startLine"], 2);
        assert!(
            body["anchor"]["blob"].is_string(),
            "the blob is what the anchor is read from: {body}"
        );
        assert!(
            body["anchor"]["lineHash"]
                .as_str()
                .unwrap()
                .starts_with("sha256:"),
            "{body}"
        );
        assert!(
            body["anchor"]["context"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("TWO")),
            "the context must hold the line itself: {body}"
        );

        let (_, back) = json(
            server,
            get_with_cookie("/api/changes/I8f3ac21/comments", TOKEN),
        )
        .await;
        assert_eq!(back["comments"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_comment_with_no_text_is_refused() {
        let repo = fixture().await;
        let (status, _) = json(
            server(&repo).await,
            post(
                "/api/changes/I8f3ac21/comments",
                r#"{"scope":"change","body":"   "}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_comment_is_deleted_on_its_own() {
        let repo = fixture().await;
        let server = server(&repo).await;

        let (_, first) = json(
            server.clone(),
            post("/api/changes/I8f3ac21/comments", LINE_COMMENT),
        )
        .await;
        let id = first["id"].as_str().unwrap().to_owned();

        let (status, body) = json(
            server.clone(),
            send(
                "DELETE",
                &format!("/api/changes/I8f3ac21/comments/{id}"),
                "",
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["deleted"], 1);

        let (_, left) = json(
            server,
            get_with_cookie("/api/changes/I8f3ac21/comments", TOKEN),
        )
        .await;
        assert!(left["comments"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_series_carries_the_comment_counts() {
        let repo = fixture().await;
        let server = server(&repo).await;

        json(
            server.clone(),
            post("/api/changes/I8f3ac21/comments", LINE_COMMENT),
        )
        .await;

        // A new session reads the store again, the way a restart would.
        let session = session_of(&repo, Options::new()).await;
        let fresh = app(AppState::new(session, TOKEN.to_owned()));
        let (_, body) = json(fresh, get_with_cookie("/api/session", TOKEN)).await;

        let change = &body["series"]["changes"][0];
        assert_eq!(change["key"], "I8f3ac21");
        assert_eq!(change["commentCount"], 1);
    }

    #[tokio::test]
    async fn an_amend_keeps_the_comments() {
        let repo = fixture().await;
        let server = server(&repo).await;

        json(server, post("/api/changes/I8f3ac21/comments", LINE_COMMENT)).await;
        let before = repo.sha("HEAD").await;

        std::fs::write(repo.path().join("src/a.blk"), "one\nTHREE\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;
        assert_ne!(
            repo.sha("HEAD").await,
            before,
            "the amend makes a new commit"
        );

        let session = session_of(&repo, Options::new()).await;
        let after = app(AppState::new(session, TOKEN.to_owned()));
        let (status, body) = json(
            after,
            get_with_cookie("/api/changes/I8f3ac21/comments", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["comments"][0]["body"], "this reads wrong",
            "the Change-Id is the key, so the amend keeps the review"
        );
    }

    #[tokio::test]
    async fn a_prev_commit_becomes_an_older_patch_set() {
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("work: do a thing")
                .file("a.txt", "one\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        let mut opts = Options::new();
        opts.prevs = vec![first.clone()];
        let server = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));

        let (status, sets) = json(
            server.clone(),
            get_with_cookie("/api/changes/Iwork/patchsets", TOKEN),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(sets["gerrit"].is_null(), "no server was asked");
        let sets = sets["sets"].as_array().unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0]["number"], 1);
        assert_eq!(sets[0]["origin"], "prev");
        assert_eq!(sets[0]["available"], true);
        assert_eq!(sets[1]["origin"], "local");

        // The series pane says how many versions the change has.
        let (_, session) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(session["series"]["changes"][0]["patchSetCount"], 2);

        // Patch set 1 against its own parent shows the first version.
        let (_, one) = json(
            server.clone(),
            get_with_cookie("/api/changes/Iwork/diff?file=a.txt&ps=1", TOKEN),
        )
        .await;
        let added: Vec<_> = one["hunks"][0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "add")
            .map(|r| r["text"].as_str().unwrap())
            .collect();
        assert_eq!(added, ["one"]);

        // Patch set 2 read against patch set 1 shows only what the amend did.
        let (status, between) = json(
            server,
            get_with_cookie("/api/changes/Iwork/diff?file=a.txt&ps=2&base=ps:1", TOKEN),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rows = between["hunks"][0]["rows"].as_array().unwrap();
        let texts: Vec<_> = rows
            .iter()
            .map(|r| (r["kind"].as_str().unwrap(), r["text"].as_str().unwrap()))
            .collect();
        assert_eq!(texts, [("remove", "one"), ("add", "two")]);
    }

    #[tokio::test]
    async fn a_comment_follows_its_line_into_the_next_patch_set() {
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("work")
                .file("a.txt", "alpha\nbeta\ngamma\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        let mut opts = Options::new();
        opts.prevs = vec![first.clone()];
        let server = app(AppState::new(
            session_of(&repo, opts.clone()).await,
            TOKEN.to_owned(),
        ));

        // A comment on "beta", which is line 2 in the first version.
        json(
            server,
            post(
                "/api/changes/Iwork/comments",
                r#"{"scope":"line","file":"a.txt","side":"new","startLine":2,"body":"why beta"}"#,
            ),
        )
        .await;

        // The amend pushes two lines above it, so beta becomes line 4.
        std::fs::write(
            repo.path().join("a.txt"),
            "new\nlines\nalpha\nbeta\ngamma\n",
        )
        .unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        let fresh = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));
        let (status, body) = json(
            fresh,
            get_with_cookie("/api/changes/Iwork/comments?ps=2", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let placed = &body["placed"][0];
        assert_eq!(placed["line"], 4, "the comment follows the line it was on");
        assert_eq!(placed["moved"], true);
        assert_eq!(placed["lost"], false);
    }

    #[tokio::test]
    async fn a_comment_whose_line_is_gone_is_kept_and_marked() {
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("work")
                .file("a.txt", "alpha\nbeta\ngamma\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;
        let mut opts = Options::new();
        opts.prevs = vec![first];
        let server = app(AppState::new(
            session_of(&repo, opts.clone()).await,
            TOKEN.to_owned(),
        ));

        json(
            server,
            post(
                "/api/changes/Iwork/comments",
                r#"{"scope":"line","file":"a.txt","side":"new","startLine":2,"body":"why beta"}"#,
            ),
        )
        .await;

        std::fs::write(repo.path().join("a.txt"), "alpha\ngamma\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        let fresh = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));
        let (_, body) = json(
            fresh,
            get_with_cookie("/api/changes/Iwork/comments?ps=2", TOKEN),
        )
        .await;

        assert_eq!(body["placed"][0]["lost"], true);
        assert_eq!(
            body["comments"][0]["body"], "why beta",
            "a comment is never dropped, only marked"
        );
    }

    #[tokio::test]
    async fn the_lines_route_reads_the_context_the_diff_left_out() {
        let long: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        let repo = build_repo(&[
            commit("base").file("a.txt", &long),
            commit("touch one line")
                .file("a.txt", &long.replace("line 20\n", "LINE TWENTY\n"))
                .change_id("Icontext"),
        ])
        .await;
        let server = server(&repo).await;

        // The diff carries a few lines around line 20 and nothing else.
        let (_, diff) = json(
            server.clone(),
            get_with_cookie("/api/changes/Icontext/diff?file=a.txt", TOKEN),
        )
        .await;
        assert_eq!(diff["lineCount"], 30, "the interface needs the length");
        let first = diff["hunks"][0]["rows"][0]["newLine"].as_u64().unwrap();
        assert!(first > 5, "the file starts well above the hunk");

        let (status, rows) = json(
            server.clone(),
            get_with_cookie("/api/changes/Icontext/lines?file=a.txt&from=1&to=4", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["newLine"], 1);
        assert_eq!(rows[0]["text"], "line 1");
        assert_eq!(rows[3]["text"], "line 4");
        assert_eq!(rows[0]["kind"], "context");

        // Past the end of the file is a short answer, not a failure.
        let (_, tail) = json(
            server.clone(),
            get_with_cookie(
                "/api/changes/Icontext/lines?file=a.txt&from=28&to=99",
                TOKEN,
            ),
        )
        .await;
        assert_eq!(tail.as_array().unwrap().len(), 3);

        let (status, _) = json(
            server,
            get_with_cookie("/api/changes/Icontext/lines?file=a.txt&from=9&to=1", TOKEN),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_patch_set_that_does_not_exist_is_a_named_error() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/files?ps=7", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("patch set 7"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn the_series_export_holds_a_comment_written_a_moment_ago() {
        let repo = fixture().await;
        let server = server(&repo).await;

        json(
            server.clone(),
            post("/api/changes/I8f3ac21/comments", LINE_COMMENT),
        )
        .await;

        let response = server
            .oneshot(get_with_cookie("/api/export", TOKEN))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes);

        // The count on the series is filled when the session opens. Trusting
        // it here made the export say the series had nothing in it.
        assert!(text.contains("this reads wrong"), "{text}");
        assert!(text.contains("src/a.blk:2"), "{text}");
    }

    #[tokio::test]
    async fn load_more_walks_past_the_base_it_stopped_on() {
        let commits: Vec<_> = (1..=8)
            .map(|i| commit(&format!("change {i}")).file("a", &format!("{i}\n")))
            .collect();
        let repo = build_repo(&commits).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~2").await;

        let server = app(AppState::new(
            session_of(&repo, Options::new()).await,
            TOKEN.to_owned(),
        ));

        let (_, first) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(first["series"]["changes"].as_array().unwrap().len(), 2);
        assert_eq!(first["series"]["boundary"]["kind"], "base");

        let request = Request::builder()
            .method("POST")
            .uri("/api/series/extend")
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"count":3}"#))
            .unwrap();
        let (status, after) = json(server, request).await;

        assert_eq!(status, StatusCode::OK);
        let changes = after["changes"].as_array().unwrap();
        assert_eq!(
            changes.len(),
            5,
            "the base is where the first batch stopped, not a wall"
        );
        assert_eq!(changes[2]["subject"], "change 6");
    }

    #[tokio::test]
    async fn load_more_walks_past_a_merge_and_keeps_it() {
        let repo = merged().await;
        let repo = {
            // One commit above the merge, so the first batch is not empty.
            std::fs::write(repo.path().join("after.txt"), "1\n").unwrap();
            repo.git(&["add", "-A"]).await;
            repo.git(&["commit", "-m", "after the merge"]).await;
            repo
        };
        let server = app(AppState::new(
            session_of(&repo, Options::new()).await,
            TOKEN.to_owned(),
        ));

        let (_, first) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(first["series"]["changes"].as_array().unwrap().len(), 1);
        assert_eq!(first["series"]["boundary"]["kind"], "merge");

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
        assert_eq!(changes[1]["subject"], "Merge side into main");
        assert_eq!(changes[1]["isMerge"], true, "the merge joins the list");
        assert_eq!(
            changes[2]["subject"], "main work",
            "and the walk goes on down the first parent"
        );
    }

    #[tokio::test]
    async fn extend_appends_and_never_reorders() {
        let commits: Vec<_> = (1..=9)
            .map(|i| commit(&format!("change {i}")).file("a", &format!("{i}\n")))
            .collect();
        let repo = build_repo(&commits).await;

        let mut opts = Options::new();
        opts.guess_max = 3;
        let server = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));

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

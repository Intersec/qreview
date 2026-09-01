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
    /// The three layers, folded. The panel writes it and every route reads
    /// it, so a setting takes effect on the next request.
    pub config: Arc<std::sync::RwLock<crate::config::Config>>,
    /// Where the repository layer lives, so a write can read it back.
    pub root: Arc<std::path::PathBuf>,
    /// Whether a newer qreview is out. Asked once, by whoever asks first.
    release: Arc<tokio::sync::OnceCell<crate::update::Release>>,
    /// What the server has in hand. The task that reads ahead waits for a
    /// quiet moment, so it never competes with the reader.
    busy: Arc<std::sync::Mutex<Busy>>,
}

/// What the server is doing, as the read-ahead task needs to know it.
#[derive(Debug)]
struct Busy {
    /// Requests being answered right now.
    in_hand: usize,
    /// When the last one ended.
    ended: std::time::Instant,
}

impl AppState {
    pub fn new(session: Session, token: String) -> Self {
        Self {
            session: Arc::new(RwLock::new(session)),
            token: Arc::new(token),
            config: Arc::new(std::sync::RwLock::new(crate::config::Config::default())),
            root: Arc::new(std::path::PathBuf::from(".")),
            release: Arc::new(tokio::sync::OnceCell::new()),
            busy: Arc::new(std::sync::Mutex::new(Busy {
                in_hand: 0,
                ended: std::time::Instant::now(),
            })),
        }
    }

    /// The configuration this run started with, and where it came from.
    pub fn with_config(mut self, config: crate::config::Config, root: std::path::PathBuf) -> Self {
        self.config = Arc::new(std::sync::RwLock::new(config));
        self.root = Arc::new(root);
        self
    }

    /// How long the server has had nothing in hand.
    ///
    /// Zero while a request is being answered, however long it runs. A
    /// timestamp alone would not do: the middle of a request that takes a
    /// second and a half reads as a quiet second.
    fn quiet_for(&self) -> std::time::Duration {
        let busy = self.busy.lock().unwrap();

        match busy.in_hand {
            0 => busy.ended.elapsed(),
            _ => std::time::Duration::ZERO,
        }
    }

    fn config(&self) -> crate::config::Config {
        self.config
            .read()
            .expect("the configuration lock is poisoned")
            .clone()
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
        .route("/api/session/refresh", post(refresh))
        .route("/api/series/extend", post(extend))
        .route("/api/changes/{key}", get(change).patch(mark_change))
        .route("/api/changes/{key}/files", get(files))
        .route("/api/changes/{key}/diff", get(diff))
        .route("/api/changes/{key}/mergelist", get(mergelist))
        .route("/api/comments", get(all_comments))
        .route("/api/update", get(update))
        .route("/api/export", get(export))
        .route("/api/config", get(config).put(save_config))
        .route("/api/changes/{key}/lines", get(lines))
        .route("/api/changes/{key}/patchsets", get(patchsets))
        .route("/api/changes/{key}/posted", get(posted))
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
        .layer(middleware::from_fn_with_state(state.clone(), timed))
        .with_state(state)
}

/// Time the whole request, so a trace shows what the browser waited for.
///
/// It sits outside the token guard: a request that fails the guard is worth
/// a line too.
async fn timed(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let started = crate::trace::start();
    let what = started.map(|_| format!("{} {}", request.method(), without_token(request.uri())));

    let _in_hand = InHand::new(state);
    let response = next.run(request).await;

    crate::trace::since(started, || {
        format!(
            "{} -> {}",
            what.unwrap_or_default(),
            response.status().as_u16()
        )
    });
    response
}

/// Held while one request is being answered.
///
/// A guard rather than a pair of calls: the browser can hang up, and then
/// the answer is dropped half made. A count that never came back down would
/// leave the server looking busy for the rest of the run.
struct InHand(AppState);

impl InHand {
    fn new(state: AppState) -> Self {
        state.busy.lock().unwrap().in_hand += 1;

        Self(state)
    }
}

impl Drop for InHand {
    fn drop(&mut self) {
        let mut busy = self.0.busy.lock().unwrap();
        busy.in_hand = busy.in_hand.saturating_sub(1);
        busy.ended = std::time::Instant::now();
    }
}

/// The address a trace may print. The token never belongs in a log.
fn without_token(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    let Some(query) = uri.query() else {
        return path.to_owned();
    };

    let kept: Vec<&str> = query.split('&').filter(|p| !p.starts_with("t=")).collect();
    match kept.is_empty() {
        true => path.to_owned(),
        false => format!("{path}?{}", kept.join("&")),
    }
}

/// How long the server must have been quiet before the task reads on.
const QUIET: std::time::Duration = std::time::Duration::from_millis(750);

/// Read the file list of every change of the series, before it is asked for.
///
/// A change of many files costs about a second of rename and copy detection,
/// and the reader pays it on the click that opens the change. A reader opens
/// one change and reads it for a while, so the rest is read in that time.
///
/// **Only while nobody is waiting.** The task holds before every change until
/// the server has answered nothing for `QUIET`. Read ahead of a reader who is
/// still clicking, it would take the git and the processor that the click
/// needs, and make the change they asked for slower to buy one they may never
/// open.
///
/// The newest change is left out: the page opens on it, and that request is
/// already in flight.
///
/// The session is held only while the base of a change is resolved. The long
/// call runs outside it, so a request the reader made is never behind this
/// one.
pub fn read_ahead(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let how = how(&state, None);
        let (git, lists, keys) = {
            let session = state.session.read().await;
            (
                session.git.clone(),
                session.lists.clone(),
                session
                    .series
                    .changes
                    .iter()
                    .skip(1)
                    .map(|change| change.key.clone())
                    .collect::<Vec<_>>(),
            )
        };

        for key in keys {
            quiet(&state).await;

            let pair = {
                let session = state.session.read().await;

                match session.commit_of(&key) {
                    Some(rev) => session
                        .base_of(&rev, &Against::Parent)
                        .await
                        .ok()
                        .map(|base| (base, rev)),
                    None => None,
                }
            };

            let Some((base, rev)) = pair else {
                continue;
            };
            if let Err(error) = lists.of(&git, &base, &rev, &how).await {
                // Nobody asked for this yet. The read that does will say so.
                crate::trace::note(|| format!("read ahead of {rev} failed: {error}"));
            }
        }
    })
}

/// Hold until the server has answered nothing for `QUIET`.
async fn quiet(state: &AppState) {
    while let Some(left) = QUIET
        .checked_sub(state.quiet_for())
        .filter(|l| !l.is_zero())
    {
        tokio::time::sleep(left).await;
    }
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
    /// The release and the commit under it, for the tooltip.
    build: &'static str,
    series: Series,
    config: crate::config::Config,
}

async fn session(State(state): State<AppState>) -> Json<SessionBody> {
    // The working tree moves while the page is open. The reload is where the
    // series catches up with it.
    let mut session = state.session.write().await;
    session.refresh_worktree().await;

    Json(body(&session, &state))
}

/// Read the repository again, and answer with the series it holds now.
///
/// `GET /api/session` catches up with the working tree alone, because a page
/// load must not walk the history a second time. Here the reader asked for
/// it, so the whole series is resolved again.
async fn refresh(State(state): State<AppState>) -> Result<Json<SessionBody>, ApiError> {
    let mut session = state.session.write().await;
    session.refresh().await?;
    let answer = body(&session, &state);
    drop(session);

    // An amend gives a change a new commit, and with it a file list nothing
    // has read yet.
    read_ahead(state.clone());

    Ok(Json(answer))
}

fn body(session: &Session, state: &AppState) -> SessionBody {
    SessionBody {
        version: crate::version::VERSION,
        build: crate::version::LONG,
        series: session.series.clone(),
        config: state.config(),
    }
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
    let series = session.series.clone();
    drop(session);

    // The commits that just joined are the ones the reader is about to open.
    read_ahead(state.clone());

    Ok(Json(series))
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

/// How to read the diff, from the query and from the configuration.
fn how(state: &AppState, ws: Option<&str>) -> crate::diff::How {
    let config = state.config();

    crate::diff::How {
        context: config.diff.context,
        syntax: config.diff.syntax,
        // The query wins for one request; the panel decides the rest.
        ignore_ws: match ws {
            Some("ignore") => true,
            Some("keep") => false,
            _ => config.diff.ignore_whitespace,
        },
    }
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
            .review_files(&commit, &against, &how(&state, view.ws.as_deref()))
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
            &how(&state, query.ws.as_deref()),
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

    match (query.scope.as_deref(), query.key) {
        (Some("change"), Some(key)) => Ok(crate::export::change(&session, &key).await?),
        (Some("change"), None) => Err(ApiError::bad_request(
            "a change export needs the key of the change".to_owned(),
        )),
        _ => Ok(crate::export::series(&session).await?),
    }
}

async fn config(State(state): State<AppState>) -> Json<crate::config::Config> {
    Json(state.config())
}

/// Write what the panel changed, and answer with the three layers folded.
///
/// The repository layer is read again afterwards, so the answer says what
/// the tool will really use, not what was asked for.
async fn save_config(
    State(state): State<AppState>,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<crate::config::Config>, ApiError> {
    let fresh = crate::config::update(&state.root, &patch)?;
    *state
        .config
        .write()
        .expect("the configuration lock is poisoned") = fresh.clone();

    Ok(Json(fresh))
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
    // A comment on a removed line is anchored on the base, so the placement
    // needs both trees, not only the one being read.
    let base = session.base_of(&rev, &Against::Parent).await?;
    let placed = anchor::place_all(&session.git, &file.comments, &rev, &base).await;

    Ok(Json(Review { file, placed }))
}

/// What the interface reads for the remarks already on the server.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Posted {
    comments: Vec<crate::model::PostedComment>,
    /// Where each of them lands in the patch set being read.
    placed: Vec<Placed>,
}

/// The remarks already posted on Gerrit, placed in the patch set being read.
///
/// Read only. A change the server does not know answers with an empty list,
/// never an error: the local review must go on without Gerrit.
async fn posted(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(view): Query<ViewQuery>,
) -> Result<Json<Posted>, ApiError> {
    let session = state.session.read().await;
    let found = session.posted_comments(&key).await;

    if found.is_empty() {
        return Ok(Json(Posted {
            comments: Vec::new(),
            placed: Vec::new(),
        }));
    }

    let rev = target(&session, &key, view.ps).await?;
    let base = session.base_of(&rev, &Against::Parent).await?;
    let placeable: Vec<_> = found.iter().map(|p| p.placeable.clone()).collect();
    let placed = anchor::place_all(&session.git, &placeable, &rev, &base).await;

    Ok(Json(Posted {
        comments: found.into_iter().map(|p| p.wire).collect(),
        placed,
    }))
}

/// Is a newer qreview out?
///
/// The interface asks after it has painted, so nothing waits on this. The
/// answer is kept for the life of the run: a reader who reloads the page
/// does not send the question again.
async fn update(State(state): State<AppState>) -> Json<crate::update::Release> {
    let where_to = state.config.read().map(|c| c.update.clone()).ok();
    let Some(where_to) = where_to else {
        return Json(crate::update::Release::default());
    };

    let found = state
        .release
        .get_or_init(|| crate::update::check(&where_to, crate::version::VERSION))
        .await;

    Json(found.clone())
}

/// Every comment of the session, in the order a review reads them.
///
/// One answer for the counts on the buttons, on the changes and on the
/// files, and for the pane that lists them. Counting them four ways from
/// four places is how four numbers end up disagreeing.
async fn all_comments(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::model::ChangeComments>>, ApiError> {
    let session = state.session.read().await;

    Ok(Json(session.all_comments().await))
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

    /// The reader clicks a change and waits for its file list. The task
    /// reads the rest of the series while they read the first one, so the
    /// next click has nothing to wait for.
    #[tokio::test]
    async fn every_change_but_the_first_is_read_before_it_is_asked_for() {
        let repo = build_repo(&[
            commit("first: start").file("src/a.c", "int a;\n"),
            commit("second: go on").file("src/a.c", "int b;\n"),
            commit("third: and on").file("src/a.c", "int c;\n"),
        ])
        .await;
        let state = AppState::new(session_of(&repo, Options::new()).await, TOKEN.to_owned());

        assert_eq!(state.session.read().await.lists.count(), 0);
        // Nothing has asked the server anything, so the task reads at once
        // after its first hold. See `QUIET`.
        read_ahead(state.clone()).await.unwrap();

        // The page opens on the newest change, and asks for that one itself.
        let session = state.session.read().await;
        assert_eq!(session.series.changes.len(), 3);
        assert_eq!(session.lists.count(), 2);
    }

    /// The reader is what the server is for. The task that reads ahead of
    /// them must never take the git a click is waiting on.
    #[tokio::test]
    async fn the_task_holds_while_the_server_is_answering() {
        let repo = fixture().await;
        let state = AppState::new(session_of(&repo, Options::new()).await, TOKEN.to_owned());

        let answering = InHand::new(state.clone());
        let reading = read_ahead(state.clone());

        // A request in hand is not a quiet server, however long it runs.
        tokio::time::sleep(QUIET * 2).await;
        assert_eq!(state.session.read().await.lists.count(), 0);

        drop(answering);
        reading.await.unwrap();
        assert!(state.session.read().await.lists.count() > 0);
    }

    /// A trace goes into a log or a bug report. The token must not.
    #[test]
    fn a_traced_address_carries_no_token() {
        let uri = |text: &str| text.parse::<axum::http::Uri>().unwrap();

        assert_eq!(without_token(&uri("/?t=secret")), "/");
        assert_eq!(
            without_token(&uri("/api/changes/I1/diff?file=a.c&t=secret")),
            "/api/changes/I1/diff?file=a.c"
        );
        assert_eq!(without_token(&uri("/api/session")), "/api/session");
    }

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

    /// The paths of a file list, without the commit message.
    ///
    /// Every change carries one, and a test about the files of a tree does
    /// not care about it.
    fn tree_paths(files: &serde_json::Value) -> Vec<&str> {
        files
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .filter(|path| *path != crate::commitmsg::PATH)
            .collect()
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
        // The tooltip on the version needs the commit under it. A build
        // outside a checkout has none, so only the release is promised.
        assert_eq!(body["version"], crate::version::VERSION);
        assert!(
            body["build"]
                .as_str()
                .unwrap()
                .starts_with(crate::version::VERSION)
        );
    }

    /// A repository whose working tree has a tracked change that is not
    /// committed, and a file nobody has added.
    async fn dirty() -> Repo {
        let repo = fixture().await;
        std::fs::write(repo.path().join("src/a.blk"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(repo.path().join("untracked.txt"), "not added\n").unwrap();

        repo
    }

    #[tokio::test]
    async fn the_work_that_is_not_committed_stands_at_the_top_of_the_series() {
        let repo = dirty().await;
        let (status, body) =
            json(server(&repo).await, get_with_cookie("/api/session", TOKEN)).await;

        assert_eq!(status, StatusCode::OK);
        let first = &body["series"]["changes"][0];
        assert_eq!(first["key"], "working-tree");
        assert_eq!(first["subject"], "Uncommitted changes");
        assert_eq!(first["worktree"], true);
        // The commits are still there, under it.
        assert_eq!(body["series"]["changes"][1]["key"], "I8f3ac21");
    }

    #[tokio::test]
    async fn the_diff_of_the_working_tree_is_what_is_not_committed() {
        let repo = dirty().await;
        let app = server(&repo).await;
        let (status, files) = json(
            app,
            get_with_cookie("/api/changes/working-tree/files", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        // No `/COMMIT_MSG`: the message on the synthetic commit is a label
        // this tool wrote, not a message anybody reviews.
        assert_eq!(tree_paths(&files), ["src/a.blk"]);
        assert_eq!(
            files.as_array().unwrap().len(),
            1,
            "the working tree has no commit message to review"
        );

        let (_, diff) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/working-tree/diff?file=src/a.blk", TOKEN),
        )
        .await;
        let rows = &diff["hunks"][0]["rows"];
        let texts: Vec<&str> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["text"].as_str().unwrap())
            .collect();

        assert!(texts.contains(&"TWO"), "the change on the disk is shown");
        assert!(!texts.contains(&"not added"), "an untracked file is not");
    }

    #[tokio::test]
    async fn a_clean_working_tree_adds_nothing() {
        let repo = fixture().await;
        let (_, body) = json(server(&repo).await, get_with_cookie("/api/session", TOKEN)).await;

        assert_eq!(body["series"]["changes"][0]["key"], "I8f3ac21");
    }

    #[tokio::test]
    async fn no_worktree_leaves_the_series_to_the_commits() {
        let repo = dirty().await;
        let opts = Options {
            worktree: false,
            ..Options::new()
        };
        let app = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));
        let (_, body) = json(app, get_with_cookie("/api/session", TOKEN)).await;

        assert_eq!(body["series"]["changes"][0]["key"], "I8f3ac21");
    }

    #[tokio::test]
    async fn a_series_read_from_another_revision_ignores_the_tree() {
        // The tree sits on HEAD. A series read from HEAD~1 has nothing to do
        // with it, and diffing the two would be a diff of unrelated things.
        let repo = dirty().await;
        let opts = Options {
            rev: Some("HEAD~1".to_owned()),
            ..Options::new()
        };
        let app = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));
        let (_, body) = json(app, get_with_cookie("/api/session", TOKEN)).await;

        let keys: Vec<&str> = body["series"]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["key"].as_str().unwrap())
            .collect();
        assert!(!keys.contains(&"working-tree"), "{keys:?}");
    }

    #[tokio::test]
    async fn a_remark_on_the_working_tree_is_kept_under_one_key() {
        let repo = dirty().await;
        let app = server(&repo).await;

        let (status, _) = json(
            app,
            post(
                "/api/changes/working-tree/comments",
                r#"{"scope":"line","file":"src/a.blk","side":"new","startLine":2,"body":"why shout"}"#,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        // The tree moves, so the synthetic commit does. The remark does not:
        // it is filed under the key, and anchored on the line.
        std::fs::write(repo.path().join("src/a.blk"), "zero\none\nTWO\nthree\n").unwrap();

        let (_, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/working-tree/comments", TOKEN),
        )
        .await;

        assert_eq!(body["comments"][0]["body"], "why shout");
        assert_eq!(body["placed"][0]["line"], 3, "the line moved down by one");
        assert_eq!(body["placed"][0]["lost"], false);
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
        assert_eq!(body[0]["path"], "/COMMIT_MSG", "the message comes first");
        assert_eq!(body[1]["path"], "src/a.blk");
        assert_eq!(body[1]["language"], "c");
        assert_eq!(body[1]["added"], 1);
        assert_eq!(body[1]["removed"], 1);
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
                .any(|c| c.starts_with("tok-storage") || c.starts_with("tok-keyword")),
            "{classes:?}"
        );
        assert!(
            classes.iter().any(|c| c.starts_with("tok-constant")),
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
            comment.iter().any(|c| c.starts_with("tok-comment")),
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
        assert_eq!(
            tree_paths(&files),
            ["f"],
            "the auto-merge shows the resolution alone"
        );
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
        let mut paths = tree_paths(&first);
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
    async fn a_comment_on_a_removed_line_follows_the_base_of_the_patch_set() {
        // The remark speaks of a line the change deletes, so it lives on the
        // base. A rebase moves that line, and the remark must follow it
        // there, not stay on the number it was written on.
        let repo = build_repo(&[
            commit("base one").file("a.txt", "alpha\nbeta\ngamma\n"),
            commit("work: drop beta")
                .file("a.txt", "alpha\ngamma\n")
                .change_id("Iremove"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        let server = app(AppState::new(
            session_of(&repo, Options::new()).await,
            TOKEN.to_owned(),
        ));
        let (status, comment) = json(
            server,
            post(
                "/api/changes/Iremove/comments",
                r#"{"scope":"line","file":"a.txt","side":"old","startLine":2,"body":"beta mattered"}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(comment["anchor"]["side"], "old");

        // The same change on a base that put two lines above beta.
        repo.git(&["switch", "--quiet", "--detach", "HEAD~1"]).await;
        std::fs::write(
            repo.path().join("a.txt"),
            "extra\nlines\nalpha\nbeta\ngamma\n",
        )
        .unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "-m", "base two: two lines above"])
            .await;
        repo.git(&["cherry-pick", &first]).await;

        let mut opts = Options::new();
        opts.prevs = vec![first];
        let fresh = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));
        let (status, body) = json(
            fresh,
            get_with_cookie("/api/changes/Iremove/comments?ps=2", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let placed = &body["placed"][0];
        assert_eq!(placed["line"], 4, "beta sits two lines lower on this base");
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
    async fn a_second_round_finds_the_version_that_was_reviewed() {
        // Round one: read the change, write two remarks on it.
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("work")
                .file("a.txt", "alpha\nbeta\ngamma\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        for line in [2, 3] {
            json(
                server(&repo).await,
                post(
                    "/api/changes/Iwork/comments",
                    &format!(
                        r#"{{"scope":"line","file":"a.txt","side":"new","startLine":{line},"body":"about {line}"}}"#
                    ),
                ),
            )
            .await;
        }

        // An agent answers the first remark and leaves the second alone.
        std::fs::write(repo.path().join("a.txt"), "alpha\nBETA IS FIXED\ngamma\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        // Round two, with no `--prev`: the store remembers the version.
        let (status, sets) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/Iwork/patchsets", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let list = sets["sets"].as_array().unwrap();
        assert_eq!(list.len(), 2, "the version that was reviewed is offered");
        assert_eq!(list[0]["commit"], first);
        assert_eq!(list[0]["origin"], "prev");
        assert_eq!(list[1]["commit"], repo.sha("HEAD").await);

        let (_, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/Iwork/comments", TOKEN),
        )
        .await;
        let placed = body["placed"].as_array().unwrap();
        let about2 = &placed[0];
        let about3 = &placed[1];

        assert_eq!(about2["lost"], true, "the line it spoke of is gone");
        assert_eq!(about3["lost"], false, "that line was not touched");
        assert_eq!(about3["line"], 3);

        // Both were written on the version before this one, which is what
        // makes them previous remarks: they are not counted and not
        // exported. See `design.md` 5.4.
        assert_eq!(body["comments"][0]["commit"], first);
        assert_eq!(body["comments"][1]["commit"], first);
    }

    #[tokio::test]
    async fn a_change_with_no_change_id_finds_its_earlier_version() {
        // No Change-Id, so the key follows the sha and an amend files the
        // next round under a new name. The reflog still has the old commit.
        let repo = build_repo(&[
            commit("base").file("base.txt", "0\n"),
            commit("work: no Change-Id here").file("a.txt", "one\ntwo\n"),
        ])
        .await;
        let before = repo.sha("HEAD").await;

        json(
            server(&repo).await,
            post(
                &format!("/api/changes/sha-{before}/comments"),
                r#"{"scope":"line","file":"a.txt","side":"new","startLine":2,"body":"why two"}"#,
            ),
        )
        .await;

        std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;
        let after = repo.sha("HEAD").await;

        // The version before the amend is a patch set, with nobody naming it.
        let (status, sets) = json(
            server(&repo).await,
            get_with_cookie(&format!("/api/changes/sha-{after}/patchsets"), TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let list = sets["sets"].as_array().unwrap();
        assert_eq!(list.len(), 2, "the reflog holds the version before it");
        assert_eq!(list[0]["commit"], before);

        // And the remark written on it is read as a remark of this change,
        // under the version it belongs to. The store moved nothing.
        let (_, pane) = json(server(&repo).await, get_with_cookie("/api/comments", TOKEN)).await;
        let change = &pane[0];

        assert_eq!(change["key"], format!("sha-{after}"));
        assert_eq!(change["comments"][0]["body"], "why two");
        assert_eq!(change["comments"][0]["commit"], before);
        assert_eq!(change["versions"][0]["commit"], before);

        // It belongs to a round that is over, so it is counted nowhere.
        let (_, body) = json(server(&repo).await, get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(body["series"]["changes"][0]["commentCount"], 0);
    }

    #[tokio::test]
    async fn a_remark_written_on_this_version_names_it() {
        let repo = fixture().await;
        let app = server(&repo).await;

        json(app, post("/api/changes/I8f3ac21/comments", LINE_COMMENT)).await;

        let (_, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/comments", TOKEN),
        )
        .await;

        assert_eq!(body["placed"][0]["lost"], false);
        assert_eq!(
            body["comments"][0]["commit"],
            repo.sha("HEAD").await,
            "a remark says which version it was written on"
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
    async fn two_changes_never_answer_to_one_key() {
        // The same trailer twice, which a cherry-pick or a bad rebase makes.
        let repo = build_repo(&[
            commit("first: a thing")
                .file("a.txt", "1\n")
                .change_id("Isame"),
            commit("second: the same trailer")
                .file("b.txt", "2\n")
                .change_id("Isame"),
        ])
        .await;
        let server = server(&repo).await;
        let (_, body) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;

        let changes = body["series"]["changes"].as_array().unwrap();
        let keys: Vec<_> = changes.iter().map(|c| c["key"].as_str().unwrap()).collect();
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1], "one key would open two changes at once");
        assert!(
            keys.iter().any(|k| k.starts_with("sha-")),
            "the later one falls back to its hash: {keys:?}"
        );

        // And each one answers with its own file.
        for (key, file) in keys.iter().zip(["b.txt", "a.txt"]) {
            let (status, files) = json(
                server.clone(),
                get_with_cookie(&format!("/api/changes/{key}/files"), TOKEN),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(tree_paths(&files), [file]);
        }
    }

    #[tokio::test]
    async fn comparing_two_versions_leaves_the_rebase_out() {
        // A change on one base, then the same change on a base that moved a
        // dozen other files. Reading version 1 against version 2 must show
        // the work, not the rebase.
        let repo = build_repo(&[
            commit("base one").file("keep.txt", "0\n"),
            commit("work: the change")
                .file("mine.txt", "first try\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        repo.git(&["switch", "--quiet", "--detach", "HEAD~1"]).await;
        for name in ["a", "b", "c"] {
            std::fs::write(repo.path().join(format!("{name}.txt")), "new base\n").unwrap();
        }
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "-m", "base two: a dozen other files"])
            .await;
        repo.git(&["cherry-pick", &first]).await;
        std::fs::write(repo.path().join("mine.txt"), "second try\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        let mut opts = Options::new();
        opts.prevs = vec![first];
        let server = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));

        let (status, files) = json(
            server,
            get_with_cookie("/api/changes/Iwork/files?ps=2&base=ps:1", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            tree_paths(&files),
            ["mine.txt"],
            "the rebase brought a.txt, b.txt and c.txt"
        );
    }

    #[tokio::test]
    async fn the_commit_message_is_a_file_of_the_change() {
        let repo = fixture().await;
        let (status, body) = json(
            server(&repo).await,
            get_with_cookie("/api/changes/I8f3ac21/diff?file=/COMMIT_MSG", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["path"], "/COMMIT_MSG");
        assert_eq!(body["status"], "added", "the parent carries another one");
        assert_eq!(body["hunks"][0]["rows"][0]["text"], "second: go on");
        assert_eq!(body["hunks"][0]["rows"][0]["kind"], "add");
    }

    #[tokio::test]
    async fn the_message_of_one_version_reads_against_the_other() {
        let repo = build_repo(&[
            commit("base").file("keep.txt", "0\n"),
            commit("work: the old subject")
                .file("mine.txt", "one\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;
        repo.git(&[
            "commit",
            "--amend",
            "--quiet",
            "-m",
            "work: the new subject\n\nChange-Id: Iwork\n",
        ])
        .await;

        let mut opts = Options::new();
        opts.prevs = vec![first];
        let server = app(AppState::new(
            session_of(&repo, opts).await,
            TOKEN.to_owned(),
        ));

        let (status, body) = json(
            server,
            get_with_cookie(
                "/api/changes/Iwork/diff?ps=2&base=ps:1&file=/COMMIT_MSG",
                TOKEN,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "modified");
        let rows = body["hunks"][0]["rows"].as_array().unwrap();
        let kinds: Vec<_> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"remove") && kinds.contains(&"add"));
    }

    #[tokio::test]
    async fn a_comment_on_the_message_follows_it_to_the_next_version() {
        let repo = build_repo(&[
            commit("base").file("keep.txt", "0\n"),
            commit("work: a subject")
                .body("The reason it is done.")
                .file("mine.txt", "one\n")
                .change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        let mut opts = Options::new();
        opts.prevs = vec![first.clone()];
        let root = repo.path().join(".qreview-test");
        let comment = serde_json::json!({
            "scope": "line",
            "file": "/COMMIT_MSG",
            "startLine": 3,
            "side": "new",
            "body": "Say why, not what.",
        });

        // The comment is written against the first version.
        let session = Session::with(
            repo.path(),
            &Options {
                gerrit: false,
                ..opts.clone()
            },
            Languages::new(),
            std::sync::Arc::new(crate::highlight::Highlighter::new()),
            Some(crate::store::Store::at(root.as_path())),
        )
        .await
        .unwrap();
        let server = app(AppState::new(session, TOKEN.to_owned()));
        let request = Request::builder()
            .method("POST")
            .uri("/api/changes/Iwork/comments")
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(comment.to_string()))
            .unwrap();
        let (status, _) = json(server, request).await;
        assert_eq!(status, StatusCode::CREATED);

        // The subject is amended. The line the comment sits on did not move.
        repo.git(&[
            "commit",
            "--amend",
            "--quiet",
            "-m",
            "work: a better subject\n\nThe reason it is done.\n\nChange-Id: Iwork\n",
        ])
        .await;

        let session = Session::with(
            repo.path(),
            &Options {
                gerrit: false,
                ..opts
            },
            Languages::new(),
            std::sync::Arc::new(crate::highlight::Highlighter::new()),
            Some(crate::store::Store::at(root.as_path())),
        )
        .await
        .unwrap();
        let (status, body) = json(
            app(AppState::new(session, TOKEN.to_owned())),
            get_with_cookie("/api/changes/Iwork/comments", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let placed = &body["placed"][0];
        assert_eq!(placed["line"], 3, "the comment is still on its line");
        assert_eq!(placed["lost"], false);
    }

    #[tokio::test]
    async fn an_empty_address_asks_nobody() {
        let repo = fixture().await;
        let session = session_of(&repo, Options::new()).await;
        // The default is the home of the project, and no test talks to it.
        let mut config = crate::config::Config::default();
        config.update.url = Some(String::new());
        let state =
            AppState::new(session, TOKEN.to_owned()).with_config(config, repo.path().to_path_buf());

        let (status, body) = json(app(state), get_with_cookie("/api/update", TOKEN)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["latest"], serde_json::Value::Null);
        assert_eq!(body["newer"], false);
    }

    #[tokio::test]
    async fn an_address_that_answers_nothing_is_not_an_error() {
        let repo = fixture().await;
        let session = session_of(&repo, Options::new()).await;
        let mut config = crate::config::Config::default();
        // A port nothing listens on. curl fails, and the reader is not told.
        config.update.url = Some("http://127.0.0.1:1/latest".to_owned());
        let state =
            AppState::new(session, TOKEN.to_owned()).with_config(config, repo.path().to_path_buf());

        let (status, body) = json(app(state), get_with_cookie("/api/update", TOKEN)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["latest"], serde_json::Value::Null);
        assert_eq!(body["newer"], false);
    }

    #[tokio::test]
    async fn the_session_answers_with_every_comment_it_holds() {
        let repo = fixture().await;
        let server = server(&repo).await;

        for (line, body) in [(2, "The second line."), (1, "The first line.")] {
            let comment = serde_json::json!({
                "scope": "line",
                "file": "src/a.blk",
                "side": "new",
                "startLine": line,
                "body": body,
            });
            let request = Request::builder()
                .method("POST")
                .uri("/api/changes/I8f3ac21/comments")
                .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(comment.to_string()))
                .unwrap();
            let (status, _) = json(server.clone(), request).await;
            assert_eq!(status, StatusCode::CREATED);
        }

        let (status, body) = json(server.clone(), get_with_cookie("/api/comments", TOKEN)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["key"], "I8f3ac21");
        assert_eq!(body[0]["subject"], "second: go on");
        // The order of the export, not the order they were written in.
        assert_eq!(body[0]["comments"][0]["body"], "The first line.");
        assert_eq!(body[0]["comments"][1]["body"], "The second line.");

        // The file list is unchanged by it: the counts per file are the
        // browser's business, out of this one answer.
        let (_, files) = json(
            server,
            get_with_cookie("/api/changes/I8f3ac21/files", TOKEN),
        )
        .await;
        assert_eq!(files[0]["path"], "/COMMIT_MSG");
        assert_eq!(files[1]["path"], "src/a.blk");
    }

    #[tokio::test]
    async fn a_comment_on_a_range_keeps_both_ends_and_the_characters() {
        let repo = fixture().await;
        let server = server(&repo).await;
        let body = serde_json::json!({
            "scope": "range",
            "file": "src/a.blk",
            "side": "new",
            "startLine": 1,
            "endLine": 2,
            "startChar": 1,
            "endChar": 3,
            "body": "These two lines belong together.",
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/changes/I8f3ac21/comments")
            .header(header::COOKIE, format!("{}={TOKEN}", auth::COOKIE))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let (status, _) = json(server.clone(), request).await;
        assert_eq!(status, StatusCode::CREATED);

        let (status, read) = json(
            server,
            get_with_cookie("/api/changes/I8f3ac21/comments", TOKEN),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let anchor = &read["comments"][0]["anchor"];
        assert_eq!(read["comments"][0]["scope"], "range");
        assert_eq!(anchor["startLine"], 1);
        assert_eq!(anchor["endLine"], 2);
        assert_eq!(anchor["startChar"], 1);
        assert_eq!(anchor["endChar"], 3);
        assert_eq!(read["placed"][0]["line"], 1);
        assert_eq!(read["placed"][0]["endLine"], 2);
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

    #[tokio::test]
    async fn a_refresh_reads_the_commits_git_holds_now() {
        let repo = fixture().await;
        let server = server(&repo).await;

        let (_, before) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(before["series"]["changes"][0]["subject"], "second: go on");

        repo.add(&commit("third: one more").file("src/a.blk", "one\nTWO\nthree\n"))
            .await;

        let (status, after) = json(server, post("/api/session/refresh", "")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(after["series"]["changes"][0]["subject"], "third: one more");
        assert_eq!(
            after["series"]["changes"][1]["subject"], "second: go on",
            "and the commits under it are the same ones"
        );
    }

    #[tokio::test]
    async fn a_refresh_follows_an_amend() {
        let repo = fixture().await;
        let server = server(&repo).await;

        let (_, before) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        let was = before["series"]["changes"][0]["commit"]
            .as_str()
            .unwrap()
            .to_owned();

        std::fs::write(repo.path().join("src/a.blk"), "one\nTHREE\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&[
            "commit",
            "--amend",
            "-m",
            "second: say it better\n\nChange-Id: I8f3ac21\n",
        ])
        .await;

        let (status, after) = json(server.clone(), post("/api/session/refresh", "")).await;

        assert_eq!(status, StatusCode::OK);
        let change = &after["series"]["changes"][0];
        assert_eq!(change["key"], "I8f3ac21", "the Change-Id keeps the key");
        assert_eq!(change["subject"], "second: say it better");
        assert_ne!(change["commit"].as_str().unwrap(), was);

        // And the diff the routes answer with is the one of that commit.
        let (_, diff) = json(
            server,
            get_with_cookie("/api/changes/I8f3ac21/diff?file=src/a.blk", TOKEN),
        )
        .await;
        assert_eq!(diff["hunks"][0]["rows"][2]["text"], "THREE");
    }

    #[tokio::test]
    async fn a_refresh_loads_the_batches_the_reader_had_loaded() {
        let repo = merged().await;
        // One commit above the merge, so the first batch stops on it.
        repo.add(&commit("after the merge").file("after.txt", "1\n"))
            .await;
        let server = server(&repo).await;

        let (_, first) = json(server.clone(), get_with_cookie("/api/session", TOKEN)).await;
        assert_eq!(first["series"]["changes"].as_array().unwrap().len(), 1);

        // The merge, and the two commits under it.
        let (_, after) = json(server.clone(), post("/api/series/extend", r#"{"count":2}"#)).await;
        assert_eq!(after["changes"].as_array().unwrap().len(), 4);

        repo.add(&commit("on top of it all").file("after.txt", "2\n"))
            .await;

        let (status, again) = json(server, post("/api/session/refresh", "")).await;

        assert_eq!(status, StatusCode::OK);
        let changes = again["series"]["changes"].as_array().unwrap();
        assert_eq!(changes[0]["subject"], "on top of it all");
        assert_eq!(
            changes.len(),
            5,
            "the batch the reader had loaded is read again, not lost"
        );
        assert_eq!(changes[2]["subject"], "Merge side into main");
        assert_eq!(changes[3]["subject"], "main work");
    }
}

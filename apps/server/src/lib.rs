//! Shared wiring for the braincrawl native server.
//!
//! Exposed so that integration tests in `tests/` can call [`make_app`] without
//! duplicating the store-construction logic.
//!
//! ## Note on `!Send` futures
//!
//! The backend traits use `#[async_trait(?Send)]`, which boxes futures without a
//! `Send` bound.  Axum requires `Send` handler futures.  To bridge this, every
//! handler wraps its store call in [`run_blocking`]: the future is created and
//! driven to completion on a blocking thread via `Handle::block_on`, so it never
//! crosses a thread boundary.

pub mod handlers;
mod watch;

pub use watch::spawn_l3_watcher;

use std::path::PathBuf;
use std::sync::Arc;

use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, FromRef, Path, Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use braincrawl_auth::{AuthOutcome, SharedSecret};
use braincrawl_blob_fs::FsBlobStore;
use braincrawl_coord_local::{LocalCoordinator, SystemClock, UuidGen};
use braincrawl_core::{
    traits::{IdResolver, JobEnqueuer},
    types::{
        Alias, CanonicalId, ContentOutcome, DomainError, EdgeDir, EdgeInput, JobSpec, ArtifactRole,
        WorkRecord,
    },
    usecases::Store,
};
use braincrawl_store_sqlite::SqliteStore;
use serde::Deserialize;

// ─── !Send bridge ─────────────────────────────────────────────────────────────

/// Drive a `!Send` future on the blocking thread pool.
///
/// `async_trait(?Send)` boxes futures without a `Send` bound; this helper
/// lets us produce those futures inside a `spawn_blocking` closure so they
/// never cross thread boundaries, satisfying axum's `Send` requirement.
async fn run_blocking<F, Fut, T>(make: F) -> T
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = T>,
    T: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(make()))
        .await
        .expect("blocking task panicked")
}

// ─── NoopResolver ─────────────────────────────────────────────────────────────

/// `IdResolver` that always returns `None` (the resolver cache is unused in Phase 2).
pub struct NoopResolver;

#[async_trait::async_trait(?Send)]
impl IdResolver for NoopResolver {
    async fn resolve(
        &self,
        _ns: &str,
        _val: &str,
    ) -> Result<Option<CanonicalId>, DomainError> {
        Ok(None)
    }
    async fn remember(
        &self,
        _id: &CanonicalId,
        _ns: &str,
        _val: &str,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

// ─── LocalStore ───────────────────────────────────────────────────────────────

pub type LocalStore = Store<
    SqliteStore,
    FsBlobStore,
    SqliteStore,
    NoopResolver,
    LocalCoordinator,
    SystemClock,
    UuidGen,
>;

/// Construct the local `Store` from a SQLite path and blob root directory.
pub fn make_store(db_path: &str, blob_root: &str) -> Result<LocalStore, String> {
    let meta = SqliteStore::open(db_path).map_err(|e| e.to_string())?;
    let artifacts = SqliteStore::open(db_path).map_err(|e| e.to_string())?;
    let blob = FsBlobStore::new(blob_root);
    Ok(Store {
        meta,
        blob,
        artifacts,
        resolver: NoopResolver,
        coord: LocalCoordinator,
        clock: SystemClock,
        id_gen: UuidGen,
    })
}

// ─── App state ────────────────────────────────────────────────────────────────

/// Router state: the `LocalStore` plus the optional L3 root. `l3_root` is
/// `None` when `BRAINCRAWL_L3_ROOT` is unset — the graph endpoint then 404s
/// and everything else keeps working.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<LocalStore>,
    pub l3_root: Option<PathBuf>,
    /// Debounced "changed" signal from the L3 file watcher; fed to
    /// `/api/events` (SSE). Always present, even when `l3_root` is `None` —
    /// it just never fires in that case.
    pub changes: tokio::sync::broadcast::Sender<()>,
}

impl FromRef<AppState> for Arc<LocalStore> {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

/// Authentication configuration passed explicitly into [`make_app`].
///
/// Constructed by `main` from env vars; tests construct it directly so
/// behaviour is deterministic without env state.
pub struct AuthConfig {
    pub disabled: bool,
    pub allowlist: SharedSecret,
}

async fn gate(
    State(cfg): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if cfg.disabled {
        return next.run(request).await;
    }
    let hdr = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    match braincrawl_auth::authorize(&cfg.allowlist, hdr.as_deref()) {
        AuthOutcome::Authenticated(tenant) => {
            let mut request = request;
            request.extensions_mut().insert(tenant);
            next.run(request).await
        }
        AuthOutcome::Unauthenticated => StatusCode::UNAUTHORIZED.into_response(),
        AuthOutcome::Forbidden => StatusCode::FORBIDDEN.into_response(),
    }
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct HaveRequest {
    pub ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct NeighborhoodHttpRequest {
    pub seeds: Vec<String>,
    pub dir: String,
    pub depth: u32,
    pub max_nodes: u32,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn parse_alias(id_str: &str) -> Option<Alias> {
    let pos = id_str.find(':')?;
    Some(Alias {
        namespace: id_str[..pos].to_string(),
        value: id_str[pos + 1..].to_string(),
    })
}

fn parse_artifact_role(s: &str) -> Option<ArtifactRole> {
    ArtifactRole::parse(s)
}

fn domain_status(e: &DomainError) -> StatusCode {
    match e {
        DomainError::NotFound => StatusCode::NOT_FOUND,
        DomainError::Conflict => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// PUT /works
async fn handler_put_work(
    State(store): State<Arc<LocalStore>>,
    Json(record): Json<WorkRecord>,
) -> impl IntoResponse {
    let result = run_blocking(move || async move { store.put_work(record).await }).await;
    match result {
        Ok(id) => (
            StatusCode::OK,
            Json(serde_json::json!({"id": format!("guid:{}", id.0)})),
        )
            .into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// PUT /edges
async fn handler_put_edges(
    State(store): State<Arc<LocalStore>>,
    Json(edges): Json<Vec<EdgeInput>>,
) -> impl IntoResponse {
    let result = run_blocking(move || async move { store.put_edges(edges).await }).await;
    match result {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({"count": count}))).into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// POST /works/have
async fn handler_have(
    State(store): State<Arc<LocalStore>>,
    Json(req): Json<HaveRequest>,
) -> impl IntoResponse {
    let aliases: Vec<Alias> = req.ids.iter().filter_map(|s| parse_alias(s)).collect();
    let result = run_blocking(move || async move { store.have(aliases).await }).await;
    match result {
        Ok(present) => {
            let ids: Vec<String> = present
                .iter()
                .map(|a| format!("{}:{}", a.namespace, a.value))
                .collect();
            Json(ids).into_response()
        }
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// GET /works/*path  — dispatches to get_work / get_content / get_edges.
///
/// Alias values may contain `/` (e.g. DOI "10.99/smoke"), so we cannot use
/// axum `:param` (single-segment).  A wildcard `*path` captures the full tail
/// and we dispatch by structural suffix.
async fn handler_works_get(
    State(store): State<Arc<LocalStore>>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    // /works/*path/edges
    if let Some(id_str) = path.strip_suffix("/edges") {
        let a = match parse_alias(id_str) {
            Some(a) => a,
            None => return (StatusCode::BAD_REQUEST, "invalid id").into_response(),
        };
        let dir = match params.get("dir").map(|s| s.as_str()) {
            Some("forward") => EdgeDir::Forward,
            Some("backward") => EdgeDir::Backward,
            _ => return (StatusCode::BAD_REQUEST, "dir must be forward or backward").into_response(),
        };
        let cursor = params.get("cursor").cloned();
        let limit = params
            .get("limit")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(20)
            .min(200);
        let result =
            run_blocking(move || async move { store.get_edges(a, dir, cursor, limit).await })
                .await;
        return match result {
            Ok((edges, cursor)) => {
                Json(serde_json::json!({ "edges": edges, "cursor": cursor })).into_response()
            }
            Err(e) => (domain_status(&e), e.to_string()).into_response(),
        };
    }

    // /works/*path/content/{kind}
    if let Some(pos) = path.rfind("/content/") {
        let id_str = &path[..pos];
        let role_str = &path[pos + "/content/".len()..];
        let a = match parse_alias(id_str) {
            Some(a) => a,
            None => return (StatusCode::BAD_REQUEST, "invalid id").into_response(),
        };
        let kind = match parse_artifact_role(role_str) {
            Some(k) => k,
            None => return (StatusCode::BAD_REQUEST, "invalid kind").into_response(),
        };
        let _ = body; // not used for GET
        let result =
            run_blocking(move || async move { store.get_content(a, kind).await }).await;
        return match result {
            Ok(ContentOutcome::Bytes { bytes, mime, content_hash: _ }) => {
                let headers = [(axum::http::header::CONTENT_TYPE, mime)];
                (StatusCode::OK, headers, bytes).into_response()
            }
            Ok(ContentOutcome::Pending) => StatusCode::ACCEPTED.into_response(),
            Ok(ContentOutcome::Absent) => StatusCode::NOT_FOUND.into_response(),
            Err(e) => (domain_status(&e), e.to_string()).into_response(),
        };
    }

    // /works/*id  — plain GET work
    let a = match parse_alias(&path) {
        Some(a) => a,
        None => return (StatusCode::BAD_REQUEST, "expected namespace:value").into_response(),
    };
    let result = run_blocking(move || async move { store.get_work(a).await }).await;
    match result {
        Ok(Some(view)) => Json(view).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// PUT /works/*path  — dispatches to put_content (only sub-resource PUT).
async fn handler_works_put(
    State(store): State<Arc<LocalStore>>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    // /works/*path/content/{kind}
    if let Some(pos) = path.rfind("/content/") {
        let id_str = &path[..pos];
        let role_str = &path[pos + "/content/".len()..];
        let a = match parse_alias(id_str) {
            Some(a) => a,
            None => return (StatusCode::BAD_REQUEST, "invalid id").into_response(),
        };
        let kind = match parse_artifact_role(role_str) {
            Some(k) => k,
            None => return (StatusCode::BAD_REQUEST, "invalid kind").into_response(),
        };
        let mime = match params.get("mime").cloned() {
            Some(m) => m,
            None => return (StatusCode::BAD_REQUEST, "missing mime").into_response(),
        };
        let source = params.get("source").cloned();
        let source_url = params.get("source_url").cloned();
        let fetched_at = params
            .get("fetched_at")
            .cloned()
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        let bytes = body.to_vec();
        let result = run_blocking(move || async move {
            store
                .put_content(a, kind, bytes, mime, source, source_url, fetched_at)
                .await
        })
        .await;
        return match result {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (domain_status(&e), e.to_string()).into_response(),
        };
    }

    StatusCode::NOT_FOUND.into_response()
}

/// GET /stats
async fn handler_stats(State(store): State<Arc<LocalStore>>) -> impl IntoResponse {
    let result = run_blocking(move || async move { store.stats().await }).await;
    match result {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// GET /health — unauthenticated liveness probe.
///
/// Sits outside the auth gate so consumers can check the shared server is up
/// without a token. Returns 200 with a small JSON body.
async fn handler_health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "braincrawl"}))
}

/// POST /graph/neighborhood
async fn handler_neighborhood(
    State(store): State<Arc<LocalStore>>,
    Json(req): Json<NeighborhoodHttpRequest>,
) -> impl IntoResponse {
    let dir = match req.dir.as_str() {
        "forward" => EdgeDir::Forward,
        "backward" => EdgeDir::Backward,
        _ => return (StatusCode::BAD_REQUEST, "dir must be forward or backward").into_response(),
    };
    let seeds: Vec<Alias> = req.seeds.iter().filter_map(|s| parse_alias(s)).collect();
    let depth = req.depth;
    let max_nodes = req.max_nodes;

    let result =
        run_blocking(move || async move { store.neighborhood(seeds, dir, depth, max_nodes).await })
            .await;
    match result {
        Ok(neighborhood) => Json(neighborhood).into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// POST /jobs — enqueue a background fetch job.
async fn handler_post_job(
    State(store): State<Arc<LocalStore>>,
    Json(spec): Json<JobSpec>,
) -> impl IntoResponse {
    let result = run_blocking(move || async move { store.meta.enqueue(spec).await }).await;
    match result {
        Ok(id) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"id": id.0})),
        )
            .into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

/// Build the axum router wired to the given store, auth config, and optional
/// L3 root (`BRAINCRAWL_L3_ROOT`; `None` disables the `/api/l3/graph` route
/// with a 404 rather than making the store construction fail).
pub fn make_app(store: Arc<LocalStore>, auth: Arc<AuthConfig>, l3_root: Option<PathBuf>) -> Router {
    // The wildcard route accepts arbitrarily large request bodies (book-sized
    // PDFs), so disable the default 2 MB body limit on it alone.
    let works_wildcard = Router::new()
        .route("/works/*path", get(handler_works_get).put(handler_works_put))
        .layer(DefaultBodyLimit::disable());

    let (changes_tx, _rx) = tokio::sync::broadcast::channel(16);
    if let Some(root) = &l3_root {
        spawn_l3_watcher(root.clone(), changes_tx.clone());
    }

    let authed = Router::new()
        // Exact static routes first so they win over wildcards.
        .route("/works/have", post(handler_have))
        .route("/works", put(handler_put_work))
        .route("/edges", put(handler_put_edges))
        .route("/jobs", post(handler_post_job))
        .route("/graph/neighborhood", post(handler_neighborhood))
        .route("/stats", get(handler_stats))
        .route("/api/l3/graph", get(handlers::handler_l3_graph))
        .route("/api/l3/docs", get(handlers::handler_l3_docs_list))
        .route(
            "/api/l3/docs/:slug",
            get(handlers::handler_l3_doc_get).put(handlers::handler_l3_doc_put),
        )
        .route("/api/l3/agent", get(handlers::handler_l3_agent_list))
        .route(
            "/api/l3/agent/:name",
            get(handlers::handler_l3_agent_get).put(handlers::handler_l3_agent_put),
        )
        .route("/api/events", get(handlers::handler_events))
        .merge(works_wildcard)
        .layer(axum::middleware::from_fn_with_state(auth, gate))
        .with_state(AppState {
            store,
            l3_root,
            changes: changes_tx,
        });

    // `/health` is merged outside the auth layer: an unauthenticated liveness
    // probe so consumers can detect the shared server without a token.
    Router::new().route("/health", get(handler_health)).merge(authed)
}

/// Mount the built web UI (`web/dist`) at `/web`, turning the API server into a
/// single service that also serves the SolidJS app. Unmatched paths under `/web`
/// fall back to `index.html` so the client-side router owns them. The web build
/// must be produced with Vite `base: '/web/'` so its asset URLs resolve here.
pub fn serve_web(app: Router, web_root: PathBuf) -> Router {
    use tower_http::services::{ServeDir, ServeFile};
    let index = web_root.join("index.html");
    let serve = ServeDir::new(web_root).fallback(ServeFile::new(index));
    app.nest_service("/web", serve)
}

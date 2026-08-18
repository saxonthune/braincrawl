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
        Alias, Artifact, CanonicalId, ContentOutcome, DomainError, EdgeDir, EdgeInput, JobSpec,
        ArtifactRole, WorkRecord, WorkSearchFilter,
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
    /// OpenRouter key for the `/api/llm/*` proxy (`OPENROUTER_API_KEY`);
    /// `None` makes the proxy answer 503 and clients must bring their own key.
    pub openrouter_key: Option<String>,
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
        scheme: id_str[..pos].to_string(),
        value: id_str[pos + 1..].to_string(),
    })
}

fn parse_artifact_role(s: &str) -> Option<ArtifactRole> {
    ArtifactRole::parse(s)
}

/// Parse the optional `derived_from_role` / `derived_from_version` query params.
/// Both present → `Some((role, version))`; both absent → `None`. An invalid role
/// or a non-numeric version is an `Err` naming the problem, for a 400 response.
fn parse_derived_from(
    params: &HashMap<String, String>,
) -> Result<Option<(ArtifactRole, u32)>, &'static str> {
    match (params.get("derived_from_role"), params.get("derived_from_version")) {
        (None, None) => Ok(None),
        (Some(role_str), Some(version_str)) => {
            let role = parse_artifact_role(role_str).ok_or("invalid derived_from_role")?;
            let version: u32 = version_str.parse().map_err(|_| "invalid derived_from_version")?;
            Ok(Some((role, version)))
        }
        _ => Err("derived_from_role and derived_from_version must be given together"),
    }
}

/// `Artifact` has no `Serialize` impl in `crates/core`, so the HTTP surface
/// projects its fields into JSON here.
fn artifact_json(a: &Artifact) -> serde_json::Value {
    let (derived_from_role, derived_from_version) = match &a.derived_from {
        Some((role, version)) => (Some(role.as_str()), Some(*version)),
        None => (None, None),
    };
    serde_json::json!({
        "canonical_id": a.canonical_id.0,
        "role": a.role.as_str(),
        "version": a.version,
        "r2_key": a.r2_key,
        "content_hash": a.content_hash,
        "byte_size": a.byte_size,
        "mime": a.mime,
        "source": a.source,
        "source_url": a.source_url,
        "fetched_at": a.fetched_at,
        "is_current": a.is_current,
        "derived_from_role": derived_from_role,
        "derived_from_version": derived_from_version,
    })
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
                .map(|a| format!("{}:{}", a.scheme, a.value))
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

    // /works/*id/artifacts
    if let Some(id_str) = path.strip_suffix("/artifacts") {
        let a = match parse_alias(id_str) {
            Some(a) => a,
            None => return (StatusCode::BAD_REQUEST, "invalid id").into_response(),
        };
        let role = match params.get("role").map(|s| parse_artifact_role(s)) {
            Some(Some(k)) => Some(k),
            Some(None) => return (StatusCode::BAD_REQUEST, "invalid role").into_response(),
            None => None,
        };
        let all_versions = params
            .get("all_versions")
            .map(|s| s == "true")
            .unwrap_or(false);
        let result = run_blocking(move || async move {
            store.list_artifacts(a, role, all_versions).await
        })
        .await;
        return match result {
            Ok(artifacts) => {
                let artifacts: Vec<_> = artifacts.iter().map(artifact_json).collect();
                Json(serde_json::json!({ "artifacts": artifacts })).into_response()
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
        None => return (StatusCode::BAD_REQUEST, "expected scheme:value").into_response(),
    };
    let result = run_blocking(move || async move {
        let view = store.get_work(a.clone()).await?;
        match view {
            Some(view) => {
                let artifacts = store.list_artifacts(a, None, false).await?;
                Ok(Some((view, artifacts)))
            }
            None => Ok(None),
        }
    })
    .await;
    match result {
        Ok(Some((view, artifacts))) => {
            let mut body = serde_json::to_value(view).expect("WorkView serializes");
            let artifacts: Vec<_> = artifacts.iter().map(artifact_json).collect();
            body["artifacts"] = serde_json::Value::Array(artifacts);
            Json(body).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            format!("no work with alias '{}'", path),
        )
            .into_response(),
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
        let derived_from = match parse_derived_from(&params) {
            Ok(d) => d,
            Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
        };
        let bytes = body.to_vec();
        let result = run_blocking(move || async move {
            store
                .put_content(a, kind, bytes, mime, source, source_url, fetched_at, derived_from)
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

/// GET /works — store-side work search.
///
/// Query params: `author`, `title` (case-insensitive substrings), `year`,
/// `with_artifact` (an artifact role the work must currently hold), `limit`
/// (default 25, max 200). At least one filter is required. Each result is a
/// merged work view plus its current artifact descriptors.
async fn handler_search_works(
    State(store): State<Arc<LocalStore>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = WorkSearchFilter {
        author: params.get("author").cloned(),
        title: params.get("title").cloned(),
        year: params.get("year").and_then(|s| s.parse::<u32>().ok()),
    };
    let with_artifact = match params.get("with_artifact").map(|s| parse_artifact_role(s)) {
        Some(Some(k)) => Some(k),
        Some(None) => return (StatusCode::BAD_REQUEST, "invalid with_artifact role").into_response(),
        None => None,
    };
    if filter.is_empty() && with_artifact.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "at least one of author, title, year, with_artifact is required",
        )
            .into_response();
    }
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(25)
        .min(200);
    let result = run_blocking(move || async move {
        let views = store.search_works(&filter, with_artifact, limit).await?;
        let mut out = Vec::with_capacity(views.len());
        for view in views {
            let artifacts = store
                .list_artifacts_by_id(view.canonical_id.clone(), None, false)
                .await?;
            out.push((view, artifacts));
        }
        Ok::<_, DomainError>(out)
    })
    .await;
    match result {
        Ok(rows) => {
            let works: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(view, artifacts)| {
                    let mut body = serde_json::to_value(view).expect("WorkView serializes");
                    let artifacts: Vec<_> = artifacts.iter().map(artifact_json).collect();
                    body["artifacts"] = serde_json::Value::Array(artifacts);
                    body
                })
                .collect();
            Json(serde_json::json!({ "works": works })).into_response()
        }
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// GET /stats
async fn handler_stats(State(store): State<Arc<LocalStore>>) -> impl IntoResponse {
    let result = run_blocking(move || async move { store.stats().await }).await;
    match result {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// Read the shared `cursor` / `limit` params for the `/export/*` routes.
/// Limit defaults to 100 and is capped at 200 per page.
fn export_page_params(params: &HashMap<String, String>) -> (Option<String>, u32) {
    let cursor = params.get("cursor").cloned();
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(100)
        .clamp(1, 200);
    (cursor, limit)
}

/// GET /export/nodes
async fn handler_export_nodes(
    State(store): State<Arc<LocalStore>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (cursor, limit) = export_page_params(&params);
    let result = run_blocking(move || async move {
        store.export_nodes(cursor.as_deref(), limit).await
    })
    .await;
    match result {
        Ok((items, cursor)) => {
            Json(serde_json::json!({ "items": items, "cursor": cursor })).into_response()
        }
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// GET /export/edges
async fn handler_export_edges(
    State(store): State<Arc<LocalStore>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (cursor, limit) = export_page_params(&params);
    let result = run_blocking(move || async move {
        store.export_edge_assertions(cursor.as_deref(), limit).await
    })
    .await;
    match result {
        Ok((items, cursor)) => {
            Json(serde_json::json!({ "items": items, "cursor": cursor })).into_response()
        }
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// GET /export/artifacts
async fn handler_export_artifacts(
    State(store): State<Arc<LocalStore>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (cursor, limit) = export_page_params(&params);
    let result = run_blocking(move || async move {
        store.export_artifacts(cursor.as_deref(), limit).await
    })
    .await;
    match result {
        Ok((items, cursor)) => {
            let items: Vec<_> = items.iter().map(artifact_json).collect();
            Json(serde_json::json!({ "items": items, "cursor": cursor })).into_response()
        }
        Err(e) => (domain_status(&e), e.to_string()).into_response(),
    }
}

/// State for the unauthenticated `/health` route: the store (to read applied
/// migrations) and the configured L3 root, both needed outside the auth gate.
#[derive(Clone)]
struct HealthState {
    store: Arc<LocalStore>,
    l3_root: Option<PathBuf>,
}

/// GET /health — unauthenticated liveness probe.
///
/// Sits outside the auth gate so consumers can check the shared server is up
/// without a token. Returns 200 with a small JSON body.
async fn handler_health(State(state): State<HealthState>) -> impl IntoResponse {
    let store = state.store.clone();
    let applied = run_blocking(move || async move { store.applied_migrations().await })
        .await
        .unwrap_or_default();
    let compiled: Vec<&str> = braincrawl_sql::migrations().iter().map(|(name, _)| *name).collect();
    let l3_root = state.l3_root.as_ref().map(|p| p.display().to_string());

    Json(serde_json::json!({
        "status": "ok",
        "service": "braincrawl",
        "version": env!("BRAINCRAWL_BUILD"),
        "migrations_applied": applied,
        "migrations_compiled": compiled,
        "l3_root": l3_root,
    }))
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
pub fn make_app(
    store: Arc<LocalStore>,
    auth: Arc<AuthConfig>,
    l3_root: Option<PathBuf>,
    openrouter_key: Option<String>,
) -> Router {
    // The wildcard route accepts arbitrarily large request bodies (book-sized
    // PDFs), so disable the default 2 MB body limit on it alone.
    let works_wildcard = Router::new()
        .route("/works/*path", get(handler_works_get).put(handler_works_put))
        .layer(DefaultBodyLimit::disable());

    let (changes_tx, _rx) = tokio::sync::broadcast::channel(16);
    if let Some(root) = &l3_root {
        spawn_l3_watcher(root.clone(), changes_tx.clone());
    }

    let health = Router::new()
        .route("/health", get(handler_health))
        .with_state(HealthState { store: store.clone(), l3_root: l3_root.clone() });

    let authed = Router::new()
        // Exact static routes first so they win over wildcards.
        .route("/works/have", post(handler_have))
        .route("/works", put(handler_put_work).get(handler_search_works))
        .route("/edges", put(handler_put_edges))
        .route("/jobs", post(handler_post_job))
        .route("/graph/neighborhood", post(handler_neighborhood))
        .route("/stats", get(handler_stats))
        .route("/export/nodes", get(handler_export_nodes))
        .route("/export/edges", get(handler_export_edges))
        .route("/export/artifacts", get(handler_export_artifacts))
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
        .route("/api/llm/*path", post(handlers::handler_llm_proxy))
        .merge(works_wildcard)
        .layer(axum::middleware::from_fn_with_state(auth, gate))
        .with_state(AppState {
            store,
            l3_root,
            changes: changes_tx,
            openrouter_key,
        });

    // `/health` is merged outside the auth layer: an unauthenticated liveness
    // probe so consumers can detect the shared server without a token.
    health.merge(authed)
}

/// Serve the built web UI (`web/dist`) from the root — API routes keep their
/// exact paths and every unmatched path falls back to the SPA, mirroring the
/// worker's production layout (assets at the origin root, `run_worker_first`
/// for the API). `/web` stays as an alias for old bookmarks. The build's
/// Vite `base: "./"` makes asset URLs relative, so both mounts resolve.
pub fn serve_web(app: Router, web_root: PathBuf) -> Router {
    use tower_http::services::{ServeDir, ServeFile};
    let index = web_root.join("index.html");
    let serve = ServeDir::new(web_root).fallback(ServeFile::new(index));
    app.nest_service("/web", serve.clone()).fallback_service(serve)
}

//! Wasm entry point for the Cloudflare Worker.
//!
//! Wires R2 + D1 + KV + Durable Object bindings into the core use-case `Store`
//! and routes the same OpenAPI surface as `apps/server` (doc02.02.00).
//!
//! ## Routing
//!
//! Mirrors `apps/server` exactly:
//! - `POST /works/have`
//! - `PUT /works`
//! - `PUT /edges`
//! - `POST /graph/neighborhood`
//! - `GET /stats`
//! - `GET /works/*id`                   → get_work
//! - `GET /works/*id/edges`             → get_edges
//! - `GET /works/*id/content/{kind}`    → get_content
//! - `PUT /works/*id/content/{kind}`    → put_content
//!
//! Plus worker-only Research Collection doc routes (see `l3` module):
//! - `GET /api/l3/docs`
//! - `GET /api/l3/docs/{slug}`
//! - `PUT /api/l3/docs/{slug}`
//! - `GET /api/l3/graph`
//!
//! Plus opaque agent context-file routes (unparsed markdown, excluded from
//! the above — see `.todo-tasks/tasks/worker-l3-agent-files.md`):
//! - `GET /api/l3/agent`
//! - `GET /api/l3/agent/{name}`
//! - `PUT /api/l3/agent/{name}`
//!
//! ## Coordinator note
//!
//! `DoCoordinator::with_lock` routes through a `WorkDurableObject` stub keyed by the
//! work's alias.  With the current `LockGuard` design (unit struct, no async drop),
//! true cross-request serialization would require routing the *entire* critical
//! section inside a single DO fetch handler.  The current implementation sends a
//! "heartbeat" request to the DO (ensuring the DO is reachable and properly keyed)
//! but the actual lock semantics are advisory: within a single Worker isolate,
//! the single-threaded Wasm event loop prevents data races already.
//! Full per-work serialization across concurrent Worker instances is a design
//! evolution that requires restructuring the `Coordinator` trait (out of scope here).

mod l3;

use async_trait::async_trait;
use braincrawl_blob_r2::R2BlobStore;
use braincrawl_core::{
    traits::{Clock, Coordinator, IdGen, LockGuard},
    types::{
        Alias, CanonicalId, ContentOutcome, DomainError, EdgeDir, EdgeInput, ArtifactRole,
        WorkRecord,
    },
    usecases::Store,
};
use braincrawl_resolver_kv::KvResolver;
use braincrawl_store_d1::D1Store;
use serde::Deserialize;
use worker::*;

// ── Type alias ───────────────────────────────────────────────────────────────

type WorkerStore = Store<D1Store, R2BlobStore, D1Store, KvResolver, DoCoordinator, WasmClock, WasmIdGen>;

// ── WasmClock ─────────────────────────────────────────────────────────────────

pub struct WasmClock;

impl Clock for WasmClock {
    fn now_rfc3339(&self) -> String {
        let ms = js_sys::Date::now() as u64;
        secs_to_rfc3339(ms / 1000)
    }
}

pub(crate) fn secs_to_rfc3339(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u32, u32, u32) {
    let mut year = 1970u32;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let months: [u32; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &dm in &months {
        if days < dm as u64 { break; }
        days -= dm as u64;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Today's date (UTC) as `yyyy-mm-dd`, for stamping L3 docs' `updated:` key.
pub(crate) fn today_utc_date() -> String {
    let ms = js_sys::Date::now() as u64;
    let (y, mo, d) = days_to_ymd(ms / 1000 / 86400);
    format!("{y:04}-{mo:02}-{d:02}")
}

// ── WasmIdGen ────────────────────────────────────────────────────────────────

pub struct WasmIdGen;

impl IdGen for WasmIdGen {
    fn new_guid(&self) -> CanonicalId {
        CanonicalId(uuid::Uuid::new_v4().to_string())
    }
}

// ── DoCoordinator ─────────────────────────────────────────────────────────────

/// `Coordinator` backed by Cloudflare Durable Objects.
///
/// Keyed by work alias: each unique alias maps to one DO instance.
/// The DO's single-threaded event loop serializes concurrent requests for the
/// same key.  See the module-level note about `LockGuard` limitations.
pub struct DoCoordinator {
    namespace: ObjectNamespace,
}

impl DoCoordinator {
    pub fn new(namespace: ObjectNamespace) -> Self {
        Self { namespace }
    }
}

#[async_trait(?Send)]
impl Coordinator for DoCoordinator {
    async fn with_lock(&self, key: &str) -> std::result::Result<LockGuard, DomainError> {
        // Route to the DO instance for this key.  The DO's single-thread queue
        // ensures only one request at a time is processed per key, providing
        // advisory serialization for the critical section that follows.
        // `map_err` converts worker::Error → DomainError so the ? operator
        // propagates into std::result::Result<LockGuard, DomainError>.
        let id = self
            .namespace
            .id_from_name(key)
            .map_err(|e| DomainError::Backend(e.to_string()))?;
        let stub = id
            .get_stub()
            .map_err(|e| DomainError::Backend(e.to_string()))?;
        let req = Request::new("http://do/lock", Method::Post)
            .map_err(|e| DomainError::Backend(e.to_string()))?;
        stub.fetch_with_request(req)
            .await
            .map_err(|e| DomainError::Backend(e.to_string()))?;
        Ok(LockGuard)
    }
}

// ── WorkDurableObject ─────────────────────────────────────────────────────────

/// Durable Object keyed by work id (alias string).
///
/// The DO's single-threaded event loop serializes concurrent `put_work` / merge
/// requests for the same work, preventing thundering-herd races on cache misses.
///
/// Rate-budget enforcement is stubbed: `POST /lock` simply acknowledges the
/// request.  Full rate-limiting (token-bucket per work) is a later evolution
/// (doc02.02.00 "later").
#[durable_object]
#[allow(dead_code)]
pub struct WorkDurableObject {
    state: State,
    env: Env,
}

impl DurableObject for WorkDurableObject {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, _req: Request) -> worker::Result<Response> {
        // Single-threaded; concurrent requests for the same key queue here.
        // Stub: acknowledge immediately. Rate-budget hook: not yet implemented.
        Response::ok("ok")
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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


fn domain_status(e: &DomainError) -> u16 {
    match e {
        DomainError::NotFound => 404,
        DomainError::Conflict => 409,
        _ => 500,
    }
}

fn err_response(e: &DomainError) -> worker::Result<Response> {
    Response::error(e.to_string(), domain_status(e))
}

fn bad_request(msg: &str) -> worker::Result<Response> {
    Response::error(msg, 400)
}

// ── Store construction ────────────────────────────────────────────────────────

fn build_store(env: &Env) -> worker::Result<WorkerStore> {
    let bucket = env.bucket("BLOB_BUCKET")?;
    let meta_db = env.d1("DB")?;
    let artifact_db = env.d1("DB")?;
    let kv = env.kv("ID_RESOLVER_KV")?;
    let do_ns = env.durable_object("WORK_DO")?;

    Ok(Store {
        meta: D1Store::new(meta_db),
        blob: R2BlobStore::new(bucket),
        artifacts: D1Store::new(artifact_db),
        resolver: KvResolver::new(kv),
        coord: DoCoordinator::new(do_ns),
        clock: WasmClock,
        id_gen: WasmIdGen,
    })
}

// ── Route handlers ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NeighborhoodHttpRequest {
    seeds: Vec<String>,
    dir: String,
    depth: u32,
    max_nodes: u32,
}

async fn handle_neighborhood(mut req: Request, store: &WorkerStore) -> worker::Result<Response> {
    let body: NeighborhoodHttpRequest = req.json().await?;
    let dir = match body.dir.as_str() {
        "forward" => EdgeDir::Forward,
        "backward" => EdgeDir::Backward,
        _ => return bad_request("dir must be forward or backward"),
    };
    let seeds: Vec<Alias> = body.seeds.iter().filter_map(|s| parse_alias(s)).collect();
    match store.neighborhood(seeds, dir, body.depth, body.max_nodes).await {
        Ok(neighborhood) => Response::from_json(&neighborhood),
        Err(e) => err_response(&e),
    }
}

async fn handle_stats(store: &WorkerStore) -> worker::Result<Response> {
    match store.stats().await {
        Ok(stats) => Response::from_json(&stats),
        Err(e) => err_response(&e),
    }
}

async fn handle_have(mut req: Request, store: &WorkerStore) -> worker::Result<Response> {
    #[derive(Deserialize)]
    struct HaveRequest { ids: Vec<String> }
    let body: HaveRequest = req.json().await?;
    let aliases: Vec<Alias> = body.ids.iter().filter_map(|s| parse_alias(s)).collect();
    match store.have(aliases).await {
        Ok(present) => {
            let ids: Vec<String> = present
                .iter()
                .map(|a| format!("{}:{}", a.namespace, a.value))
                .collect();
            Response::from_json(&ids)
        }
        Err(e) => err_response(&e),
    }
}

async fn handle_put_work(mut req: Request, store: &WorkerStore) -> worker::Result<Response> {
    let record: WorkRecord = req.json().await?;
    match store.put_work(record).await {
        Ok(id) => Response::from_json(&serde_json::json!({"id": format!("guid:{}", id.0)})),
        Err(e) => err_response(&e),
    }
}

async fn handle_put_edges(mut req: Request, store: &WorkerStore) -> worker::Result<Response> {
    let edges: Vec<EdgeInput> = req.json().await?;
    match store.put_edges(edges).await {
        Ok(count) => Response::from_json(&serde_json::json!({"count": count})),
        Err(e) => err_response(&e),
    }
}

async fn handle_get_work(id_str: &str, store: &WorkerStore) -> worker::Result<Response> {
    let a = match parse_alias(id_str) {
        Some(a) => a,
        None => return bad_request("expected namespace:value"),
    };
    match store.get_work(a).await {
        Ok(Some(view)) => Response::from_json(&view),
        Ok(None) => Response::error("not found", 404),
        Err(e) => err_response(&e),
    }
}

async fn handle_get_edges(
    id_str: &str,
    url: &Url,
    store: &WorkerStore,
) -> worker::Result<Response> {
    let a = match parse_alias(id_str) {
        Some(a) => a,
        None => return bad_request("expected namespace:value"),
    };
    let params: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    let dir = match params.get("dir").map(|s| s.as_str()) {
        Some("forward") => EdgeDir::Forward,
        Some("backward") => EdgeDir::Backward,
        _ => return bad_request("dir must be forward or backward"),
    };
    let cursor = params.get("cursor").cloned();
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20)
        .min(200);
    match store.get_edges(a, dir, cursor, limit).await {
        Ok((edges, cursor)) => {
            Response::from_json(&serde_json::json!({ "edges": edges, "cursor": cursor }))
        }
        Err(e) => err_response(&e),
    }
}

async fn handle_get_content(
    id_str: &str,
    role_str: &str,
    store: &WorkerStore,
) -> worker::Result<Response> {
    let a = match parse_alias(id_str) {
        Some(a) => a,
        None => return bad_request("expected namespace:value"),
    };
    let kind = match parse_artifact_role(role_str) {
        Some(k) => k,
        None => return bad_request("invalid kind"),
    };
    match store.get_content(a, kind).await {
        Ok(ContentOutcome::Bytes { bytes, mime, content_hash: _ }) => {
            let headers = Headers::new();
            headers.set("Content-Type", &mime)?;
            Ok(Response::from_bytes(bytes)?.with_headers(headers))
        }
        Ok(ContentOutcome::Pending) => Ok(Response::empty()?.with_status(202)),
        Ok(ContentOutcome::Absent) => Response::error("not found", 404),
        Err(e) => err_response(&e),
    }
}

async fn handle_put_content(
    id_str: &str,
    role_str: &str,
    url: &Url,
    mut req: Request,
    store: &WorkerStore,
) -> worker::Result<Response> {
    let a = match parse_alias(id_str) {
        Some(a) => a,
        None => return bad_request("expected namespace:value"),
    };
    let kind = match parse_artifact_role(role_str) {
        Some(k) => k,
        None => return bad_request("invalid kind"),
    };
    let params: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    let mime = match params.get("mime").cloned() {
        Some(m) => m,
        None => return bad_request("missing mime"),
    };
    let source = params.get("source").cloned();
    let source_url = params.get("source_url").cloned();
    let fetched_at = params
        .get("fetched_at")
        .cloned()
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
    let bytes = req.bytes().await?;
    match store
        .put_content(a, kind, bytes, mime, source, source_url, fetched_at)
        .await
    {
        Ok(_) => Response::empty(),
        Err(e) => err_response(&e),
    }
}

// ── Main fetch handler ────────────────────────────────────────────────────────

/// Stamp `Access-Control-Allow-Origin: *` onto any response — success or
/// error — since the PWA client authenticates via bearer header, not cookies,
/// making a wildcard origin safe.
fn with_cors(resp: worker::Result<Response>) -> worker::Result<Response> {
    let resp = resp?;
    resp.headers().set("Access-Control-Allow-Origin", "*")?;
    Ok(resp)
}

fn cors_preflight_response() -> worker::Result<Response> {
    let headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, PUT, POST, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
    headers.set("Access-Control-Max-Age", "86400")?;
    Ok(Response::empty()?.with_status(204).with_headers(headers))
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    if req.method() == Method::Options {
        return cors_preflight_response();
    }
    with_cors(route(req, env).await)
}

async fn route(req: Request, env: Env) -> worker::Result<Response> {
    let url = req.url()?;
    let path = url.path();
    let method = req.method();

    // GET /health — unauthenticated liveness probe.
    if method == Method::Get && path == "/health" {
        return Response::from_json(&serde_json::json!({"status": "ok", "service": "braincrawl"}));
    }

    // ── Auth gate ─────────────────────────────────────────────────────────────
    let secret = match env.secret("AUTH_TOKEN") {
        Ok(s) => s.to_string(),
        Err(_) => return Response::error("auth not configured", 500),
    };
    let allowlist = braincrawl_auth::SharedSecret::new(&secret, "default");
    let auth_header: Option<String> = req.headers().get("Authorization").ok().flatten();
    match braincrawl_auth::authorize(&allowlist, auth_header.as_deref()) {
        braincrawl_auth::AuthOutcome::Authenticated(_) => {}
        braincrawl_auth::AuthOutcome::Unauthenticated => {
            return Response::error("unauthorized", 401)
        }
        braincrawl_auth::AuthOutcome::Forbidden => return Response::error("forbidden", 403),
    }
    // ─────────────────────────────────────────────────────────────────────────

    let store = build_store(&env)?;

    // ── /api/l3/* ─────────────────────────────────────────────────────────────
    if let Some(rest) = path.strip_prefix("/api/l3/") {
        let bucket = env.bucket("BLOB_BUCKET")?;
        if method == Method::Get && rest == "agent" {
            return l3::handle_list_agent_files(&bucket).await;
        }
        if let Some(name) = rest.strip_prefix("agent/") {
            match method {
                Method::Get => return l3::handle_get_agent_file(name, &bucket).await,
                Method::Put => return l3::handle_put_agent_file(name, req, &bucket).await,
                _ => {}
            }
            return Response::error("not found", 404);
        }
        if method == Method::Get && rest == "graph" {
            return l3::handle_graph(&req, &bucket).await;
        }
        if method == Method::Get && rest == "docs" {
            return l3::handle_list_docs(&bucket).await;
        }
        if let Some(slug) = rest.strip_prefix("docs/") {
            match method {
                Method::Get => return l3::handle_get_doc(slug, &bucket).await,
                Method::Put => return l3::handle_put_doc(slug, &url, req, &bucket).await,
                _ => {}
            }
        }
        return Response::error("not found", 404);
    }
    // ─────────────────────────────────────────────────────────────────────────

    // POST /works/have
    if method == Method::Post && path == "/works/have" {
        return handle_have(req, &store).await;
    }

    // PUT /works  (no trailing path segment)
    if method == Method::Put && path == "/works" {
        return handle_put_work(req, &store).await;
    }

    // PUT /edges
    if method == Method::Put && path == "/edges" {
        return handle_put_edges(req, &store).await;
    }

    // POST /graph/neighborhood
    if method == Method::Post && path == "/graph/neighborhood" {
        return handle_neighborhood(req, &store).await;
    }

    // GET /stats
    if method == Method::Get && path == "/stats" {
        return handle_stats(&store).await;
    }

    // /works/*path
    if let Some(rest) = path.strip_prefix("/works/") {
        match method {
            Method::Get => {
                // GET /works/*id/edges
                if let Some(id_str) = rest.strip_suffix("/edges") {
                    return handle_get_edges(id_str, &url, &store).await;
                }
                // GET /works/*id/content/{kind}
                if let Some(pos) = rest.rfind("/content/") {
                    let id_str = &rest[..pos];
                    let role_str = &rest[pos + "/content/".len()..];
                    return handle_get_content(id_str, role_str, &store).await;
                }
                // GET /works/*id  — plain work lookup
                return handle_get_work(rest, &store).await;
            }
            Method::Put => {
                // PUT /works/*id/content/{kind}
                if let Some(pos) = rest.rfind("/content/") {
                    let id_str = &rest[..pos];
                    let role_str = &rest[pos + "/content/".len()..];
                    return handle_put_content(id_str, role_str, &url, req, &store).await;
                }
                return Response::error("not found", 404);
            }
            _ => {}
        }
    }

    Response::error("not found", 404)
}

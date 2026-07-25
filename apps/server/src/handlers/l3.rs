//! `/api/l3/*` handlers — fs-backed Research Collection docs and opaque agent
//! context files, over the configured L3 root (`BRAINCRAWL_L3_ROOT`). Mirrors
//! `apps/worker/src/l3.rs` byte-for-byte on the wire; see
//! `.todo-tasks/tasks/server-l3-parity.md`.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use axum::{
    body::Bytes,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::AppState;

const FILE_SUFFIX: &str = ".l3.md";
const PREV_DIR: &str = "_prev";
const AGENT_DIR: &str = "_agent";
const AGENT_SUFFIX: &str = ".md";
const AGENT_MAX_BYTES: usize = 64 * 1024;

fn is_valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn root_missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        "BRAINCRAWL_L3_ROOT is not configured on this server",
    )
        .into_response()
}

fn secs_to_rfc3339(secs: u64) -> String {
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
        if days < dy {
            break;
        }
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
        if days < dm as u64 {
            break;
        }
        days -= dm as u64;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Every `*.l3.md` file under `root`, recursively, skipping `_`-prefixed entries
/// and `INDEX.md` — the same rules `l3::parse` walks by. Returns `(path, slug)`
/// pairs, `slug` being the `/`-joined path relative to `root` with the suffix
/// stripped.
fn find_docs(root: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    collect_docs(root, root, &mut out);
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

fn collect_docs(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('_') {
            continue;
        }
        if path.is_dir() {
            collect_docs(root, &path, out);
            continue;
        }
        if name == "INDEX.md" || !name.ends_with(FILE_SUFFIX) {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let slug = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let slug = slug.trim_end_matches(FILE_SUFFIX).to_string();
        out.push((path, slug));
    }
}

fn doc_path(root: &Path, slug: &str) -> PathBuf {
    root.join(format!("{slug}{FILE_SUFFIX}"))
}

fn prev_doc_path(root: &Path, slug: &str) -> PathBuf {
    root.join(PREV_DIR).join(format!("{slug}{FILE_SUFFIX}"))
}

fn agent_path(root: &Path, name: &str) -> PathBuf {
    root.join(AGENT_DIR).join(format!("{name}{AGENT_SUFFIX}"))
}

/// Write `content` to `path`, via a same-directory temp file + rename so
/// the fs watcher never observes a partial write mid-debounce.
fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp_path = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)
}

// ── GET /api/l3/docs ─────────────────────────────────────────────────────────

pub async fn handler_l3_docs_list(State(state): State<AppState>) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };

    tokio::task::spawn_blocking(move || {
        let mut docs = Vec::new();
        for (path, slug) in find_docs(&root) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| secs_to_rfc3339(d.as_secs()))
                .unwrap_or_default();
            docs.push(serde_json::json!({
                "doc": slug,
                "size": meta.len(),
                "modified": modified,
            }));
        }
        axum::Json(docs).into_response()
    })
    .await
    .expect("blocking task panicked")
}

// ── GET /api/l3/docs/{slug} ──────────────────────────────────────────────────

pub async fn handler_l3_doc_get(
    State(state): State<AppState>,
    AxumPath(slug): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };
    if !is_valid_name(&slug) {
        return bad_request("invalid slug");
    }
    let prev = params.get("version").map(|v| v == "prev").unwrap_or(false);

    tokio::task::spawn_blocking(move || {
        let path = if prev { prev_doc_path(&root, &slug) } else { doc_path(&root, &slug) };
        match std::fs::read_to_string(path) {
        Ok(text) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            text,
        )
            .into_response(),
            Err(_) => not_found(),
        }
    })
    .await
    .expect("blocking task panicked")
}

// ── PUT /api/l3/docs/{slug} ──────────────────────────────────────────────────

pub async fn handler_l3_doc_put(
    State(state): State<AppState>,
    AxumPath(slug): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };
    if !is_valid_name(&slug) {
        return bad_request("invalid slug");
    }
    let force = params.get("force").map(|v| v == "1").unwrap_or(false);
    let Ok(body) = String::from_utf8(body.to_vec()) else {
        return bad_request("body must be valid UTF-8");
    };

    tokio::task::spawn_blocking(move || {
        let target_path = doc_path(&root, &slug);
        let other_docs: Vec<(String, String)> = find_docs(&root)
            .into_iter()
            .filter(|(path, _)| path != &target_path)
            .filter_map(|(path, slug)| std::fs::read_to_string(&path).ok().map(|c| (slug, c)))
            .collect();

        match l3::normalize_doc(&slug, &body, &other_docs, force) {
            l3::NormalizeOutcome::BlockingWarnings(blocking_warnings) => {
                let warnings: Vec<_> = blocking_warnings
                    .iter()
                    .map(|w| serde_json::json!({"doc": w.doc, "line": w.line, "message": w.message}))
                    .collect();
                (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"warnings": warnings})))
                    .into_response()
            }
            l3::NormalizeOutcome::AnchorConflicts(conflicts) => (
                StatusCode::CONFLICT,
                axum::Json(serde_json::json!({"conflicts": conflicts})),
            )
                .into_response(),
            l3::NormalizeOutcome::Normalized { content: final_content, .. } => {
                if target_path.exists() {
                    let prev_path = prev_doc_path(&root, &slug);
                    if let Some(parent) = prev_path.parent() {
                        if std::fs::create_dir_all(parent).is_err() {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                    }
                    if std::fs::copy(&target_path, &prev_path).is_err() {
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
                if let Some(parent) = target_path.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
                if write_atomic(&target_path, final_content.as_bytes()).is_err() {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                    final_content,
                )
                    .into_response()
            }
        }
    })
    .await
    .expect("blocking task panicked")
}

// ── GET /api/l3/agent ────────────────────────────────────────────────────────

pub async fn handler_l3_agent_list(State(state): State<AppState>) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };

    tokio::task::spawn_blocking(move || {
        let dir = root.join(AGENT_DIR);
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                let Some(name) = file_name.strip_suffix(AGENT_SUFFIX) else { continue };
                let Ok(meta) = std::fs::metadata(&path) else { continue };
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| secs_to_rfc3339(d.as_secs()))
                    .unwrap_or_default();
                files.push(serde_json::json!({
                    "name": name,
                    "size": meta.len(),
                    "modified": modified,
                }));
            }
        }
        files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        axum::Json(files).into_response()
    })
    .await
    .expect("blocking task panicked")
}

// ── GET /api/l3/agent/{name} ─────────────────────────────────────────────────

pub async fn handler_l3_agent_get(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };
    if !is_valid_name(&name) {
        return bad_request("invalid name");
    }

    tokio::task::spawn_blocking(move || match std::fs::read_to_string(agent_path(&root, &name)) {
        Ok(text) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            text,
        )
            .into_response(),
        Err(_) => not_found(),
    })
    .await
    .expect("blocking task panicked")
}

// ── PUT /api/l3/agent/{name} ─────────────────────────────────────────────────

pub async fn handler_l3_agent_put(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    body: Bytes,
) -> Response {
    let Some(root) = state.l3_root.clone() else { return root_missing() };
    if !is_valid_name(&name) {
        return bad_request("invalid name");
    }
    if body.len() > AGENT_MAX_BYTES {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }

    tokio::task::spawn_blocking(move || {
        let dir = root.join(AGENT_DIR);
        if std::fs::create_dir_all(&dir).is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if write_atomic(&agent_path(&root, &name), &body).is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        StatusCode::NO_CONTENT.into_response()
    })
    .await
    .expect("blocking task panicked")
}

// ── GET /api/l3/graph ─────────────────────────────────────────────────────────

pub async fn handler_l3_graph(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(root) = state.l3_root.clone() else {
        return (
            StatusCode::NOT_FOUND,
            "BRAINCRAWL_L3_ROOT is not configured on this server",
        )
            .into_response();
    };

    let body = tokio::task::spawn_blocking(move || {
        let (graph, _warnings) = l3::parse(&root);
        serde_json::to_vec(&graph).expect("Graph serialization is infallible")
    })
    .await
    .expect("blocking task panicked");

    let etag = format!("\"{}\"", hex_digest(&body));

    if headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(axum::http::header::ETAG, etag)],
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json".to_string()),
            (axum::http::header::ETAG, etag),
        ],
        body,
    )
        .into_response()
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

//! `/api/l3/*` handlers — R2-backed Research Collection docs. See
//! `.todo-tasks/tasks/worker-l3-docs.md` for the routes' contract.
//!
//! Each doc is one R2 object at `l3/<slug>.l3.md` (verbatim markdown); the
//! `l3` crate's in-memory API (`parse_sources`, `assign_ids_source`,
//! `collect_anchors`, `upsert_frontmatter_key`) does all parsing/normalizing.
//! Deliberately bypasses the `Store`/`BlobStore` artifact scheme — this is a
//! separate, file-shaped R2 prefix.

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use worker::{Bucket, Headers, Request, Response, Url};

const PREFIX: &str = "l3/";
const SUFFIX: &str = ".l3.md";

fn doc_key(slug: &str) -> String {
    format!("{PREFIX}{slug}{SUFFIX}")
}

fn slug_from_key(key: &str) -> Option<String> {
    key.strip_prefix(PREFIX)?.strip_suffix(SUFFIX).map(str::to_string)
}

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn bad_request(msg: &str) -> worker::Result<Response> {
    Response::error(msg, 400)
}

/// Every doc currently in the store, as `(slug, markdown)` pairs. The store is
/// a few dozen small files at this scale, so a full list+get scan on every
/// call (list, and PUT's anchor check) is acceptable and keeps anchor state
/// index-free; revisit if doc count grows (see plan's "Out of Scope").
async fn load_all_docs(bucket: &Bucket) -> worker::Result<Vec<(String, String)>> {
    let listed = bucket.list().prefix(PREFIX).execute().await?;
    let mut out = Vec::new();
    for obj in listed.objects() {
        let Some(slug) = slug_from_key(&obj.key()) else { continue };
        if let Some(text) = get_doc_text(bucket, &doc_key(&slug)).await? {
            out.push((slug, text));
        }
    }
    Ok(out)
}

async fn get_doc_text(bucket: &Bucket, key: &str) -> worker::Result<Option<String>> {
    let Some(obj) = bucket.get(key).execute().await? else { return Ok(None) };
    let body = obj
        .body()
        .ok_or_else(|| worker::Error::RustError("R2 object body already consumed".into()))?;
    let bytes = body.bytes().await?;
    let text = String::from_utf8(bytes).map_err(|e| worker::Error::RustError(e.to_string()))?;
    Ok(Some(text))
}

// ── GET /api/l3/docs ──────────────────────────────────────────────────────────

pub async fn handle_list_docs(bucket: &Bucket) -> worker::Result<Response> {
    let listed = bucket.list().prefix(PREFIX).execute().await?;
    let mut docs = Vec::new();
    for obj in listed.objects() {
        let Some(slug) = slug_from_key(&obj.key()) else { continue };
        docs.push(serde_json::json!({
            "doc": slug,
            "size": obj.size(),
            "modified": crate::secs_to_rfc3339(obj.uploaded().as_millis() / 1000),
        }));
    }
    Response::from_json(&docs)
}

// ── GET /api/l3/docs/{slug} ───────────────────────────────────────────────────

pub async fn handle_get_doc(slug: &str, bucket: &Bucket) -> worker::Result<Response> {
    if !is_valid_slug(slug) {
        return bad_request("invalid slug");
    }
    match get_doc_text(bucket, &doc_key(slug)).await? {
        Some(text) => {
            let headers = Headers::new();
            headers.set("Content-Type", "text/markdown; charset=utf-8")?;
            Ok(Response::ok(text)?.with_headers(headers))
        }
        None => Response::error("not found", 404),
    }
}

// ── PUT /api/l3/docs/{slug} ───────────────────────────────────────────────────

pub async fn handle_put_doc(
    slug: &str,
    url: &Url,
    mut req: Request,
    bucket: &Bucket,
) -> worker::Result<Response> {
    if !is_valid_slug(slug) {
        return bad_request("invalid slug");
    }
    let force = url.query_pairs().any(|(k, v)| k == "force" && v == "1");
    let body = req.text().await?;

    let (incoming_graph, incoming_warnings) = l3::parse_sources(&[(slug.to_string(), body.clone())]);
    // "heading without anchor" is the expected, normal case a write resolves via
    // assign_ids_source below — it never blocks a write. Every other warning kind
    // (malformed flow map, missing frontmatter, ...) means a garbled edit and blocks.
    let blocking_warnings: Vec<_> =
        incoming_warnings.iter().filter(|w| w.message != "heading without anchor").collect();
    if !blocking_warnings.is_empty() && !force {
        let warnings: Vec<_> = blocking_warnings
            .iter()
            .map(|w| serde_json::json!({"doc": w.doc, "line": w.line, "message": w.message}))
            .collect();
        return Ok(Response::from_json(&serde_json::json!({"warnings": warnings}))?.with_status(400));
    }

    let existing_docs = load_all_docs(bucket).await?;
    let other_docs: Vec<(String, String)> =
        existing_docs.into_iter().filter(|(s, _)| s != slug).collect();
    let other_anchors = l3::collect_anchors(&other_docs);

    let incoming_ids: HashSet<String> =
        incoming_graph.nodes.iter().filter_map(|n| n.id.as_ref().map(|id| id.0.clone())).collect();
    let mut conflicts: Vec<&String> = incoming_ids.intersection(&other_anchors).collect();
    if !conflicts.is_empty() {
        conflicts.sort();
        return Ok(Response::from_json(&serde_json::json!({"conflicts": conflicts}))?.with_status(409));
    }

    let mut anchors = other_anchors;
    anchors.extend(incoming_ids);
    let (assigned_content, _assigned) = l3::assign_ids_source(&body, &mut anchors);

    let today = crate::today_utc_date();
    let final_content = l3::upsert_frontmatter_key(&assigned_content, "updated", &today);

    bucket
        .put(doc_key(slug), final_content.clone().into_bytes())
        .execute()
        .await?;

    let headers = Headers::new();
    headers.set("Content-Type", "text/markdown; charset=utf-8")?;
    Ok(Response::ok(final_content)?.with_headers(headers))
}

// ── GET /api/l3/graph ──────────────────────────────────────────────────────────

pub async fn handle_graph(req: &Request, bucket: &Bucket) -> worker::Result<Response> {
    let docs = load_all_docs(bucket).await?;
    let (graph, _warnings) = l3::parse_sources(&docs);
    let body = serde_json::to_vec(&graph).expect("Graph serialization is infallible");
    let etag = format!("\"{}\"", hex_digest(&body));

    let if_none_match: Option<String> = req.headers().get("If-None-Match").ok().flatten();
    if if_none_match.as_deref() == Some(etag.as_str()) {
        let headers = Headers::new();
        headers.set("ETag", &etag)?;
        return Ok(Response::empty()?.with_status(304).with_headers(headers));
    }

    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("ETag", &etag)?;
    Ok(Response::from_bytes(body)?.with_headers(headers))
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

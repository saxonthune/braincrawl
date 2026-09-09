//! `POST /api/llm/*` — OpenRouter proxy so devices never hold the LLM key.
//!
//! Native twin of `apps/worker/src/llm_proxy.rs`: same two-path allowlist
//! (so the proxy can't reach OpenRouter account/credits endpoints), same
//! 503 when no key is configured, and the upstream body is streamed back
//! untouched so SSE passes through. The client's own `Authorization`
//! header is never forwarded — only the server's key is.

use std::sync::OnceLock;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header::CONTENT_TYPE, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::AppState;

const ALLOWED_PATHS: [&str; 2] = ["v1/messages", "v1/chat/completions"];

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

pub async fn handler_llm_proxy(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !ALLOWED_PATHS.contains(&path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(key) = state.openrouter_key.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"error": "llm proxy not configured"})),
        )
            .into_response();
    };

    let mut upstream_req = http_client()
        .post(format!("https://openrouter.ai/api/{path}"))
        .bearer_auth(key)
        .header("X-Title", "braincrawl")
        .body(body);
    if let Some(ct) = headers.get(CONTENT_TYPE) {
        upstream_req = upstream_req.header(CONTENT_TYPE, ct.clone());
    }
    if let Some(av) = headers.get("anthropic-version") {
        upstream_req = upstream_req.header("anthropic-version", av.clone());
    }

    match upstream_req.send().await {
        Ok(upstream) => {
            let mut builder = Response::builder().status(upstream.status());
            if let Some(ct) = upstream.headers().get(CONTENT_TYPE) {
                builder = builder.header(CONTENT_TYPE, ct.clone());
            }
            builder
                .body(Body::from_stream(upstream.bytes_stream()))
                .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response(),
    }
}

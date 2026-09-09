//! `/api/llm/*` — OpenRouter proxy so devices never hold the LLM key.
//!
//! Allowlists exactly `v1/messages` and `v1/chat/completions` (POST only) so
//! the proxy can't reach OpenRouter account/credits endpoints. The client's
//! own `Authorization` header (its store session token) is never forwarded —
//! only the worker's `OPENROUTER_API_KEY` secret is. The upstream response is
//! returned as-is so streaming (SSE) passes through untouched.

use worker::{Env, Fetch, Headers, Method, Request, RequestInit, Response};

const ALLOWED_PATHS: [&str; 2] = ["v1/messages", "v1/chat/completions"];

pub async fn handle(mut req: Request, env: &Env, remainder: &str) -> worker::Result<Response> {
    if req.method() != Method::Post || !ALLOWED_PATHS.contains(&remainder) {
        return Response::error("not found", 404);
    }

    let api_key = match env.secret("OPENROUTER_API_KEY") {
        Ok(s) => s.to_string(),
        Err(_) => {
            return Ok(Response::from_json(
                &serde_json::json!({"error": "llm proxy not configured"}),
            )?
            .with_status(503));
        }
    };

    let content_type = req.headers().get("Content-Type")?;
    let anthropic_version = req.headers().get("anthropic-version")?;
    let body = req.bytes().await?;

    let out_headers = Headers::new();
    out_headers.set("Authorization", &format!("Bearer {api_key}"))?;
    out_headers.set("X-Title", "braincrawl")?;
    if let Some(ct) = content_type {
        out_headers.set("Content-Type", &ct)?;
    }
    if let Some(av) = anthropic_version {
        out_headers.set("anthropic-version", &av)?;
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(out_headers);
    init.with_body(Some(worker::js_sys::Uint8Array::from(body.as_slice()).into()));

    let upstream_url = format!("https://openrouter.ai/api/{remainder}");
    let out_req = Request::new_with_init(&upstream_url, &init)?;

    Fetch::Request(out_req).send().await
}

use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

pub fn run_all(base_url: &str, token: Option<&str>) -> Result<(), String> {
    // A bounded timeout turns a wedged server into a fast, legible failure
    // instead of an indefinite hang.
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    // stats must run first: it checks the empty-store zeros before other cases insert data
    stats(&client, base_url, token)?;
    put_and_get_work(&client, base_url, token)?;
    have(&client, base_url, token)?;
    content_roundtrip(&client, base_url, token)?;
    large_content(&client, base_url, token)?;
    neighborhood(&client, base_url, token)?;
    Ok(())
}

/// GET `/health` with no Authorization header — checks the worker's
/// unauthenticated liveness probe stays reachable behind the auth gate.
pub fn check_health(base_url: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let res = client
        .get(format!("{base_url}/health"))
        .send()
        .map_err(|e| format!("check_health: GET /health failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_health: expected 200, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("check_health: response parse failed: {e}"))?;
    if body["status"] != "ok" {
        return Err(format!("check_health: expected status=ok, got {:?}", body["status"]));
    }
    Ok(())
}

/// OPTIONS `/works/have` preflight with no Authorization header — checks the
/// worker answers CORS preflights before the auth gate.
pub fn check_cors_preflight(base_url: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let res = client
        .request(reqwest::Method::OPTIONS, format!("{base_url}/works/have"))
        .header("Origin", "https://example.invalid")
        .header("Access-Control-Request-Method", "POST")
        .send()
        .map_err(|e| format!("check_cors_preflight: OPTIONS request failed: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("check_cors_preflight: expected 2xx, got {}", res.status()));
    }
    let allow_origin = res.headers().get("Access-Control-Allow-Origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if allow_origin != "*" {
        return Err(format!("check_cors_preflight: expected Access-Control-Allow-Origin=*, got {allow_origin:?}"));
    }
    let allow_headers = res.headers().get("Access-Control-Allow-Headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !allow_headers.contains("Authorization") {
        return Err(format!("check_cors_preflight: expected Access-Control-Allow-Headers to contain Authorization, got {allow_headers:?}"));
    }
    Ok(())
}

/// The L3 doc routes (`/api/l3/*`), backed by R2 on the worker and the
/// filesystem on the native server — both must satisfy this check verbatim.
/// Not part of `run_all` since it needs an L3 root/bucket configured, which
/// `run_all`'s callers don't always set up.
pub fn check_l3_docs(base_url: &str, token: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let token = Some(token);

    let slug = "conformance-l3-smoke";
    let doc = "---\ndoc: conformance-l3-smoke\nschema: freeform\n---\n\n# L3 — Conformance Smoke\n\n## A heading with no anchor\n- tags: #smoke\n";

    let res = auth(client.put(format!("{base_url}/api/l3/docs/{slug}")).body(doc), token)
        .send()
        .map_err(|e| format!("check_l3_docs: PUT doc failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_docs: PUT doc expected 200, got {}", res.status()));
    }
    let normalized = res.text()
        .map_err(|e| format!("check_l3_docs: PUT doc body read failed: {e}"))?;
    if !normalized.contains(" ^r-") {
        return Err(format!("check_l3_docs: expected an assigned ` ^r-` anchor in PUT response, got: {normalized}"));
    }
    if !normalized.lines().any(|l| l.starts_with("updated:")) {
        return Err(format!("check_l3_docs: expected an `updated:` frontmatter line in PUT response, got: {normalized}"));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/docs/{slug}")), token)
        .send()
        .map_err(|e| format!("check_l3_docs: GET doc failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_docs: GET doc expected 200, got {}", res.status()));
    }
    let fetched = res.text()
        .map_err(|e| format!("check_l3_docs: GET doc body read failed: {e}"))?;
    if fetched != normalized {
        return Err("check_l3_docs: GET doc did not equal the normalized PUT response".to_string());
    }

    let res = auth(client.get(format!("{base_url}/api/l3/docs")), token)
        .send()
        .map_err(|e| format!("check_l3_docs: GET /api/l3/docs failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_docs: GET /api/l3/docs expected 200, got {}", res.status()));
    }
    let listing: Value = res.json()
        .map_err(|e| format!("check_l3_docs: GET /api/l3/docs parse failed: {e}"))?;
    let docs = listing.as_array().ok_or("check_l3_docs: /api/l3/docs did not return an array")?;
    if !docs.iter().any(|d| d["doc"] == slug) {
        return Err(format!("check_l3_docs: expected {slug:?} in /api/l3/docs listing, got {docs:?}"));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/graph")), token)
        .send()
        .map_err(|e| format!("check_l3_docs: GET /api/l3/graph failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_docs: GET /api/l3/graph expected 200, got {}", res.status()));
    }
    let etag = res.headers().get("ETag")
        .and_then(|v| v.to_str().ok())
        .ok_or("check_l3_docs: GET /api/l3/graph missing ETag header")?
        .to_string();
    let graph: Value = res.json()
        .map_err(|e| format!("check_l3_docs: GET /api/l3/graph parse failed: {e}"))?;
    let nodes = graph["nodes"].as_array().ok_or("check_l3_docs: graph missing nodes array")?;
    if nodes.is_empty() {
        return Err("check_l3_docs: expected >=1 node in graph, got 0".to_string());
    }

    let res = auth(
        client.get(format!("{base_url}/api/l3/graph")).header("If-None-Match", &etag),
        token,
    )
    .send()
    .map_err(|e| format!("check_l3_docs: GET /api/l3/graph (If-None-Match) failed: {e}"))?;
    if res.status() != 304 {
        return Err(format!("check_l3_docs: If-None-Match replay expected 304, got {}", res.status()));
    }

    let malformed = "---\ndoc: conformance-l3-malformed\n---\n\n## Node ^r-cnfrm\n- [[dangling\n";
    let res = auth(
        client.put(format!("{base_url}/api/l3/docs/conformance-l3-malformed")).body(malformed),
        token,
    )
    .send()
    .map_err(|e| format!("check_l3_docs: PUT malformed doc failed: {e}"))?;
    if res.status() != 400 {
        return Err(format!("check_l3_docs: PUT malformed doc expected 400, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("check_l3_docs: PUT malformed doc response parse failed: {e}"))?;
    if body["warnings"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
        return Err(format!("check_l3_docs: expected non-empty warnings, got {body:?}"));
    }

    Ok(())
}

/// The opaque `/api/l3/agent/*` context-file routes, backed by R2 on the
/// worker and the filesystem on the native server. Not part of `run_all`
/// since it needs an L3 root/bucket configured. See
/// `.todo-tasks/tasks/worker-l3-agent-files.md`.
pub fn check_l3_agent_files(base_url: &str, token: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let token = Some(token);

    let name = "principles";
    let body = "# Conformance smoke principles\n\nBe kind.\n";

    let res = auth(client.put(format!("{base_url}/api/l3/agent/{name}")).body(body), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: PUT failed: {e}"))?;
    if res.status() != 204 {
        return Err(format!("check_l3_agent_files: PUT expected 204, got {}", res.status()));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/agent/{name}")), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: GET failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_agent_files: GET expected 200, got {}", res.status()));
    }
    let fetched = res.text()
        .map_err(|e| format!("check_l3_agent_files: GET body read failed: {e}"))?;
    if fetched != body {
        return Err(format!(
            "check_l3_agent_files: GET body did not match PUT body, got {fetched:?}"
        ));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/agent")), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: GET list failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_agent_files: GET list expected 200, got {}", res.status()));
    }
    let listing: Value = res.json()
        .map_err(|e| format!("check_l3_agent_files: GET list parse failed: {e}"))?;
    let files = listing.as_array().ok_or("check_l3_agent_files: list did not return an array")?;
    if !files.iter().any(|f| f["name"] == name) {
        return Err(format!("check_l3_agent_files: expected {name:?} in list, got {files:?}"));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/agent/does-not-exist")), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: GET missing failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!("check_l3_agent_files: GET missing expected 404, got {}", res.status()));
    }

    let oversize = "x".repeat(64 * 1024 + 1);
    let res = auth(client.put(format!("{base_url}/api/l3/agent/{name}")).body(oversize), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: PUT oversize failed: {e}"))?;
    if res.status() != 413 {
        return Err(format!("check_l3_agent_files: PUT oversize expected 413, got {}", res.status()));
    }

    let res = auth(client.get(format!("{base_url}/api/l3/docs")), token)
        .send()
        .map_err(|e| format!("check_l3_agent_files: GET /api/l3/docs failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_agent_files: GET /api/l3/docs expected 200, got {}", res.status()));
    }
    let docs: Value = res.json()
        .map_err(|e| format!("check_l3_agent_files: GET /api/l3/docs parse failed: {e}"))?;
    let docs = docs.as_array().ok_or("check_l3_agent_files: /api/l3/docs did not return an array")?;
    if docs.iter().any(|d| d["doc"] == name) {
        return Err(format!(
            "check_l3_agent_files: agent file {name:?} must not appear in /api/l3/docs, got {docs:?}"
        ));
    }

    Ok(())
}

/// The prev-backup contract on `/api/l3/docs/{slug}` — both backends must
/// keep exactly one prior version and serve it at `?version=prev`. See
/// `.todo-tasks/tasks/worker-l3-prev-backup.md`.
pub fn check_l3_prev_backup(base_url: &str, token: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let token = Some(token);

    let slug = "conformance-l3-prev-backup";
    let doc_v1 = "---\ndoc: conformance-l3-prev-backup\nschema: freeform\n---\n\n# L3 — Prev Backup v1\n\n## A heading with no anchor\n- tags: #smoke\n";
    let doc_v2 = "---\ndoc: conformance-l3-prev-backup\nschema: freeform\n---\n\n# L3 — Prev Backup v2\n\n## A heading with no anchor\n- tags: #smoke\n";

    let res = auth(client.get(format!("{base_url}/api/l3/docs/{slug}?version=prev")), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: GET ?version=prev (never overwritten) failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!(
            "check_l3_prev_backup: GET ?version=prev on a never-overwritten doc expected 404, got {}",
            res.status()
        ));
    }

    let res = auth(client.put(format!("{base_url}/api/l3/docs/{slug}")).body(doc_v1), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: PUT v1 failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_prev_backup: PUT v1 expected 200, got {}", res.status()));
    }
    let normalized_v1 = res.text()
        .map_err(|e| format!("check_l3_prev_backup: PUT v1 body read failed: {e}"))?;

    let res = auth(client.get(format!("{base_url}/api/l3/docs/{slug}?version=prev")), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: GET ?version=prev (after v1) failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!(
            "check_l3_prev_backup: GET ?version=prev after the first PUT expected 404, got {}",
            res.status()
        ));
    }

    let res = auth(client.put(format!("{base_url}/api/l3/docs/{slug}")).body(doc_v2), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: PUT v2 failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_prev_backup: PUT v2 expected 200, got {}", res.status()));
    }
    let normalized_v2 = res.text()
        .map_err(|e| format!("check_l3_prev_backup: PUT v2 body read failed: {e}"))?;

    let res = auth(client.get(format!("{base_url}/api/l3/docs/{slug}?version=prev")), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: GET ?version=prev (after v2) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!(
            "check_l3_prev_backup: GET ?version=prev after the second PUT expected 200, got {}",
            res.status()
        ));
    }
    let prev = res.text()
        .map_err(|e| format!("check_l3_prev_backup: GET ?version=prev body read failed: {e}"))?;
    if prev != normalized_v1 {
        return Err("check_l3_prev_backup: ?version=prev did not equal the normalized v1 content".to_string());
    }

    let res = auth(client.get(format!("{base_url}/api/l3/docs/{slug}")), token)
        .send()
        .map_err(|e| format!("check_l3_prev_backup: GET (plain) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("check_l3_prev_backup: GET (plain) expected 200, got {}", res.status()));
    }
    let current = res.text()
        .map_err(|e| format!("check_l3_prev_backup: GET (plain) body read failed: {e}"))?;
    if current != normalized_v2 {
        return Err("check_l3_prev_backup: plain GET did not equal the normalized v2 content".to_string());
    }

    Ok(())
}

/// `request-code` for an arbitrary (unallowlisted) address must answer the
/// same uniform 200 as an allowlisted one, and `verify` with a bogus code
/// must 401 — exercised with no `ALLOWED_EMAILS`/`RESEND_API_KEY` secrets
/// configured, so this only proves the uniform-response branch, not delivery.
pub fn check_auth_otp_uniform(base_url: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;

    let res = client
        .post(format!("{base_url}/api/auth/request-code"))
        .json(&serde_json::json!({"email": "conformance-otp-smoke@example.invalid"}))
        .send()
        .map_err(|e| format!("check_auth_otp_uniform: POST request-code failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!(
            "check_auth_otp_uniform: POST request-code expected 200, got {}",
            res.status()
        ));
    }
    let body: Value = res.json()
        .map_err(|e| format!("check_auth_otp_uniform: request-code response parse failed: {e}"))?;
    if body["ok"] != true {
        return Err(format!("check_auth_otp_uniform: expected ok=true, got {body:?}"));
    }

    let res = client
        .post(format!("{base_url}/api/auth/verify"))
        .json(&serde_json::json!({
            "email": "conformance-otp-smoke@example.invalid",
            "code": "000000",
        }))
        .send()
        .map_err(|e| format!("check_auth_otp_uniform: POST verify failed: {e}"))?;
    if res.status() != 401 {
        return Err(format!(
            "check_auth_otp_uniform: POST verify (bogus code) expected 401, got {}",
            res.status()
        ));
    }

    Ok(())
}

/// `/api/llm/*` — the OpenRouter proxy. No `OPENROUTER_API_KEY` secret is
/// configured in the dev environment, so an authed request to an allowlisted
/// path proves the route exists, is authed, and fails closed (503) rather
/// than silently forwarding. Not part of `run_all` since it assumes the
/// dev/test worker has no `OPENROUTER_API_KEY` secret set.
pub fn check_llm_proxy(base_url: &str, token: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;

    let res = client
        .post(format!("{base_url}/api/llm/v1/chat/completions"))
        .json(&serde_json::json!({"model": "conformance/smoke", "messages": []}))
        .send()
        .map_err(|e| format!("check_llm_proxy: unauthenticated POST failed: {e}"))?;
    if res.status() != 401 {
        return Err(format!(
            "check_llm_proxy: unauthenticated POST expected 401, got {}",
            res.status()
        ));
    }

    let res = auth(
        client
            .post(format!("{base_url}/api/llm/v1/chat/completions"))
            .json(&serde_json::json!({"model": "conformance/smoke", "messages": []})),
        Some(token),
    )
    .send()
    .map_err(|e| format!("check_llm_proxy: authed POST v1/chat/completions failed: {e}"))?;
    if res.status() != 503 {
        return Err(format!(
            "check_llm_proxy: authed POST v1/chat/completions expected 503 (no secret configured), got {}",
            res.status()
        ));
    }

    let res = auth(client.post(format!("{base_url}/api/llm/v1/other")).json(&serde_json::json!({})), Some(token))
        .send()
        .map_err(|e| format!("check_llm_proxy: authed POST v1/other failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!(
            "check_llm_proxy: authed POST v1/other expected 404, got {}",
            res.status()
        ));
    }

    Ok(())
}

fn auth(req: reqwest::blocking::RequestBuilder, token: Option<&str>) -> reqwest::blocking::RequestBuilder {
    match token {
        Some(t) => req.header("Authorization", format!("Bearer {t}")),
        None => req,
    }
}

fn put_and_get_work(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let body = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"namespace": "doi", "value": "10.99/smoke"}],
        "attrs": {"title": "Smoke Test Paper"}
    });
    let res = auth(client.put(format!("{base}/works")).json(&body), token)
        .send()
        .map_err(|e| format!("put_and_get_work: PUT /works request failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("put_and_get_work: PUT /works expected 200, got {}", res.status()));
    }
    let json: Value = res.json()
        .map_err(|e| format!("put_and_get_work: PUT /works response parse failed: {e}"))?;
    if !json["id"].as_str().unwrap_or("").starts_with("guid:") {
        return Err(format!("put_and_get_work: id should start with 'guid:', got {:?}", json["id"]));
    }

    let res = auth(client.get(format!("{base}/works/doi:10.99/smoke")), token)
        .send()
        .map_err(|e| format!("put_and_get_work: GET /works/doi:10.99/smoke failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("put_and_get_work: GET existing work expected 200, got {}", res.status()));
    }

    let res = auth(client.get(format!("{base}/works/doi:10.99/does-not-exist")), token)
        .send()
        .map_err(|e| format!("put_and_get_work: GET unknown work failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!("put_and_get_work: GET unknown work expected 404, got {}", res.status()));
    }

    Ok(())
}

fn have(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let res = auth(
        client.post(format!("{base}/works/have"))
            .json(&serde_json::json!({"ids": ["doi:10.0/none"]})),
        token,
    )
    .send()
    .map_err(|e| format!("have: POST /works/have (empty) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("have: POST /works/have expected 200, got {}", res.status()));
    }
    let present: Vec<String> = res.json()
        .map_err(|e| format!("have: POST /works/have parse failed: {e}"))?;
    if !present.is_empty() {
        return Err(format!("have: expected empty present list, got {present:?}"));
    }

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.0/known"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("have: PUT work failed: {e}"))?;

    let res = auth(
        client.post(format!("{base}/works/have"))
            .json(&serde_json::json!({"ids": ["doi:10.0/known", "doi:10.0/none"]})),
        token,
    )
    .send()
    .map_err(|e| format!("have: POST /works/have (after put) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("have: POST /works/have (after put) expected 200, got {}", res.status()));
    }
    let present: Vec<String> = res.json()
        .map_err(|e| format!("have: POST /works/have (after put) parse failed: {e}"))?;
    if present != vec!["doi:10.0/known"] {
        return Err(format!("have: expected [\"doi:10.0/known\"], got {present:?}"));
    }

    Ok(())
}

fn content_roundtrip(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: PUT work failed: {e}"))?;

    let res = auth(
        client.put(format!(
            "{base}/works/doi:10.1/content/abstract?mime=text/plain&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body("hello braincrawl"),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: PUT content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("content_roundtrip: PUT content expected 200, got {}", res.status()));
    }

    let res = auth(
        client.get(format!("{base}/works/doi:10.1/content/abstract")),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: GET content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("content_roundtrip: GET content expected 200, got {}", res.status()));
    }
    let body = res.text()
        .map_err(|e| format!("content_roundtrip: GET content body read failed: {e}"))?;
    if body != "hello braincrawl" {
        return Err(format!("content_roundtrip: body mismatch: expected 'hello braincrawl', got {body:?}"));
    }

    Ok(())
}

fn large_content(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.2/large"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: PUT work failed: {e}"))?;

    let large_body: Vec<u8> = (0u8..=255).cycle().take(5 * 1024 * 1024).collect();

    let res = auth(
        client.put(format!(
            "{base}/works/doi:10.2/large/content/fulltext?mime=application/pdf&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body(large_body.clone()),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: PUT large content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("large_content: PUT large content must not 413, got {}", res.status()));
    }

    let res = auth(
        client.get(format!("{base}/works/doi:10.2/large/content/fulltext")),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: GET large content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("large_content: GET large content expected 200, got {}", res.status()));
    }
    let returned = res.bytes()
        .map_err(|e| format!("large_content: GET large content body read failed: {e}"))?;
    if returned.as_ref() != large_body.as_slice() {
        return Err("large_content: bytes did not round-trip".to_string());
    }

    Ok(())
}

fn stats(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let res = auth(client.get(format!("{base}/stats")), token)
        .send()
        .map_err(|e| format!("stats: GET /stats failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("stats: GET /stats expected 200, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("stats: GET /stats parse failed: {e}"))?;
    if body["works"] != 0 {
        return Err(format!("stats: expected works=0, got {}", body["works"]));
    }
    if body["edges_total"] != 0 {
        return Err(format!("stats: expected edges_total=0, got {}", body["edges_total"]));
    }

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "openalex",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/described"}],
                "attrs": {"title": "Described"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("stats: PUT described work failed: {e}"))?;

    auth(
        client.put(format!("{base}/edges"))
            .json(&serde_json::json!([{
                "src": {"namespace": "doi", "value": "10.1/described"},
                "dst": {"namespace": "doi", "value": "10.1/stub"},
                "relation": "cites",
                "source": "openalex",
                "attrs": null,
                "fetched_at": "2024-01-01T00:00:00Z"
            }])),
        token,
    )
    .send()
    .map_err(|e| format!("stats: PUT edge failed: {e}"))?;

    let res = auth(client.get(format!("{base}/stats")), token)
        .send()
        .map_err(|e| format!("stats: GET /stats (after) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("stats: GET /stats (after) expected 200, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("stats: GET /stats (after) parse failed: {e}"))?;

    if body["works"] != 2 {
        return Err(format!("stats: expected works=2, got {}", body["works"]));
    }
    if body["works_described"] != 1 {
        return Err(format!("stats: expected works_described=1, got {}", body["works_described"]));
    }
    if body["works_stub"] != 1 {
        return Err(format!("stats: expected works_stub=1, got {}", body["works_stub"]));
    }
    if body["nodes_total"] != 2 {
        return Err(format!("stats: expected nodes_total=2, got {}", body["nodes_total"]));
    }
    if body["tombstones"] != 0 {
        return Err(format!("stats: expected tombstones=0, got {}", body["tombstones"]));
    }
    if body["edges_total"] != 1 {
        return Err(format!("stats: expected edges_total=1, got {}", body["edges_total"]));
    }

    let kinds = body["nodes_by_kind"].as_array()
        .ok_or("stats: nodes_by_kind missing or not array")?;
    if kinds.is_empty() || kinds[0]["key"] != "work" || kinds[0]["count"] != 2 {
        return Err(format!("stats: nodes_by_kind unexpected: {kinds:?}"));
    }

    let rels = body["edges_by_relation"].as_array()
        .ok_or("stats: edges_by_relation missing or not array")?;
    if rels.is_empty() || rels[0]["key"] != "cites" || rels[0]["count"] != 1 {
        return Err(format!("stats: edges_by_relation unexpected: {rels:?}"));
    }

    let sources = body["assertions_by_source"].as_array()
        .ok_or("stats: assertions_by_source missing or not array")?;
    if sources.is_empty() || sources[0]["key"] != "openalex" || sources[0]["count"] != 1 {
        return Err(format!("stats: assertions_by_source unexpected: {sources:?}"));
    }

    Ok(())
}

fn neighborhood(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/src"}],
                "attrs": {"title": "Source"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT src work failed: {e}"))?;

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/dst"}],
                "attrs": {"title": "Dest"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT dst work failed: {e}"))?;

    auth(
        client.put(format!("{base}/edges"))
            .json(&serde_json::json!([{
                "src": {"namespace": "doi", "value": "10.1/src"},
                "dst": {"namespace": "doi", "value": "10.1/dst"},
                "relation": "cites",
                "source": "test",
                "attrs": null,
                "fetched_at": "2024-01-01T00:00:00Z"
            }])),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT edge failed: {e}"))?;

    let res = auth(
        client.post(format!("{base}/graph/neighborhood"))
            .json(&serde_json::json!({
                "seeds": ["doi:10.1/src"],
                "dir": "forward",
                "depth": 1,
                "max_nodes": 50
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: POST /graph/neighborhood failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("neighborhood: POST /graph/neighborhood expected 200, got {}", res.status()));
    }

    let body: Value = res.json()
        .map_err(|e| format!("neighborhood: POST /graph/neighborhood parse failed: {e}"))?;

    if body.get("nodes").is_none() {
        return Err("neighborhood: response missing 'nodes'".to_string());
    }
    if body.get("edges").is_none() {
        return Err("neighborhood: response missing 'edges'".to_string());
    }
    if body.get("truncated").is_none() {
        return Err("neighborhood: response missing 'truncated'".to_string());
    }

    let nodes = body["nodes"].as_array()
        .ok_or("neighborhood: nodes not an array")?;
    if nodes.len() != 2 {
        return Err(format!("neighborhood: expected 2 nodes, got {}", nodes.len()));
    }

    let edges = body["edges"].as_array()
        .ok_or("neighborhood: edges not an array")?;
    if edges.len() != 1 {
        return Err(format!("neighborhood: expected 1 edge, got {}", edges.len()));
    }

    if body["truncated"].as_bool() != Some(false) {
        return Err(format!("neighborhood: expected truncated=false, got {:?}", body["truncated"]));
    }

    let res = auth(
        client.post(format!("{base}/graph/neighborhood"))
            .json(&serde_json::json!({
                "seeds": ["doi:10.1/src"],
                "dir": "sideways",
                "depth": 1,
                "max_nodes": 50
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: POST /graph/neighborhood (bad dir) failed: {e}"))?;
    if res.status() != 400 {
        return Err(format!("neighborhood: bad dir expected 400, got {}", res.status()));
    }

    Ok(())
}

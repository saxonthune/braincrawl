use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status} from {url}: {body}")]
    Server { status: u16, url: String, body: String },
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// One entry from `GET /api/l3/docs`.
#[derive(Debug, serde::Deserialize)]
pub struct L3RemoteDoc {
    pub doc: String,
    pub size: u64,
    pub modified: String,
}

/// One entry from `GET /api/l3/agent`.
#[derive(Debug, serde::Deserialize)]
pub struct L3RemoteAgentFile {
    pub name: String,
    pub size: u64,
    pub modified: String,
}

/// A parse warning surfaced by the worker's PUT `/api/l3/docs/{slug}` 400 response.
#[derive(Debug, serde::Deserialize)]
pub struct L3Warning {
    pub doc: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum L3PutError {
    /// 400 — the incoming doc has blocking parse warnings; retry with `force` to bypass.
    #[error("doc rejected: {0:?}")]
    Warnings(Vec<L3Warning>),
    /// 409 — an anchor in the incoming doc collides with one already used elsewhere.
    #[error("anchor conflicts with another doc: {0:?}")]
    Conflicts(Vec<String>),
    #[error(transparent)]
    Client(#[from] ClientError),
}

/// CLI-local content outcome, mirroring the server's ContentOutcome over HTTP.
#[derive(Debug)]
pub enum ContentOutcome {
    Bytes { bytes: Vec<u8>, mime: String },
    Pending,
    Absent,
}

/// What the store knows about a work's artifacts. `Held(vec![])` — a known work
/// holding nothing — is a different answer from `UnknownWork`.
#[derive(Debug)]
pub enum ArtifactListing {
    Held(Vec<serde_json::Value>),
    UnknownWork,
}

/// Thin blocking HTTP client for the braincrawl metadata server.
/// Records and edges are untyped JSON; this crate has no dependency on crates/core.
pub struct StoreClient {
    base_url: String,
    http: reqwest::blocking::Client,
    token: Option<String>,
}

impl StoreClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        StoreClient {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::new(),
            token: None,
        }
    }

    /// Attach a bearer token sent with every request.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    fn apply_auth(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(t) = &self.token {
            req.bearer_auth(t)
        } else {
            req
        }
    }

    /// PUT /works — returns the assigned canonical id (`guid:…`).
    pub fn put_work(&self, record: &serde_json::Value) -> Result<String> {
        let url = format!("{}/works", self.base_url);
        let resp = self.apply_auth(self.http.put(&url)).json(record).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        let json: serde_json::Value = resp.json()?;
        Ok(json["id"].as_str().unwrap_or("").to_string())
    }

    /// PUT /edges — returns the number of edges stored.
    pub fn put_edges(&self, edges: &[serde_json::Value]) -> Result<u64> {
        let url = format!("{}/edges", self.base_url);
        let resp = self.apply_auth(self.http.put(&url)).json(edges).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        let json: serde_json::Value = resp.json()?;
        Ok(json["count"].as_u64().unwrap_or(0))
    }

    /// POST /works/have — returns the subset of `ids` that are present.
    /// Each id should be in `ns:value` form.
    pub fn have(&self, ids: &[String]) -> Result<Vec<String>> {
        let url = format!("{}/works/have", self.base_url);
        let body = serde_json::json!({ "ids": ids });
        let resp = self.apply_auth(self.http.post(&url)).json(&body).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// POST /graph/neighborhood — returns the neighborhood subgraph as JSON.
    pub fn neighborhood(
        &self,
        seeds: &[String],
        dir: &str,
        depth: u32,
        max_nodes: u32,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/graph/neighborhood", self.base_url);
        let body = serde_json::json!({
            "seeds": seeds,
            "dir": dir,
            "depth": depth,
            "max_nodes": max_nodes,
        });
        let resp = self.apply_auth(self.http.post(&url)).json(&body).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// GET /health — unauthenticated liveness probe; returns the raw JSON body.
    pub fn health(&self) -> Result<serde_json::Value> {
        let url = format!("{}/health", self.base_url);
        let resp = self.http.get(&url).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// GET /stats — returns aggregate network statistics as JSON.
    pub fn stats(&self) -> Result<serde_json::Value> {
        let url = format!("{}/stats", self.base_url);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// GET /works/{alias} — returns the work JSON, or `None` on 404.
    /// `alias` should be in `ns:value` form.
    pub fn get_work(&self, alias: &str) -> Result<Option<serde_json::Value>> {
        let url = format!("{}/works/{}", self.base_url, alias);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(Some(resp.json()?))
    }

    /// PUT /works/{alias}/content/{kind} — store a content artifact.
    ///
    /// Params are sent as query parameters per the server's handler_works_put:
    /// `mime` is required; `source`, `source_url`, `fetched_at` optional.
    pub fn put_content(
        &self,
        alias: &str,
        kind: &str,
        bytes: Vec<u8>,
        mime: &str,
        source: Option<&str>,
        source_url: Option<&str>,
    ) -> Result<()> {
        let fetched_at = rfc3339_now();
        self.put_content_with_fetched_at(alias, kind, bytes, mime, source, source_url, &fetched_at, None)
    }

    /// Same as `put_content`, but with a caller-supplied `fetched_at` instead of "now"
    /// (used by migration replay to preserve the original provenance timestamp) and an
    /// optional `derived_from` — the (role, version) this artifact was read from, e.g.
    /// `("fulltext", 1)` for a `chunks` artifact chunked out of `fulltext` v1.
    #[allow(clippy::too_many_arguments)]
    pub fn put_content_with_fetched_at(
        &self,
        alias: &str,
        kind: &str,
        bytes: Vec<u8>,
        mime: &str,
        source: Option<&str>,
        source_url: Option<&str>,
        fetched_at: &str,
        derived_from: Option<(&str, u32)>,
    ) -> Result<()> {
        let url = format!("{}/works/{}/content/{}", self.base_url, alias, kind);
        let version_str;
        let params = match derived_from {
            Some((role, version)) => {
                version_str = version.to_string();
                let mut params = build_put_content_params(mime, source, source_url, fetched_at);
                params.push(("derived_from_role", role));
                params.push(("derived_from_version", &version_str));
                params
            }
            None => build_put_content_params(mime, source, source_url, fetched_at),
        };
        let resp = self
            .apply_auth(self.http.put(&url).query(&params).body(bytes))
            .send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(())
    }

    /// GET /works/{alias}/content/{kind} — retrieve a content artifact.
    ///
    /// Maps server responses to `ContentOutcome`:
    ///   200 → Bytes, 202 → Pending, 404 → Absent.
    pub fn get_content(&self, alias: &str, kind: &str) -> Result<ContentOutcome> {
        let url = format!("{}/works/{}/content/{}", self.base_url, alias, kind);
        let resp = self
            .apply_auth(self.http.get(&url))
            .send()?;
        let status = resp.status().as_u16();
        match status {
            200 => {
                let mime = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let bytes = resp.bytes()?.to_vec();
                Ok(ContentOutcome::Bytes { bytes, mime })
            }
            202 => Ok(ContentOutcome::Pending),
            404 => Ok(ContentOutcome::Absent),
            _ => {
                let body = resp.text().unwrap_or_default();
                Err(ClientError::Server { status, url: url.clone(), body })
            }
        }
    }

    /// GET /works/{alias}/artifacts — every artifact descriptor a work holds.
    ///
    /// A 200 with an empty list and a 404 are different answers: the first says the
    /// work is in the Catalog and holds nothing, the second says the Catalog has
    /// never heard of the alias. Callers must be able to tell them apart, so the
    /// 404 becomes `ArtifactListing::UnknownWork` rather than a transport error.
    pub fn list_artifacts(
        &self,
        alias: &str,
        role: Option<&str>,
        all_versions: bool,
    ) -> Result<ArtifactListing> {
        let url = format!("{}/works/{}/artifacts", self.base_url, alias);
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(role) = role {
            params.push(("role", role.to_string()));
        }
        if all_versions {
            params.push(("all_versions", "true".to_string()));
        }
        let resp = self
            .apply_auth(self.http.get(&url).query(&params))
            .send()?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(ArtifactListing::UnknownWork);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        let body: serde_json::Value = resp.json()?;
        Ok(ArtifactListing::Held(
            body["artifacts"].as_array().cloned().unwrap_or_default(),
        ))
    }

    /// GET /api/l3/docs — every doc currently in the consolidated L3 store.
    pub fn l3_list(&self) -> Result<Vec<L3RemoteDoc>> {
        let url = format!("{}/api/l3/docs", self.base_url);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// GET /api/l3/docs/{slug} — the doc's raw markdown, or `None` on 404.
    pub fn l3_get(&self, slug: &str) -> Result<Option<String>> {
        let url = format!("{}/api/l3/docs/{}", self.base_url, slug);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(Some(resp.text()?))
    }

    /// PUT /api/l3/docs/{slug} — returns the worker's normalized markdown on success.
    pub fn l3_put(&self, slug: &str, body: &str, force: bool) -> std::result::Result<String, L3PutError> {
        let url = format!("{}/api/l3/docs/{}", self.base_url, slug);
        let mut req = self.apply_auth(self.http.put(&url)).body(body.to_string());
        if force {
            req = req.query(&[("force", "1")]);
        }
        let resp = req.send().map_err(ClientError::from)?;
        let status = resp.status();
        match status.as_u16() {
            200..=299 => Ok(resp.text().map_err(ClientError::from)?),
            400 => {
                let json: serde_json::Value = resp.json().map_err(ClientError::from)?;
                let warnings: Vec<L3Warning> = serde_json::from_value(
                    json.get("warnings").cloned().unwrap_or_default(),
                )
                .unwrap_or_default();
                Err(L3PutError::Warnings(warnings))
            }
            409 => {
                let json: serde_json::Value = resp.json().map_err(ClientError::from)?;
                let conflicts: Vec<String> = serde_json::from_value(
                    json.get("conflicts").cloned().unwrap_or_default(),
                )
                .unwrap_or_default();
                Err(L3PutError::Conflicts(conflicts))
            }
            _ => {
                let body = resp.text().unwrap_or_default();
                Err(L3PutError::Client(ClientError::Server {
                    status: status.as_u16(),
                    url: url.clone(),
                    body,
                }))
            }
        }
    }

    /// GET /api/l3/agent — every opaque agent context file currently stored.
    pub fn l3_agent_list(&self) -> Result<Vec<L3RemoteAgentFile>> {
        let url = format!("{}/api/l3/agent", self.base_url);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(resp.json()?)
    }

    /// GET /api/l3/agent/{name} — the file's raw markdown, or `None` on 404.
    pub fn l3_agent_get(&self, name: &str) -> Result<Option<String>> {
        let url = format!("{}/api/l3/agent/{}", self.base_url, name);
        let resp = self.apply_auth(self.http.get(&url)).send()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(Some(resp.text()?))
    }

    /// PUT /api/l3/agent/{name} — store the body verbatim.
    pub fn l3_agent_put(&self, name: &str, body: &str) -> Result<()> {
        let url = format!("{}/api/l3/agent/{}", self.base_url, name);
        let resp = self.apply_auth(self.http.put(&url)).body(body.to_string()).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), url: url.clone(), body });
        }
        Ok(())
    }
}

/// Build the query-param list for PUT /works/.../content/{kind}.
/// Extracted as a pure function so it can be unit-tested without live HTTP.
pub(crate) fn build_put_content_params<'a>(
    mime: &'a str,
    source: Option<&'a str>,
    source_url: Option<&'a str>,
    fetched_at: &'a str,
) -> Vec<(&'a str, &'a str)> {
    let mut params = vec![("mime", mime), ("fetched_at", fetched_at)];
    if let Some(s) = source {
        params.push(("source", s));
    }
    if let Some(su) = source_url {
        params.push(("source_url", su));
    }
    params
}

pub fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let dims: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dim in &dims {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_params_required_fields() {
        let params = build_put_content_params("application/pdf", None, None, "2024-06-01T00:00:00Z");
        assert!(params.iter().any(|&(k, v)| k == "mime" && v == "application/pdf"));
        assert!(params.iter().any(|&(k, v)| k == "fetched_at" && v == "2024-06-01T00:00:00Z"));
        assert!(!params.iter().any(|&(k, _)| k == "source"));
        assert!(!params.iter().any(|&(k, _)| k == "source_url"));
    }

    #[test]
    fn put_params_optional_fields_present() {
        let params = build_put_content_params(
            "application/pdf",
            Some("openalex-oa"),
            Some("https://example.com/paper.pdf"),
            "2024-06-01T00:00:00Z",
        );
        assert!(params.iter().any(|&(k, v)| k == "source" && v == "openalex-oa"));
        assert!(params.iter().any(|&(k, v)| k == "source_url" && v == "https://example.com/paper.pdf"));
    }
}

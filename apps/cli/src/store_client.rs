use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Server { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// CLI-local content outcome, mirroring the server's ContentOutcome over HTTP.
#[derive(Debug)]
pub enum ContentOutcome {
    Bytes { bytes: Vec<u8>, mime: String },
    /// LinkOnly: server returned a redirect; value is the Location URL.
    Redirect(String),
    Pending,
    Restricted,
    Absent,
}

/// Thin blocking HTTP client for the braincrawl metadata server.
/// Records and edges are untyped JSON; this crate has no dependency on crates/core.
pub struct StoreClient {
    base_url: String,
    http: reqwest::blocking::Client,
    /// Separate client that does not follow redirects, used for get_content so
    /// a LinkOnly 3xx response is observable rather than transparently followed.
    http_no_redirect: reqwest::blocking::Client,
    token: Option<String>,
}

impl StoreClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http_no_redirect = reqwest::blocking::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build no-redirect HTTP client");
        StoreClient {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::new(),
            http_no_redirect,
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
            return Err(ClientError::Server { status: status.as_u16(), body });
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
            return Err(ClientError::Server { status: status.as_u16(), body });
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
            return Err(ClientError::Server { status: status.as_u16(), body });
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
            return Err(ClientError::Server { status: status.as_u16(), body });
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
            return Err(ClientError::Server { status: status.as_u16(), body });
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
            return Err(ClientError::Server { status: status.as_u16(), body });
        }
        Ok(Some(resp.json()?))
    }

    /// PUT /works/{alias}/content/{kind} — store a content payload.
    ///
    /// Params are sent as query parameters per the server's handler_works_put:
    /// `mime` and `rights` are required; `source`, `source_url`, `fetched_at` optional.
    /// Accepted `rights` values: `"open"`, `"link_only"`, `"restricted"`.
    pub fn put_content(
        &self,
        alias: &str,
        kind: &str,
        bytes: Vec<u8>,
        mime: &str,
        rights: &str,
        source: Option<&str>,
        source_url: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/works/{}/content/{}", self.base_url, alias, kind);
        let fetched_at = rfc3339_now();
        let params = build_put_content_params(mime, rights, source, source_url, &fetched_at);
        let resp = self
            .apply_auth(self.http.put(&url).query(&params).body(bytes))
            .send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ClientError::Server { status: status.as_u16(), body });
        }
        Ok(())
    }

    /// GET /works/{alias}/content/{kind} — retrieve a content payload.
    ///
    /// Uses a non-redirect-following client so a `LinkOnly` 3xx is observable.
    /// Maps server responses to `ContentOutcome`:
    ///   200 → Bytes, 3xx+Location → Redirect, 202 → Pending, 451 → Restricted, 404 → Absent.
    pub fn get_content(&self, alias: &str, kind: &str) -> Result<ContentOutcome> {
        let url = format!("{}/works/{}/content/{}", self.base_url, alias, kind);
        let resp = self
            .apply_auth(self.http_no_redirect.get(&url))
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
            451 => Ok(ContentOutcome::Restricted),
            301 | 302 | 303 | 307 | 308 => {
                let location = resp
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                Ok(ContentOutcome::Redirect(location))
            }
            _ => {
                let body = resp.text().unwrap_or_default();
                Err(ClientError::Server { status, body })
            }
        }
    }
}

/// Build the query-param list for PUT /works/.../content/{kind}.
/// Extracted as a pure function so it can be unit-tested without live HTTP.
pub(crate) fn build_put_content_params<'a>(
    mime: &'a str,
    rights: &'a str,
    source: Option<&'a str>,
    source_url: Option<&'a str>,
    fetched_at: &'a str,
) -> Vec<(&'a str, &'a str)> {
    let mut params = vec![("mime", mime), ("rights", rights), ("fetched_at", fetched_at)];
    if let Some(s) = source {
        params.push(("source", s));
    }
    if let Some(su) = source_url {
        params.push(("source_url", su));
    }
    params
}

fn rfc3339_now() -> String {
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
        let params = build_put_content_params("application/pdf", "open", None, None, "2024-06-01T00:00:00Z");
        assert!(params.iter().any(|&(k, v)| k == "mime" && v == "application/pdf"));
        assert!(params.iter().any(|&(k, v)| k == "rights" && v == "open"));
        assert!(params.iter().any(|&(k, v)| k == "fetched_at" && v == "2024-06-01T00:00:00Z"));
        assert!(!params.iter().any(|&(k, _)| k == "source"));
        assert!(!params.iter().any(|&(k, _)| k == "source_url"));
    }

    #[test]
    fn put_params_optional_fields_present() {
        let params = build_put_content_params(
            "application/pdf",
            "open",
            Some("openalex-oa"),
            Some("https://example.com/paper.pdf"),
            "2024-06-01T00:00:00Z",
        );
        assert!(params.iter().any(|&(k, v)| k == "source" && v == "openalex-oa"));
        assert!(params.iter().any(|&(k, v)| k == "source_url" && v == "https://example.com/paper.pdf"));
    }

    #[test]
    fn put_params_link_only_rights() {
        let params = build_put_content_params("text/html", "link_only", Some("openalex-oa"), Some("https://example.com/landing"), "2024-06-01T00:00:00Z");
        assert!(params.iter().any(|&(k, v)| k == "rights" && v == "link_only"));
        assert!(params.iter().any(|&(k, v)| k == "source_url" && v == "https://example.com/landing"));
    }
}

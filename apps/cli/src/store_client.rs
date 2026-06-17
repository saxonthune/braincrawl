use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Server { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, ClientError>;

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
}

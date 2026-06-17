use super::{Result, SemanticScholarError};

const DEFAULT_BASE_URL: &str = "https://api.semanticscholar.org/graph/v1";
const MAX_RETRIES: u32 = 3;

pub struct SemanticScholarClient {
    pub base_url: String,
    api_key: Option<String>,
    http: reqwest::blocking::Client,
}

impl SemanticScholarClient {
    pub fn new(api_key: Option<String>) -> Self {
        SemanticScholarClient {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key,
            http: reqwest::blocking::Client::new(),
        }
    }

    /// Override the base URL — used in tests to point at a fixture server.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// GET /paper/{path_id}?fields=…
    pub fn get_paper(&self, path_id: &str, fields: &str) -> Result<(serde_json::Value, String)> {
        let url = format!("{}/paper/{}?fields={}", self.base_url, path_id, fields);
        let built_url = url.clone();
        let value = self.send_with_retry(|| self.apply_key(self.http.get(&url)))?;
        Ok((value, built_url))
    }

    /// GET /author/{path_id}?fields=…
    pub fn get_author(&self, path_id: &str, fields: &str) -> Result<(serde_json::Value, String)> {
        let url = format!("{}/author/{}?fields={}", self.base_url, path_id, fields);
        let built_url = url.clone();
        let value = self.send_with_retry(|| self.apply_key(self.http.get(&url)))?;
        Ok((value, built_url))
    }

    /// GET /paper/search?query=…
    /// Response: `{total, offset, next, data:[…]}`
    pub fn search_papers(
        &self,
        query: &str,
        fields: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let url = self.build_url("paper/search", &[
            ("query", query),
            ("fields", fields),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]);
        self.send_with_retry(|| self.apply_key(self.http.get(&url)))
    }

    /// GET /author/search?query=…
    /// Response: `{total, offset, next, data:[…]}`
    pub fn search_authors(
        &self,
        query: &str,
        fields: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let url = self.build_url("author/search", &[
            ("query", query),
            ("fields", fields),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]);
        self.send_with_retry(|| self.apply_key(self.http.get(&url)))
    }

    /// GET /paper/{id}/citations
    /// Response: `{offset, next, data:[{citingPaper:{…}}]}`
    pub fn citations(
        &self,
        path_id: &str,
        fields: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let url = self.build_url(&format!("paper/{path_id}/citations"), &[
            ("fields", fields),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]);
        self.send_with_retry(|| self.apply_key(self.http.get(&url)))
    }

    /// GET /paper/{id}/references
    /// Response: `{offset, next, data:[{citedPaper:{…}}]}`
    pub fn references(
        &self,
        path_id: &str,
        fields: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let url = self.build_url(&format!("paper/{path_id}/references"), &[
            ("fields", fields),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]);
        self.send_with_retry(|| self.apply_key(self.http.get(&url)))
    }

    fn apply_key(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(key) = &self.api_key {
            req.header("x-api-key", key)
        } else {
            req
        }
    }

    fn build_url(&self, endpoint: &str, params: &[(&str, &str)]) -> String {
        let mut url = reqwest::Url::parse(&format!("{}/{}", self.base_url, endpoint))
            .expect("valid base URL");
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in params {
                pairs.append_pair(k, v);
            }
        }
        url.to_string()
    }

    fn send_with_retry<F>(&self, build: F) -> Result<serde_json::Value>
    where
        F: Fn() -> reqwest::blocking::RequestBuilder,
    {
        let mut last_err: Option<SemanticScholarError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let ms = 1_000u64 << (attempt - 1); // 1s, 2s, 4s
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let resp = match build().send() {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(SemanticScholarError::Http(e));
                    continue;
                }
            };
            let status = resp.status();
            if status.as_u16() == 429 || status.is_server_error() {
                let body = resp.text().unwrap_or_default();
                last_err = Some(SemanticScholarError::Api { status: status.as_u16(), body });
                continue;
            }
            if !status.is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(SemanticScholarError::Api { status: status.as_u16(), body });
            }
            return Ok(resp.json()?);
        }
        Err(last_err.unwrap_or(SemanticScholarError::Api {
            status: 0,
            body: "max retries exceeded".into(),
        }))
    }
}

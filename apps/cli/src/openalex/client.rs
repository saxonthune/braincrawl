use super::{OpenAlexError, Result};
use super::entity::Entity;

const DEFAULT_BASE_URL: &str = "https://api.openalex.org";
const MAX_RETRIES: u32 = 3;

pub struct OpenAlexClient {
    pub base_url: String,
    api_key: Option<String>,
    http: reqwest::blocking::Client,
}

pub struct ListPage {
    pub count: u64,
    pub next_cursor: Option<String>,
    pub results: Vec<serde_json::Value>,
    pub url: String,
}

#[derive(Default)]
pub struct ListParams<'a> {
    pub filter: Option<&'a str>,
    pub search: Option<&'a str>,
    pub sort: Option<&'a str>,
    pub select: Option<&'a str>,
    pub per_page: Option<u32>,
    pub cursor: Option<&'a str>,
}

impl OpenAlexClient {
    pub fn new(api_key: Option<String>) -> Self {
        OpenAlexClient {
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

    /// Fetch a single entity. `id` is passed directly in the URL path.
    pub fn get_one(
        &self,
        entity: Entity,
        id: &str,
        select: Option<&str>,
    ) -> Result<(serde_json::Value, String)> {
        let mut url = format!("{}/{}/{}", self.base_url, entity.path_segment(), id);
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(sel) = select {
            params.push(("select", sel.to_string()));
        }
        if let Some(key) = &self.api_key {
            params.push(("api_key", key.clone()));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(
                &params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        let built_url = url.clone();
        let value = self.send_with_retry(|| self.http.get(&url))?;
        Ok((value, built_url))
    }

    /// Fetch one page of list results.
    pub fn list(&self, entity: Entity, params: ListParams<'_>) -> Result<ListPage> {
        let base = format!("{}/{}", self.base_url, entity.path_segment());
        let mut qp: Vec<(&str, String)> = Vec::new();
        if let Some(f) = params.filter {
            qp.push(("filter", f.to_string()));
        }
        if let Some(s) = params.search {
            qp.push(("search", s.to_string()));
        }
        if let Some(s) = params.sort {
            qp.push(("sort", s.to_string()));
        }
        if let Some(s) = params.select {
            qp.push(("select", s.to_string()));
        }
        if let Some(n) = params.per_page {
            qp.push(("per_page", n.to_string()));
        }
        let cursor = params.cursor.unwrap_or("*");
        qp.push(("cursor", cursor.to_string()));
        if let Some(key) = &self.api_key {
            qp.push(("api_key", key.clone()));
        }
        let qs = qp
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{base}?{qs}");
        let built_url = url.clone();
        let raw = self.send_with_retry(|| self.http.get(&url))?;
        let count = raw
            .get("meta")
            .and_then(|m| m.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let next_cursor = raw
            .get("meta")
            .and_then(|m| m.get("next_cursor"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let results = raw
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(ListPage { count, next_cursor, results, url: built_url })
    }

    /// Autocomplete query against an entity collection.
    pub fn autocomplete(
        &self,
        entity: Entity,
        q: &str,
    ) -> Result<(serde_json::Value, String)> {
        let mut qp = vec![format!("q={q}")];
        if let Some(key) = &self.api_key {
            qp.push(format!("api_key={key}"));
        }
        let url = format!(
            "{}/autocomplete/{}?{}",
            self.base_url,
            entity.path_segment(),
            qp.join("&")
        );
        let built_url = url.clone();
        let value = self.send_with_retry(|| self.http.get(&url))?;
        Ok((value, built_url))
    }

    fn send_with_retry<F>(&self, build: F) -> Result<serde_json::Value>
    where
        F: Fn() -> reqwest::blocking::RequestBuilder,
    {
        let mut last_err: Option<OpenAlexError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let ms = 1_000u64 << (attempt - 1); // 1s, 2s, 4s
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let resp = match build().send() {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(OpenAlexError::Http(e));
                    continue;
                }
            };
            let status = resp.status();
            if status.as_u16() == 429 || status.is_server_error() {
                let body = resp.text().unwrap_or_default();
                last_err = Some(OpenAlexError::Api { status: status.as_u16(), body });
                continue;
            }
            if !status.is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(OpenAlexError::Api { status: status.as_u16(), body });
            }
            return Ok(resp.json()?);
        }
        Err(last_err.unwrap_or(OpenAlexError::Api {
            status: 0,
            body: "max retries exceeded".into(),
        }))
    }
}

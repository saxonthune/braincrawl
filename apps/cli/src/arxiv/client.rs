use super::{ArxivError, Result};

const DEFAULT_BASE_URL: &str = "http://export.arxiv.org/api/query";
const MAX_RETRIES: u32 = 3;

pub struct ArxivClient {
    pub base_url: String,
    http: reqwest::blocking::Client,
}

impl Default for ArxivClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ArxivClient {
    pub fn new() -> Self {
        ArxivClient {
            base_url: DEFAULT_BASE_URL.to_string(),
            http: reqwest::blocking::Client::builder()
                .user_agent(concat!("braincrawl/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// Search arXiv: returns raw Atom XML body.
    pub fn search(&self, search_query: &str, start: u32, max_results: u32) -> Result<String> {
        let url = build_url(&self.base_url, &[
            ("search_query", search_query),
            ("start", &start.to_string()),
            ("max_results", &max_results.to_string()),
        ]);
        self.send_with_retry(&url)
    }

    /// Fetch by id_list: returns raw Atom XML body.
    pub fn get(&self, id_list: &str) -> Result<String> {
        let url = build_url(&self.base_url, &[("id_list", id_list)]);
        self.send_with_retry(&url)
    }

    fn send_with_retry(&self, url: &str) -> Result<String> {
        let mut last_err: Option<ArxivError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let ms = 1_000u64 << (attempt - 1);
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let resp = match self.http.get(url).send() {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(ArxivError::Http(e));
                    continue;
                }
            };
            let status = resp.status();
            if status.as_u16() == 429 || status.is_server_error() {
                let body = resp.text().unwrap_or_default();
                last_err = Some(ArxivError::Api { status: status.as_u16(), body });
                continue;
            }
            if !status.is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(ArxivError::Api { status: status.as_u16(), body });
            }
            return Ok(resp.text()?);
        }
        Err(last_err.unwrap_or(ArxivError::Api {
            status: 0,
            body: "max retries exceeded".into(),
        }))
    }
}

fn build_url(base: &str, params: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse(base).expect("valid base URL");
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in params {
            pairs.append_pair(k, v);
        }
    }
    url.to_string()
}

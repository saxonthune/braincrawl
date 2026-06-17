use super::{RefsBackfillError, Result};

const DEFAULT_BASE_URL: &str = "https://opencitations.net";
const MAX_RETRIES: u32 = 3;

pub struct OpenCitationsClient {
    pub base_url: String,
    http: reqwest::blocking::Client,
}

impl OpenCitationsClient {
    pub fn new() -> Self {
        OpenCitationsClient {
            base_url: DEFAULT_BASE_URL.to_string(),
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// GET /index/coci/api/v1/references/{doi}
    /// Returns cited DOIs (lowercase). No API key required.
    pub fn get_references(&self, doi: &str) -> Result<Vec<String>> {
        let url = format!("{}/index/coci/api/v1/references/{}", self.base_url, doi);
        let value = self.send_with_retry(|| self.http.get(&url))?;
        Ok(parse_opencitations_references(&value))
    }

    fn send_with_retry<F>(&self, build: F) -> Result<serde_json::Value>
    where
        F: Fn() -> reqwest::blocking::RequestBuilder,
    {
        let mut last_err: Option<RefsBackfillError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let ms = 1_000u64 << (attempt - 1);
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let resp = match build().send() {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(RefsBackfillError::Http(e));
                    continue;
                }
            };
            let status = resp.status();
            if status.as_u16() == 429 || status.is_server_error() {
                let body = resp.text().unwrap_or_default();
                last_err = Some(RefsBackfillError::Api { status: status.as_u16(), body });
                continue;
            }
            if !status.is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(RefsBackfillError::Api { status: status.as_u16(), body });
            }
            return Ok(resp.json()?);
        }
        Err(last_err.unwrap_or(RefsBackfillError::Api {
            status: 0,
            body: "max retries exceeded".into(),
        }))
    }
}

/// Parse an OpenCitations COCI references response — pure function for offline testability.
/// Each array element has a `cited` field containing the cited DOI.
/// Returns cited DOIs normalized to lowercase.
pub fn parse_opencitations_references(value: &serde_json::Value) -> Vec<String> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| entry.get("cited").and_then(|c| c.as_str()))
        .map(|doi| doi.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opencitations_dois_lowercased() {
        let response = serde_json::json!([
            {"cited": "10.1000/ABC", "citing": "10.9999/source", "creation": "2020"},
            {"cited": "10.2000/DEF", "citing": "10.9999/source"},
        ]);
        let dois = parse_opencitations_references(&response);
        assert_eq!(dois, vec!["10.1000/abc", "10.2000/def"]);
    }

    #[test]
    fn parse_opencitations_already_lowercase() {
        let response = serde_json::json!([
            {"cited": "10.1000/abc"},
        ]);
        let dois = parse_opencitations_references(&response);
        assert_eq!(dois, vec!["10.1000/abc"]);
    }

    #[test]
    fn parse_opencitations_empty_array() {
        let response = serde_json::json!([]);
        let dois = parse_opencitations_references(&response);
        assert!(dois.is_empty());
    }

    #[test]
    fn parse_opencitations_not_an_array() {
        let response = serde_json::json!({});
        let dois = parse_opencitations_references(&response);
        assert!(dois.is_empty());
    }

    #[test]
    fn parse_opencitations_entry_missing_cited_skipped() {
        let response = serde_json::json!([
            {"cited": "10.1000/abc"},
            {"citing": "10.9999/source"},
            {"cited": "10.2000/def"},
        ]);
        let dois = parse_opencitations_references(&response);
        assert_eq!(dois, vec!["10.1000/abc", "10.2000/def"]);
    }
}

use super::{RefsBackfillError, Result};

const DEFAULT_BASE_URL: &str = "https://api.crossref.org";
const MAX_RETRIES: u32 = 3;

pub struct CrossrefClient {
    pub base_url: String,
    mailto: Option<String>,
    http: reqwest::blocking::Client,
}

impl CrossrefClient {
    pub fn new(mailto: Option<String>) -> Self {
        CrossrefClient {
            base_url: DEFAULT_BASE_URL.to_string(),
            mailto,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// GET /works/{doi}?mailto={email}
    /// Returns (cited_dois_lowercase, skipped_count).
    /// DOI slashes are kept as-is in the path — Crossref handles them.
    pub fn get_references(&self, doi: &str) -> Result<(Vec<String>, usize)> {
        let base = format!("{}/works/{}", self.base_url, doi);
        let url = if let Some(m) = &self.mailto {
            format!("{}?mailto={}", base, m)
        } else {
            base
        };
        let ua = self.user_agent();
        let value = self.send_with_retry(|| {
            self.http
                .get(&url)
                .header(reqwest::header::USER_AGENT, &ua)
        })?;
        Ok(parse_crossref_references(&value))
    }

    fn user_agent(&self) -> String {
        if let Some(m) = &self.mailto {
            format!("braincrawl/{} (mailto:{})", env!("CARGO_PKG_VERSION"), m)
        } else {
            concat!("braincrawl/", env!("CARGO_PKG_VERSION")).to_string()
        }
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

/// Parse a Crossref works response — pure function for offline testability.
/// Returns (cited_dois_lowercase, skipped_count) where skipped entries lack a DOI field.
pub fn parse_crossref_references(value: &serde_json::Value) -> (Vec<String>, usize) {
    let Some(refs) = value
        .get("message")
        .and_then(|m| m.get("reference"))
        .and_then(|r| r.as_array())
    else {
        return (Vec::new(), 0);
    };

    let mut cited_dois: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for entry in refs {
        match entry.get("DOI").and_then(|d| d.as_str()) {
            Some(doi) => cited_dois.push(doi.to_lowercase()),
            None => skipped += 1,
        }
    }
    (cited_dois, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_crossref_dois_and_skipped_count() {
        let response = serde_json::json!({
            "message": {
                "reference": [
                    {"DOI": "10.1000/FIRST", "author": "Smith"},
                    {"unstructured": "some unresolved reference"},
                    {"DOI": "10.2000/second"},
                    {"key": "no-doi-entry"},
                ]
            }
        });
        let (dois, skipped) = parse_crossref_references(&response);
        assert_eq!(dois, vec!["10.1000/first", "10.2000/second"]);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn parse_crossref_doi_normalized_to_lowercase() {
        let response = serde_json::json!({
            "message": {
                "reference": [
                    {"DOI": "10.1000/ABC-XYZ"},
                ]
            }
        });
        let (dois, skipped) = parse_crossref_references(&response);
        assert_eq!(dois, vec!["10.1000/abc-xyz"]);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn parse_crossref_empty_reference_list() {
        let response = serde_json::json!({
            "message": {"reference": []}
        });
        let (dois, skipped) = parse_crossref_references(&response);
        assert!(dois.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn parse_crossref_no_reference_field() {
        let response = serde_json::json!({
            "message": {"title": ["Some paper"]}
        });
        let (dois, skipped) = parse_crossref_references(&response);
        assert!(dois.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn parse_crossref_all_skipped() {
        let response = serde_json::json!({
            "message": {
                "reference": [
                    {"unstructured": "Smith 1990"},
                    {"unstructured": "Jones 2000"},
                ]
            }
        });
        let (dois, skipped) = parse_crossref_references(&response);
        assert!(dois.is_empty());
        assert_eq!(skipped, 2);
    }
}

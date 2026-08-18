//! `FetchHandler` that acquires fulltext artifacts and stores them.
//!
//! Ports the decision flow from `apps/cli/src/fetch_content.rs`:
//! resolve URL → download → put_content.  The CLI provider is left untouched.

use std::sync::Arc;

use async_trait::async_trait;
use braincrawl_core::{
    traits::FetchHandler,
    types::{Alias, ContentOutcome, DomainError, Job, JobKind, ArtifactRole},
};

use crate::LocalStore;

const MAX_BYTES: usize = 50 * 1024 * 1024;

pub struct FulltextHandler {
    pub store: Arc<LocalStore>,
    pub unpaywall_email: Option<String>,
}

#[async_trait(?Send)]
impl FetchHandler for FulltextHandler {
    fn kind(&self) -> JobKind {
        JobKind::Fulltext
    }

    async fn handle(&self, job: &Job) -> Result<(), DomainError> {
        let alias = parse_alias(&job.target_id).ok_or_else(|| {
            DomainError::Backend(format!("invalid target_id: {}", job.target_id))
        })?;

        // Idempotency: skip if bytes artifact already present.
        if let ContentOutcome::Bytes { .. } = self
            .store
            .get_content(alias.clone(), ArtifactRole::Fulltext)
            .await?
        {
            return Ok(());
        }

        let work = self
            .store
            .get_work(alias.clone())
            .await?
            .ok_or(DomainError::NotFound)?;

        let attrs = &work.attrs;

        // Resolve download URL from stored OpenAlex attrs.
        let artifact_url = resolve_oa_url(attrs)
            .or_else(|| {
                // Params may carry an explicit URL.
                job.params["url"].as_str().map(|s| s.to_string())
            });

        let Some(url) = artifact_url else {
            // Try Unpaywall if a DOI alias is available.
            let doi = work
                .aliases
                .iter()
                .find(|a| a.scheme == "doi")
                .map(|a| a.value.clone());

            if let (Some(doi), Some(email)) = (doi, &self.unpaywall_email) {
                if let Some(url) = query_unpaywall(&doi, email).await? {
                    return self.fetch_and_store(alias, &url, "unpaywall").await;
                }
            }
            // No downloadable URL found — nothing to do for now.
            return Ok(());
        };

        self.fetch_and_store(alias, &url, "openalex-oa").await
    }
}

impl FulltextHandler {
    async fn fetch_and_store(
        &self,
        alias: Alias,
        url: &str,
        source: &str,
    ) -> Result<(), DomainError> {
        let (bytes, mime) = download(url).await?;

        let now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format_rfc3339(s)
        };

        self.store
            .put_content(
                alias,
                ArtifactRole::Fulltext,
                bytes,
                mime,
                Some(source.to_string()),
                Some(url.to_string()),
                now,
                None,
            )
            .await?;

        Ok(())
    }
}

fn resolve_oa_url(attrs: &serde_json::Value) -> Option<String> {
    for ptr in [
        &attrs["primary_location"]["pdf_url"],
        &attrs["open_access"]["oa_url"],
        &attrs["best_oa_location"]["pdf_url"],
    ] {
        if let Some(s) = ptr.as_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

async fn query_unpaywall(
    doi: &str,
    email: &str,
) -> Result<Option<String>, DomainError> {
    let url = format!("https://api.unpaywall.org/v2/{}?email={}", doi, email);
    let client = build_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    for ptr in [
        &json["best_oa_location"]["url_for_pdf"],
        &json["best_oa_location"]["url"],
    ] {
        if let Some(s) = ptr.as_str() {
            if !s.is_empty() {
                return Ok(Some(s.to_string()));
            }
        }
    }
    Ok(None)
}

async fn download(url: &str) -> Result<(Vec<u8>, String), DomainError> {
    let client = build_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(DomainError::Backend(format!(
            "HTTP {} downloading {}",
            resp.status(),
            url
        )));
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let body = resp
        .bytes()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    if body.len() > MAX_BYTES {
        return Err(DomainError::Backend(format!(
            "artifact too large: {} bytes",
            body.len()
        )));
    }
    Ok((body.to_vec(), mime))
}

fn build_client() -> Result<reqwest::Client, DomainError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(concat!("braincrawl-server/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| DomainError::Backend(e.to_string()))
}

fn parse_alias(s: &str) -> Option<Alias> {
    let pos = s.find(':')?;
    Some(Alias {
        scheme: s[..pos].to_string(),
        value: s[pos + 1..].to_string(),
    })
}

fn format_rfc3339(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3_600) % 24;
    let days = secs / 86_400;
    let (year, month, day) = ft_days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn ft_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let dy = if ft_is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u64; 12] = if ft_is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dm in &months {
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    (year, month, days + 1)
}

fn ft_is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

//! `FetchHandler` that backfills outgoing references via Crossref + OpenCitations.
//!
//! Ports the DOI → cited-DOIs → edges flow from
//! `apps/cli/src/refs_backfill/{crossref,opencitations,mapping}.rs`.
//! The CLI providers are left untouched.

use std::sync::Arc;

use async_trait::async_trait;
use braincrawl_core::{
    traits::FetchHandler,
    types::{Alias, DomainError, EdgeInput, Job, JobKind},
};

use crate::LocalStore;

pub struct RefsHandler {
    pub store: Arc<LocalStore>,
    pub crossref_mailto: Option<String>,
}

#[async_trait(?Send)]
impl FetchHandler for RefsHandler {
    fn kind(&self) -> JobKind {
        JobKind::Refs
    }

    async fn handle(&self, job: &Job) -> Result<(), DomainError> {
        let alias = parse_alias(&job.target_id).ok_or_else(|| {
            DomainError::Backend(format!("invalid target_id: {}", job.target_id))
        })?;
        if alias.namespace != "doi" {
            return Err(DomainError::Backend(format!(
                "RefsHandler requires a doi: target_id, got {}",
                job.target_id
            )));
        }
        let doi = &alias.value;

        let now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format_rfc3339(s)
        };

        let client = build_client()?;
        let mut all_cited: Vec<String> = Vec::new();

        // Crossref
        let crossref_dois = fetch_crossref(&client, doi, self.crossref_mailto.as_deref()).await;
        match crossref_dois {
            Ok(dois) => all_cited.extend(dois),
            Err(e) => {
                // Non-fatal: log and continue to OpenCitations.
                eprintln!("RefsHandler: crossref error for {doi}: {e}");
            }
        }

        // OpenCitations
        let oc_dois = fetch_opencitations(&client, doi).await;
        match oc_dois {
            Ok(dois) => {
                for d in dois {
                    if !all_cited.contains(&d) {
                        all_cited.push(d);
                    }
                }
            }
            Err(e) => {
                eprintln!("RefsHandler: opencitations error for {doi}: {e}");
            }
        }

        if all_cited.is_empty() {
            return Ok(());
        }

        let edges: Vec<EdgeInput> = all_cited
            .iter()
            .map(|cited| EdgeInput {
                src: Alias { namespace: "doi".to_string(), value: doi.clone() },
                dst: Alias { namespace: "doi".to_string(), value: cited.clone() },
                relation: "cites".to_string(),
                source: "crossref+opencitations".to_string(),
                attrs: None,
                fetched_at: now.clone(),
            })
            .collect();

        self.store.put_edges(edges).await?;
        Ok(())
    }
}

async fn fetch_crossref(
    client: &reqwest::Client,
    doi: &str,
    mailto: Option<&str>,
) -> Result<Vec<String>, DomainError> {
    let base = format!("https://api.crossref.org/works/{}", doi);
    let url = if let Some(m) = mailto {
        format!("{base}?mailto={m}")
    } else {
        base
    };
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(DomainError::Backend(format!(
            "crossref HTTP {}",
            resp.status()
        )));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    Ok(parse_crossref_references(&json))
}

async fn fetch_opencitations(
    client: &reqwest::Client,
    doi: &str,
) -> Result<Vec<String>, DomainError> {
    let url = format!(
        "https://opencitations.net/index/coci/api/v1/references/{}",
        doi
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(DomainError::Backend(format!(
            "opencitations HTTP {}",
            resp.status()
        )));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| DomainError::Backend(e.to_string()))?;
    Ok(parse_opencitations_references(&json))
}

fn parse_crossref_references(value: &serde_json::Value) -> Vec<String> {
    let Some(refs) = value
        .get("message")
        .and_then(|m| m.get("reference"))
        .and_then(|r| r.as_array())
    else {
        return Vec::new();
    };
    refs.iter()
        .filter_map(|e| e.get("DOI").and_then(|d| d.as_str()))
        .map(|doi| doi.to_lowercase())
        .collect()
}

fn parse_opencitations_references(value: &serde_json::Value) -> Vec<String> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| e.get("cited").and_then(|c| c.as_str()))
        .map(|doi| doi.to_lowercase())
        .collect()
}

fn build_client() -> Result<reqwest::Client, DomainError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("braincrawl-server/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| DomainError::Backend(e.to_string()))
}

fn parse_alias(s: &str) -> Option<Alias> {
    let pos = s.find(':')?;
    Some(Alias {
        namespace: s[..pos].to_string(),
        value: s[pos + 1..].to_string(),
    })
}

fn format_rfc3339(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3_600) % 24;
    let days = secs / 86_400;
    let (year, month, day) = rf_days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn rf_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let dy = if rf_is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u64; 12] = if rf_is_leap(year) {
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

fn rf_is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

use crate::store_client::{ContentOutcome, StoreClient};

pub enum Source {
    Auto,
    Openalex,
    Unpaywall,
}

#[derive(Debug)]
pub enum Outcome {
    Stored { bytes_len: usize, mime: String },
    AlreadyPresent,
    LinkOnly { url: String },
    NoOaFound { reason: String },
}

const MAX_ARTIFACT_BYTES: usize = 50 * 1024 * 1024;

/// Acquire the fulltext artifact for `id` from the open web and store it.
///
/// Decision flow:
/// 1. Idempotency — skip if a Bytes payload already exists (unless `force`).
/// 2. Resolve a downloadable URL from stored OpenAlex attrs and/or Unpaywall.
/// 3. Download the artifact (60 s timeout, 50 MB cap).
/// 4. Validate %PDF magic if `require_pdf` or the mime/URL suggest PDF.
/// 5. PUT to the store as a Fulltext/Open payload.
/// 6. If only a landing page is available, store a LinkOnly descriptor instead.
pub fn fetch_content(
    store: &StoreClient,
    unpaywall_email: Option<&str>,
    id: &str,
    from: Source,
    require_pdf: bool,
    force: bool,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    // Step 1: idempotency
    if !force {
        if let Ok(ContentOutcome::Bytes { .. }) = store.get_content(id, "fulltext") {
            return Ok(Outcome::AlreadyPresent);
        }
    }

    // Step 2: fetch stored work record
    let work = store
        .get_work(id)?
        .ok_or_else(|| format!("work not found in store: {}", id))?;

    // Step 3: resolve a downloadable artifact URL
    let resolved = resolve_artifact_url(&work, &from, unpaywall_email)?;

    let (artifact_url, source_label) = match resolved {
        Some(pair) => pair,
        None => {
            // No downloadable URL — try to record a landing page as LinkOnly
            if let Some(landing) = resolve_landing_url(&work) {
                store.put_content(
                    id,
                    "fulltext",
                    vec![],
                    "text/html",
                    "link_only",
                    Some("openalex-oa"),
                    Some(&landing),
                )?;
                return Ok(Outcome::LinkOnly { url: landing });
            }
            let reason = match from {
                Source::Unpaywall if unpaywall_email.is_none() => {
                    "unpaywall requires BRAINCRAWL_UNPAYWALL_EMAIL to be set".to_string()
                }
                _ => "no open-access artifact or landing page found in stored attrs or Unpaywall"
                    .to_string(),
            };
            return Ok(Outcome::NoOaFound { reason });
        }
    };

    // Step 4: download
    let (bytes, mime) = download_artifact(&artifact_url)?;

    // Step 5: validate PDF magic bytes when requested or when content appears to be PDF
    if (require_pdf || mime.contains("pdf") || artifact_url.to_ascii_lowercase().ends_with(".pdf"))
        && !is_valid_pdf(&bytes)
    {
        return Err(format!(
            "artifact at {} does not start with %PDF magic bytes \
             (received {} bytes, content-type: {})",
            artifact_url,
            bytes.len(),
            mime
        )
        .into());
    }

    // Step 6: store
    store.put_content(
        id,
        "fulltext",
        bytes.clone(),
        &mime,
        "open",
        Some(source_label),
        Some(&artifact_url),
    )?;

    Ok(Outcome::Stored { bytes_len: bytes.len(), mime })
}

// ── URL resolution ────────────────────────────────────────────────────────────

/// Resolve and download the artifact for `id` WITHOUT writing to the store.
/// Returns (bytes, mime, artifact_url). Errors if no downloadable URL exists
/// (no LinkOnly fallback — that belongs to fetch_content).
pub fn fetch_artifact_bytes(
    store: &StoreClient,
    unpaywall_email: Option<&str>,
    id: &str,
    from: Source,
    require_pdf: bool,
) -> Result<(Vec<u8>, String, String), Box<dyn std::error::Error>> {
    let work = store
        .get_work(id)?
        .ok_or_else(|| format!("work not found in store: {}", id))?;

    let (artifact_url, _source_label) =
        resolve_artifact_url(&work, &from, unpaywall_email)?
            .ok_or_else(|| format!("no downloadable artifact URL found for {}", id))?;

    let (bytes, mime) = download_artifact(&artifact_url)?;

    if (require_pdf || mime.contains("pdf") || artifact_url.to_ascii_lowercase().ends_with(".pdf"))
        && !is_valid_pdf(&bytes)
    {
        return Err(format!(
            "artifact at {} does not start with %PDF magic bytes \
             (received {} bytes, content-type: {})",
            artifact_url,
            bytes.len(),
            mime
        )
        .into());
    }

    Ok((bytes, mime, artifact_url))
}

/// Best-effort mime for raw bytes with no HTTP response to read.
pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"%PDF") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

/// Resolve a downloadable URL from stored attrs and/or Unpaywall.
/// Returns `(url, source_label)` or `None` if no URL is found.
pub fn resolve_artifact_url(
    work: &serde_json::Value,
    from: &Source,
    unpaywall_email: Option<&str>,
) -> Result<Option<(String, &'static str)>, Box<dyn std::error::Error>> {
    // Try OpenAlex stored attrs first
    if matches!(from, Source::Auto | Source::Openalex) {
        if let Some(url) = resolve_oa_url_from_attrs(work) {
            return Ok(Some((url, "openalex-oa")));
        }
    }

    // Unpaywall fallback (Auto) or explicit request (Unpaywall)
    if matches!(from, Source::Auto | Source::Unpaywall) {
        match (extract_doi(work), unpaywall_email) {
            (Some(doi), Some(email)) => {
                if let Some(url) = query_unpaywall(&doi, email)? {
                    return Ok(Some((url, "unpaywall")));
                }
            }
            (None, _) if matches!(from, Source::Unpaywall) => {
                return Err("no doi alias found; unpaywall lookup requires a DOI".into());
            }
            (_, None) if matches!(from, Source::Unpaywall) => {
                return Err(
                    "--from unpaywall requires BRAINCRAWL_UNPAYWALL_EMAIL to be set".into()
                );
            }
            // Auto + no email or no DOI: silently skip Unpaywall
            _ => {}
        }
    }

    Ok(None)
}

/// Extract the best open-access downloadable URL from OpenAlex-sourced work attrs.
///
/// Precedence (highest to lowest):
///   1. `primary_location.pdf_url`
///   2. `open_access.oa_url`
///   3. `best_oa_location.pdf_url`
pub fn resolve_oa_url_from_attrs(work: &serde_json::Value) -> Option<String> {
    let attrs = &work["attrs"];
    for url_ptr in [
        &attrs["primary_location"]["pdf_url"],
        &attrs["open_access"]["oa_url"],
        &attrs["best_oa_location"]["pdf_url"],
    ] {
        if let Some(s) = url_ptr.as_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Extract a landing-page URL for a LinkOnly descriptor (no downloadable artifact).
fn resolve_landing_url(work: &serde_json::Value) -> Option<String> {
    let attrs = &work["attrs"];
    for url_ptr in [
        &attrs["primary_location"]["landing_page_url"],
        &attrs["open_access"]["oa_url"],
    ] {
        if let Some(s) = url_ptr.as_str() {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Extract the bare DOI value from a WorkView's aliases list.
pub fn extract_doi(work: &serde_json::Value) -> Option<String> {
    let aliases = work["aliases"].as_array()?;
    for alias in aliases {
        if alias["namespace"].as_str() == Some("doi") {
            if let Some(v) = alias["value"].as_str() {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ── Network helpers ───────────────────────────────────────────────────────────

/// Query the Unpaywall API for the best open-access PDF URL for a DOI.
fn query_unpaywall(
    doi: &str,
    email: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let url = format!("https://api.unpaywall.org/v2/{}?email={}", doi, email);
    let client = reqwest::blocking::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(format!("braincrawl/{} (+fetch-content)", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let json: serde_json::Value = resp.json()?;
    for url_ptr in [
        &json["best_oa_location"]["url_for_pdf"],
        &json["best_oa_location"]["url"],
    ] {
        if let Some(s) = url_ptr.as_str() {
            if !s.is_empty() {
                return Ok(Some(s.to_string()));
            }
        }
    }
    Ok(None)
}

/// Download an artifact URL, returning `(bytes, mime)`.
/// Follows redirects, enforces a 60 s timeout and a 50 MB size cap.
pub fn download_artifact(url: &str) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(format!("braincrawl/{} (+fetch-content)", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading {}", resp.status(), url).into());
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let body = resp.bytes()?;
    if body.len() > MAX_ARTIFACT_BYTES {
        return Err(format!(
            "artifact too large: {} bytes (limit {} MB)",
            body.len(),
            MAX_ARTIFACT_BYTES / (1024 * 1024)
        )
        .into());
    }
    Ok((body.to_vec(), mime))
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Return `true` if `bytes` begins with the PDF magic sequence `%PDF`.
pub fn is_valid_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── URL resolution precedence ─────────────────────────────────────────────

    #[test]
    fn primary_location_pdf_url_wins() {
        let work = json!({
            "attrs": {
                "primary_location": { "pdf_url": "https://example.com/primary.pdf" },
                "open_access": { "oa_url": "https://example.com/oa" },
                "best_oa_location": { "pdf_url": "https://example.com/best.pdf" }
            },
            "aliases": []
        });
        assert_eq!(
            resolve_oa_url_from_attrs(&work),
            Some("https://example.com/primary.pdf".to_string())
        );
    }

    #[test]
    fn oa_url_wins_when_no_primary_pdf() {
        let work = json!({
            "attrs": {
                "primary_location": {},
                "open_access": { "oa_url": "https://example.com/oa" },
                "best_oa_location": { "pdf_url": "https://example.com/best.pdf" }
            },
            "aliases": []
        });
        assert_eq!(
            resolve_oa_url_from_attrs(&work),
            Some("https://example.com/oa".to_string())
        );
    }

    #[test]
    fn best_oa_location_is_last_fallback() {
        let work = json!({
            "attrs": {
                "primary_location": {},
                "open_access": {},
                "best_oa_location": { "pdf_url": "https://example.com/best.pdf" }
            },
            "aliases": []
        });
        assert_eq!(
            resolve_oa_url_from_attrs(&work),
            Some("https://example.com/best.pdf".to_string())
        );
    }

    #[test]
    fn no_url_when_all_absent() {
        let work = json!({
            "attrs": {
                "primary_location": {},
                "open_access": {},
                "best_oa_location": {}
            },
            "aliases": []
        });
        assert_eq!(resolve_oa_url_from_attrs(&work), None);
    }

    #[test]
    fn empty_string_urls_are_skipped() {
        let work = json!({
            "attrs": {
                "primary_location": { "pdf_url": "" },
                "open_access": { "oa_url": "" },
                "best_oa_location": { "pdf_url": "https://example.com/best.pdf" }
            },
            "aliases": []
        });
        assert_eq!(
            resolve_oa_url_from_attrs(&work),
            Some("https://example.com/best.pdf".to_string())
        );
    }

    // ── PDF validation ────────────────────────────────────────────────────────

    #[test]
    fn accepts_pdf_magic_bytes() {
        assert!(is_valid_pdf(b"%PDF-1.5\n..."));
        assert!(is_valid_pdf(b"%PDF"));
    }

    #[test]
    fn rejects_html_body() {
        assert!(!is_valid_pdf(b"<html><body>Access denied</body></html>"));
        assert!(!is_valid_pdf(b"<!DOCTYPE html>"));
    }

    #[test]
    fn rejects_empty_body() {
        assert!(!is_valid_pdf(b""));
    }

    #[test]
    fn rejects_other_content() {
        assert!(!is_valid_pdf(b"Not a PDF at all"));
    }

    // ── sniff_mime ────────────────────────────────────────────────────────────

    #[test]
    fn sniff_mime_detects_pdf() {
        assert_eq!(sniff_mime(b"%PDF-1.7 ..."), "application/pdf");
        assert_eq!(sniff_mime(b"%PDF"), "application/pdf");
    }

    #[test]
    fn sniff_mime_falls_back_to_octet_stream() {
        assert_eq!(sniff_mime(b"<html></html>"), "application/octet-stream");
        assert_eq!(sniff_mime(b""), "application/octet-stream");
        assert_eq!(sniff_mime(b"not a pdf"), "application/octet-stream");
    }

    // ── DOI extraction ────────────────────────────────────────────────────────

    #[test]
    fn extracts_doi_from_aliases() {
        let work = json!({
            "attrs": {},
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "doi", "value": "10.1234/test"}
            ]
        });
        assert_eq!(extract_doi(&work), Some("10.1234/test".to_string()));
    }

    #[test]
    fn returns_none_when_no_doi() {
        let work = json!({
            "attrs": {},
            "aliases": [
                {"namespace": "openalex", "value": "W123"}
            ]
        });
        assert_eq!(extract_doi(&work), None);
    }
}

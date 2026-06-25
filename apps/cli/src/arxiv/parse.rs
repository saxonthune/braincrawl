use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde_json::{json, Value};

use super::{ArxivError, Result};

/// Parse an arXiv Atom XML feed into normalized JSON values (one per entry)
/// and the feed's total result count.
pub fn parse_feed(xml: &str) -> Result<(Vec<Value>, u64)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries: Vec<Value> = Vec::new();
    let mut total_results: u64 = 0;

    // State machine over XML events
    let mut in_entry = false;
    let mut current_tag: Vec<u8> = Vec::new();

    // Per-entry accumulators
    let mut entry_id = String::new();
    let mut title = String::new();
    let mut summary = String::new();
    let mut published = String::new();
    let mut updated = String::new();
    let mut doi: Option<String> = None;
    let mut primary_category: Option<String> = None;
    let mut categories: Vec<String> = Vec::new();
    let mut authors: Vec<String> = Vec::new();
    let mut pdf_url: Option<String> = None;
    let mut in_author = false;
    let mut author_name = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(ArxivError::Parse(format!("XML parse error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                current_tag = local.to_vec();

                match current_tag.as_slice() {
                    b"entry" => {
                        in_entry = true;
                        entry_id.clear();
                        title.clear();
                        summary.clear();
                        published.clear();
                        updated.clear();
                        doi = None;
                        primary_category = None;
                        categories.clear();
                        authors.clear();
                        pdf_url = None;
                        in_author = false;
                        author_name.clear();
                    }
                    b"author" if in_entry => {
                        in_author = true;
                        author_name.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if in_entry {
                    match local.as_slice() {
                        b"primary_category" => {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"term" {
                                    primary_category = Some(
                                        String::from_utf8_lossy(&attr.value).into_owned(),
                                    );
                                }
                            }
                        }
                        b"category" => {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"term" {
                                    categories
                                        .push(String::from_utf8_lossy(&attr.value).into_owned());
                                }
                            }
                        }
                        b"link" => {
                            let mut is_pdf = false;
                            let mut href = String::new();
                            for attr in e.attributes().flatten() {
                                let k = attr.key.local_name();
                                match k.as_ref() {
                                    b"title" if &*attr.value == b"pdf" => is_pdf = true,
                                    b"href" => {
                                        href = String::from_utf8_lossy(&attr.value).into_owned()
                                    }
                                    _ => {}
                                }
                            }
                            if is_pdf && !href.is_empty() {
                                pdf_url = Some(href);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                match local.as_slice() {
                    b"author" if in_entry => {
                        if !author_name.is_empty() {
                            authors.push(std::mem::take(&mut author_name));
                        }
                        in_author = false;
                    }
                    b"entry" => {
                        in_entry = false;
                        let arxiv_id = normalize_id(&entry_id);
                        let title_clean = collapse_whitespace(&title);
                        let summary_clean = summary.trim().to_string();
                        let authors_val: Vec<Value> =
                            authors.iter().map(|a| json!({"name": a})).collect();

                        let mut record = json!({
                            "arxiv_id": arxiv_id,
                            "title": title_clean,
                            "abstract": summary_clean,
                            "authors": authors_val,
                            "published": published,
                            "updated": updated,
                            "primary_category": primary_category,
                            "categories": categories,
                        });
                        if let Some(d) = &doi {
                            record["doi"] = json!(d);
                        }
                        if let Some(u) = &pdf_url {
                            record["pdf_url"] = json!(u);
                        }
                        entries.push(record);
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                let text = text.as_ref();

                if in_entry {
                    match current_tag.as_slice() {
                        b"id" => entry_id.push_str(text),
                        b"title" => title.push_str(text),
                        b"summary" => summary.push_str(text),
                        b"published" => published.push_str(text),
                        b"updated" => updated.push_str(text),
                        b"doi" => doi = Some(text.trim().to_string()),
                        b"name" if in_author => author_name.push_str(text),
                        _ => {}
                    }
                } else if current_tag.as_slice() == b"totalResults" {
                    if let Ok(n) = text.trim().parse::<u64>() {
                        total_results = n;
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok((entries, total_results))
}

/// Strip the `http://arxiv.org/abs/` URL prefix and version suffix from an entry `<id>`.
fn normalize_id(raw: &str) -> String {
    let bare = raw
        .strip_prefix("https://arxiv.org/abs/")
        .or_else(|| raw.strip_prefix("http://arxiv.org/abs/"))
        .unwrap_or(raw);
    super::entity::strip_version_pub(bare)
}

/// Extract the local name (after `:`) from a possibly-namespaced XML element name.
fn local_name(name: &[u8]) -> &[u8] {
    if let Some(pos) = name.iter().rposition(|&b| b == b':') {
        &name[pos + 1..]
    } else {
        name
    }
}

/// Collapse internal whitespace/newlines in a title to a single space.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arxiv_search.xml");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()))
    }

    #[test]
    fn parse_entry_count() {
        let xml = load_fixture();
        let (entries, total) = parse_feed(&xml).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(total, 42);
    }

    #[test]
    fn first_entry_fields() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        let e = &entries[0];
        assert_eq!(e["arxiv_id"].as_str(), Some("2301.07041"), "version must be stripped");
        assert!(e["title"].as_str().unwrap().contains("Constrained Decoding"));
        assert!(!e["abstract"].as_str().unwrap().is_empty());
        let authors = e["authors"].as_array().unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0]["name"].as_str(), Some("Alice Researcher"));
    }

    #[test]
    fn first_entry_doi() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        let e = &entries[0];
        assert_eq!(e["doi"].as_str(), Some("10.18653/v1/2023.acl-long.108"));
    }

    #[test]
    fn first_entry_pdf_url() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        let e = &entries[0];
        assert!(e["pdf_url"].as_str().unwrap().contains("2301.07041"));
    }

    #[test]
    fn second_entry_no_doi() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        let e = &entries[1];
        assert_eq!(e["arxiv_id"].as_str(), Some("2206.06336"));
        assert!(e.get("doi").is_none() || e["doi"].is_null());
    }

    #[test]
    fn categories_parsed() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        let e = &entries[0];
        let cats = e["categories"].as_array().unwrap();
        assert!(cats.iter().any(|c| c.as_str() == Some("cs.CL")));
    }

    #[test]
    fn primary_category() {
        let xml = load_fixture();
        let (entries, _) = parse_feed(&xml).unwrap();
        assert_eq!(entries[0]["primary_category"].as_str(), Some("cs.CL"));
        assert_eq!(entries[1]["primary_category"].as_str(), Some("cs.LG"));
    }
}

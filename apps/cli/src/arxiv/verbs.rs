use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::Emission;

use super::Result;
use super::client::ArxivClient;
use super::entity::infer_id;
use super::{mapping, parse, shape};

const DEFAULT_PER_PAGE: u32 = 25;
const ARXIV_RATE_LIMIT_MS: u64 = 3_000;

/// arXiv field operators that can appear in user queries.
const FIELD_OPERATORS: &[&str] = &["ti:", "au:", "abs:", "cat:", "all:", "co:", "jr:"];

/// Fetch a single arXiv record by id.
pub fn get(
    client: &ArxivClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let bare_id = infer_id(id)?;
    let xml = client.get(&bare_id)?;
    let (records, _total) = parse::parse_feed(&xml)?;
    let count = records.len() as u64;
    let emission = mapping::to_emission(&records);
    let envelope = shape::build_envelope(
        records,
        count,
        Some(format!("arxiv:{bare_id}")),
        None,
        opts,
    );
    Ok((envelope, emission))
}

/// Search arXiv by query, with pagination.
pub fn search(
    client: &ArxivClient,
    query: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let search_query = if has_field_operator(query) {
        query.to_string()
    } else {
        format!("all:{query}")
    };

    let per_page = page_size(opts);
    let limit = if opts.all { None } else { opts.limit };

    // Fetch first page to get total_count, then paginate if --all requested.
    let xml = client.search(&search_query, 0, per_page)?;
    let (first_records, total_count) = parse::parse_feed(&xml)?;
    let mut all_records: Vec<serde_json::Value> = first_records;
    let mut start = all_records.len() as u32;

    while opts.all
        && !all_records.is_empty()
        && all_records.len() < total_count as usize
        && limit.is_none_or(|lim| all_records.len() < lim as usize)
    {
        std::thread::sleep(std::time::Duration::from_millis(ARXIV_RATE_LIMIT_MS));
        let xml = client.search(&search_query, start, per_page)?;
        let (records, _) = parse::parse_feed(&xml)?;
        if records.is_empty() { break; }
        start += records.len() as u32;
        all_records.extend(records);
    }

    let emission = mapping::to_emission(&all_records);
    let envelope = shape::build_envelope(
        all_records,
        total_count,
        Some(search_query),
        None,
        opts,
    );
    Ok((envelope, emission))
}

fn has_field_operator(query: &str) -> bool {
    FIELD_OPERATORS.iter().any(|op| query.contains(op))
}

fn page_size(opts: &OutputOpts) -> u32 {
    if let Some(lim) = opts.limit {
        (lim as u32).min(100)
    } else {
        DEFAULT_PER_PAGE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_field_operator_detects_ti() {
        assert!(has_field_operator("ti:constrained decoding"));
    }

    #[test]
    fn has_field_operator_detects_cat() {
        assert!(has_field_operator("cat:cs.LG"));
    }

    #[test]
    fn plain_query_has_no_operator() {
        assert!(!has_field_operator("constrained decoding"));
    }
}

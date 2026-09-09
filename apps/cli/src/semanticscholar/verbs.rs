use serde_json::Value;

use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::Emission;

use super::Result;
use super::client::SemanticScholarClient;
use super::entity::{Entity, infer_entity};
use super::mapping;
use super::shape::build_envelope;

const PAPER_FIELDS: &str =
    "paperId,externalIds,title,abstract,year,publicationDate,venue,citationCount,referenceCount,authors,openAccessPdf";
const AUTHOR_FIELDS: &str =
    "authorId,externalIds,name,affiliations,paperCount,citationCount,hIndex";

const DEFAULT_PER_PAGE: u32 = 25;

/// Fetch a single entity by ID, inferring the entity type from the ID form.
pub fn get(
    client: &SemanticScholarClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let (entity, path_id) = infer_entity(id)?;
    let (raw, url) = match entity {
        Entity::Papers => client.get_paper(&path_id, PAPER_FIELDS)?,
        Entity::Authors => client.get_author(&path_id, AUTHOR_FIELDS)?,
    };
    let records = vec![(entity, raw.clone())];
    let envelope = build_envelope(entity, vec![raw], 1, None, Some(path_id), Some(url), opts);
    Ok((envelope, mapping::to_emission(&records, &[])))
}

/// Full-text search over papers or authors.
pub fn search(
    client: &SemanticScholarClient,
    entity_str: &str,
    query: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let entity = Entity::parse(entity_str)?;
    let per_page = page_size(opts);
    let limit = if opts.all { None } else { opts.limit };

    let mut all_results: Vec<Value> = Vec::new();
    let mut total_count = 0u64;
    let mut offset = 0u32;

    loop {
        let raw = match entity {
            Entity::Papers => client.search_papers(query, PAPER_FIELDS, offset, per_page)?,
            Entity::Authors => client.search_authors(query, AUTHOR_FIELDS, offset, per_page)?,
        };

        let count = raw.get("total").and_then(|v| v.as_u64()).unwrap_or(total_count);
        total_count = count;
        let results: Vec<Value> = raw
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let batch_empty = results.is_empty();
        all_results.extend(results);

        let next = raw.get("next").and_then(|v| v.as_u64());
        let reached_limit = limit.is_some_and(|lim| all_results.len() >= lim as usize);

        if batch_empty || next.is_none() || reached_limit || !opts.all {
            break;
        }
        offset = next.unwrap() as u32;
    }

    let records: Vec<(Entity, Value)> = all_results.iter().map(|v| (entity, v.clone())).collect();
    let envelope = build_envelope(
        entity,
        all_results,
        total_count,
        None,
        Some(query.to_string()),
        None,
        opts,
    );
    Ok((envelope, mapping::to_emission(&records, &[])))
}

/// List papers that cite the given paper ID.
/// Each citing paper is pushed as a Work node AND a citation edge is emitted.
pub fn cited_by(
    client: &SemanticScholarClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let (_, path_id) = infer_entity(id)?;

    // Fetch seed paper's externalIds to compute its best merge alias
    let (seed_raw, _) = client.get_paper(&path_id, "paperId,externalIds")?;
    let seed_alias = best_paper_alias(&seed_raw);

    let per_page = page_size(opts);
    let limit = if opts.all { None } else { opts.limit };

    let mut all_papers: Vec<Value> = Vec::new();
    let mut edge_pairs: Vec<(String, String)> = Vec::new();
    let mut offset = 0u32;

    loop {
        let raw = client.citations(&path_id, PAPER_FIELDS, offset, per_page)?;
        let data: Vec<Value> = raw
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let batch_empty = data.is_empty();

        for item in &data {
            if let Some(citing_paper) = item.get("citingPaper") {
                let citing_alias = best_paper_alias(citing_paper);
                edge_pairs.push((citing_alias, seed_alias.clone()));
                all_papers.push(citing_paper.clone());
            }
        }

        let next = raw.get("next").and_then(|v| v.as_u64());
        let reached_limit = limit.is_some_and(|lim| all_papers.len() >= lim as usize);

        if batch_empty || next.is_none() || reached_limit || !opts.all {
            break;
        }
        offset = next.unwrap() as u32;
    }

    if all_papers.is_empty() {
        let envelope = build_envelope(
            Entity::Papers,
            vec![],
            0,
            None,
            Some(format!("cited-by:{path_id}")),
            None,
            opts,
        );
        return Ok((envelope, Emission::empty()));
    }

    let count = all_papers.len() as u64;
    let records: Vec<(Entity, Value)> =
        all_papers.iter().map(|v| (Entity::Papers, v.clone())).collect();
    let envelope = build_envelope(
        Entity::Papers,
        all_papers,
        count,
        None,
        Some(format!("cited-by:{path_id}")),
        None,
        opts,
    );
    Ok((envelope, mapping::to_emission(&records, &edge_pairs)))
}

/// List papers referenced by the given paper ID.
/// Each cited paper is pushed as a Work node AND a citation edge is emitted.
pub fn refs(
    client: &SemanticScholarClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, Emission)> {
    let (_, path_id) = infer_entity(id)?;

    // Fetch seed paper's externalIds to compute its best merge alias
    let (seed_raw, _) = client.get_paper(&path_id, "paperId,externalIds")?;
    let seed_alias = best_paper_alias(&seed_raw);

    let per_page = page_size(opts);
    let limit = if opts.all { None } else { opts.limit };

    let mut all_papers: Vec<Value> = Vec::new();
    let mut edge_pairs: Vec<(String, String)> = Vec::new();
    let mut offset = 0u32;

    loop {
        let raw = client.references(&path_id, PAPER_FIELDS, offset, per_page)?;
        let data: Vec<Value> = raw
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let batch_empty = data.is_empty();

        for item in &data {
            if let Some(cited_paper) = item.get("citedPaper") {
                let cited_alias = best_paper_alias(cited_paper);
                edge_pairs.push((seed_alias.clone(), cited_alias));
                all_papers.push(cited_paper.clone());
            }
        }

        let next = raw.get("next").and_then(|v| v.as_u64());
        let reached_limit = limit.is_some_and(|lim| all_papers.len() >= lim as usize);

        if batch_empty || next.is_none() || reached_limit || !opts.all {
            break;
        }
        offset = next.unwrap() as u32;
    }

    if all_papers.is_empty() {
        let envelope = build_envelope(
            Entity::Papers,
            vec![],
            0,
            None,
            Some(format!("refs:{path_id}")),
            None,
            opts,
        );
        return Ok((envelope, Emission::empty()));
    }

    let count = all_papers.len() as u64;
    let records: Vec<(Entity, Value)> =
        all_papers.iter().map(|v| (Entity::Papers, v.clone())).collect();
    let envelope = build_envelope(
        Entity::Papers,
        all_papers,
        count,
        None,
        Some(format!("refs:{path_id}")),
        None,
        opts,
    );
    Ok((envelope, mapping::to_emission(&records, &edge_pairs)))
}

/// Choose the best alias for a paper to maximize merge with OpenAlex edges.
/// Prefers `doi:<bare>` if the paper has a DOI, else falls back to `s2:<paperId>`.
fn best_paper_alias(record: &Value) -> String {
    if let Some(ext) = record.get("externalIds").and_then(|v| v.as_object()) {
        if let Some(doi) = ext.get("DOI").and_then(|v| v.as_str()) {
            let bare = doi.strip_prefix("https://doi.org/").unwrap_or(doi);
            return format!("doi:{bare}");
        }
    }
    if let Some(paper_id) = record.get("paperId").and_then(|v| v.as_str()) {
        return format!("s2:{paper_id}");
    }
    "s2:unknown".to_string()
}

fn page_size(opts: &OutputOpts) -> u32 {
    if let Some(lim) = opts.limit {
        (lim as u32).min(100)
    } else {
        DEFAULT_PER_PAGE
    }
}

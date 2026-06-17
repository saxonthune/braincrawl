use serde_json::Value;

use crate::cli::OutputOpts;
use crate::output::Envelope;

use super::{PushBatch, Result};
use super::client::{ListParams, OpenAlexClient};
use super::entity::{Entity, infer_entity};
use super::filters::{KV, validate_filters};
use super::shape::build_envelope;

const DEFAULT_PER_PAGE: u32 = 25;
const REFS_CHUNK_SIZE: usize = 50;

/// Fetch a single entity by ID, inferring the entity type from the ID form.
pub fn get(
    client: &OpenAlexClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let (entity, path_id) = infer_entity(id)?;
    let (raw, url) = client.get_one(entity, &path_id, None)?;
    let records = vec![(entity, raw.clone())];
    let envelope = build_envelope(
        entity,
        vec![raw],
        1,
        None,
        Some(path_id),
        Some(url),
        opts,
    );
    Ok((envelope, PushBatch { records, edges: Vec::new() }))
}

/// Full-text search over an entity collection.
pub fn search(
    client: &OpenAlexClient,
    entity_str: &str,
    query: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let entity = Entity::parse(entity_str)?;
    let per_page = page_size(opts);
    collect_pages(
        client,
        entity,
        ListParams { search: Some(query), per_page: Some(per_page), ..Default::default() },
        Some(query.to_string()),
        opts,
    )
}

/// Filter an entity collection by key:value pairs.
pub fn find(
    client: &OpenAlexClient,
    entity_str: &str,
    filter_strs: &[String],
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let entity = Entity::parse(entity_str)?;
    let kvs: std::result::Result<Vec<KV>, String> =
        filter_strs.iter().map(|s| KV::parse(s)).collect();
    let kvs = kvs.map_err(super::OpenAlexError::BadFilterExpr)?;
    let filter_val = validate_filters(entity, &kvs)?;
    let resolved = filter_val.clone();
    let per_page = page_size(opts);
    collect_pages(
        client,
        entity,
        ListParams {
            filter: Some(filter_val.as_str()),
            per_page: Some(per_page),
            ..Default::default()
        },
        Some(resolved),
        opts,
    )
}

/// Autocomplete entity names by prefix query.
pub fn autocomplete(
    client: &OpenAlexClient,
    entity_str: &str,
    q: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let entity = Entity::parse(entity_str)?;
    let (raw, url) = client.autocomplete(entity, q)?;
    let count = raw
        .get("meta")
        .and_then(|m| m.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let results: Vec<Value> = raw
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let records: Vec<(Entity, Value)> = results.iter().map(|r| (entity, r.clone())).collect();
    let envelope = build_envelope(entity, results, count, None, Some(q.to_string()), Some(url), opts);
    Ok((envelope, PushBatch { records, edges: Vec::new() }))
}

/// List works that cite the given work ID.
pub fn cited_by(
    client: &OpenAlexClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let (citing_entity, path_id) = infer_entity(id)?;
    let filter = format!("cites:{path_id}");
    let resolved = filter.clone();
    let per_page = page_size(opts);
    let (envelope, mut batch) = collect_pages(
        client,
        Entity::Works,
        ListParams {
            filter: Some(filter.as_str()),
            per_page: Some(per_page),
            ..Default::default()
        },
        Some(resolved),
        opts,
    )?;
    // Record citation edges: each result cites the given ID.
    // Use bare OpenAlex IDs (not entity-path prefix); mapping.rs strips URL prefixes.
    let _ = citing_entity; // entity type used only for infer; edge uses the bare path_id
    for result in &envelope.results {
        if let Some(citing_id) = result.get("id").and_then(|v| v.as_str()) {
            batch.edges.push((citing_id.to_string(), path_id.clone()));
        }
    }
    Ok((envelope, batch))
}

/// List works referenced by the given work ID.
pub fn refs(
    client: &OpenAlexClient,
    id: &str,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let (_, path_id) = infer_entity(id)?;
    // Step 1: get just the referenced_works list
    let (raw, _) = client.get_one(Entity::Works, &path_id, Some("id,referenced_works"))?;
    let ref_ids: Vec<String> = raw
        .get("referenced_works")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| {
                    // Strip URL prefix to get bare ID
                    s.strip_prefix("https://openalex.org/").unwrap_or(s).to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    if ref_ids.is_empty() {
        let envelope = build_envelope(
            Entity::Works,
            vec![],
            0,
            None,
            Some(format!("refs:{path_id}")),
            None,
            opts,
        );
        return Ok((envelope, PushBatch::empty()));
    }

    // Step 2: batch fetch in chunks of ≤50 using ids.openalex OR filter
    let mut all_results: Vec<Value> = Vec::new();
    let mut all_records: Vec<(Entity, Value)> = Vec::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    let citing_id = format!("https://openalex.org/{path_id}");

    for chunk in ref_ids.chunks(REFS_CHUNK_SIZE) {
        let filter_val = format!("ids.openalex:{}", chunk.join("|"));
        let per_page = REFS_CHUNK_SIZE.min(100) as u32;
        let page = client.list(
            Entity::Works,
            ListParams {
                filter: Some(filter_val.as_str()),
                per_page: Some(per_page),
                ..Default::default()
            },
        )?;
        for r in &page.results {
            if let Some(cited_id) = r.get("id").and_then(|v| v.as_str()) {
                edges.push((citing_id.clone(), cited_id.to_string()));
            }
        }
        all_records.extend(page.results.iter().map(|r| (Entity::Works, r.clone())));
        all_results.extend(page.results);
    }

    let count = all_results.len() as u64;
    let envelope = build_envelope(
        Entity::Works,
        all_results,
        count,
        None,
        Some(format!("refs:{path_id}")),
        None,
        opts,
    );
    Ok((envelope, PushBatch { records: all_records, edges }))
}

/// Collect pages from a list endpoint, respecting --all / --limit.
/// Returns an envelope and a PushBatch of raw records.
fn collect_pages(
    client: &OpenAlexClient,
    entity: Entity,
    base_params: ListParams<'_>,
    resolved_filter: Option<String>,
    opts: &OutputOpts,
) -> Result<(Envelope, PushBatch)> {
    let limit = if opts.all { None } else { opts.limit };
    let mut all_results: Vec<Value> = Vec::new();
    let mut last_url: Option<String> = None;
    let mut cursor: Option<String> = None;
    let mut total_count;

    loop {
        let cursor_str = cursor.as_deref().unwrap_or("*");
        let params = ListParams {
            filter: base_params.filter,
            search: base_params.search,
            sort: base_params.sort,
            select: base_params.select,
            per_page: base_params.per_page,
            cursor: Some(cursor_str),
        };
        let page = client.list(entity, params)?;
        if last_url.is_none() {
            last_url = Some(page.url.clone());
        }
        total_count = page.count;
        let batch_empty = page.results.is_empty();
        all_results.extend(page.results);

        let reached_limit = limit.map_or(false, |lim| all_results.len() >= lim as usize);
        if batch_empty || page.next_cursor.is_none() || reached_limit {
            break;
        }
        if !opts.all {
            break;
        }
        cursor = page.next_cursor;
    }

    let records: Vec<(Entity, Value)> = all_results.iter().map(|v| (entity, v.clone())).collect();
    let envelope = build_envelope(
        entity,
        all_results,
        total_count,
        None,
        resolved_filter,
        last_url,
        opts,
    );
    Ok((envelope, PushBatch { records, edges: Vec::new() }))
}

fn page_size(opts: &OutputOpts) -> u32 {
    if let Some(lim) = opts.limit {
        (lim as u32).min(100)
    } else {
        DEFAULT_PER_PAGE
    }
}

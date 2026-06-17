use braincrawl_cli::cli::{Cli, GraphCmd, Namespace, OpenalexCmd, OutputOpts, SemanticscholarCmd, StoreCmd};
use std::fmt::Write as _;
use braincrawl_cli::config::Config;
use braincrawl_cli::fetch_content;
use braincrawl_cli::openalex::client::OpenAlexClient;
use braincrawl_cli::openalex::mapping::{to_edges, to_work_record};
use braincrawl_cli::openalex::verbs;
use braincrawl_cli::openalex::{PushBatch, PushSummary};
use braincrawl_cli::output::{Envelope, QueryMeta, render};
use braincrawl_cli::semanticscholar::client::SemanticScholarClient;
use braincrawl_cli::semanticscholar::mapping::{
    to_edges as s2_to_edges, to_work_record as s2_to_work_record,
};
use braincrawl_cli::semanticscholar::verbs as s2_verbs;
use braincrawl_cli::semanticscholar::{PushBatch as S2PushBatch, PushSummary as S2PushSummary};
use braincrawl_cli::store_client::StoreClient;
use clap::Parser;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let opts = OutputOpts::from(cli.global);
    let config = Config::resolve();

    match cli.namespace {
        Namespace::Store(store) => {
            let client = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            match store.cmd {
                StoreCmd::Have { ids } => {
                    let present = client.have(&ids)?;
                    let count = present.len() as u64;
                    let results = present
                        .into_iter()
                        .map(|s| serde_json::json!({ "id": s }))
                        .collect();
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("store:have".to_string()),
                            resolved_filter: None,
                            url: None,
                        },
                        count,
                        returned: count as usize,
                        truncated: false,
                        next_cursor: None,
                        results,
                    };
                    render(&envelope, &opts);
                }
                StoreCmd::Get { id } => {
                    let result = client.get_work(&id)?;
                    let results = result.map(|v| vec![v]).unwrap_or_default();
                    let count = results.len() as u64;
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("store:get".to_string()),
                            resolved_filter: Some(id),
                            url: None,
                        },
                        count,
                        returned: results.len(),
                        truncated: false,
                        next_cursor: None,
                        results,
                    };
                    render(&envelope, &opts);
                }
            }
        }
        Namespace::Graph(graph) => {
            let client = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            match graph.cmd {
                GraphCmd::Neighborhood { seeds, dir, depth, max_nodes } => {
                    let value = client.neighborhood(&seeds, &dir, depth, max_nodes)?;
                    render_neighborhood(&value, &opts);
                }
            }
        }
        Namespace::Stats => {
            let client = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let value = client.stats()?;
            render_stats(&value, &opts);
        }
        Namespace::Openalex(oa) => {
            let oa_client = OpenAlexClient::new(config.openalex_api_key);
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let (envelope, push_batch) = match oa.cmd {
                OpenalexCmd::Get { id } => verbs::get(&oa_client, &id, &opts)?,
                OpenalexCmd::Search { entity, query } => {
                    verbs::search(&oa_client, &entity, &query, &opts)?
                }
                OpenalexCmd::Find { entity, filters } => {
                    verbs::find(&oa_client, &entity, &filters, &opts)?
                }
                OpenalexCmd::Autocomplete { entity, q } => {
                    verbs::autocomplete(&oa_client, &entity, &q, &opts)?
                }
                OpenalexCmd::CitedBy { id } => verbs::cited_by(&oa_client, &id, &opts)?,
                OpenalexCmd::Refs { id } => verbs::refs(&oa_client, &id, &opts)?,
            };
            render(&envelope, &opts);
            if !opts.skip_push {
                let summary = push_batch_to_store(&store, &push_batch);
                report_push_summary(&summary);
                if !summary.errors.is_empty() {
                    std::process::exit(1);
                }
            }
        }
        Namespace::Semanticscholar(s2) => {
            let s2_client = SemanticScholarClient::new(config.semanticscholar_api_key);
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let (envelope, push_batch) = match s2.cmd {
                SemanticscholarCmd::Get { id } => s2_verbs::get(&s2_client, &id, &opts)?,
                SemanticscholarCmd::Search { entity, query } => {
                    s2_verbs::search(&s2_client, &entity, &query, &opts)?
                }
                SemanticscholarCmd::CitedBy { id } => {
                    s2_verbs::cited_by(&s2_client, &id, &opts)?
                }
                SemanticscholarCmd::Refs { id } => s2_verbs::refs(&s2_client, &id, &opts)?,
            };
            render(&envelope, &opts);
            if !opts.skip_push {
                let summary = push_s2_batch_to_store(&store, &push_batch);
                report_s2_push_summary(&summary);
                if !summary.errors.is_empty() {
                    std::process::exit(1);
                }
            }
        }
        Namespace::FetchContent(fc) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let source = match fc.from.as_str() {
                "openalex" => fetch_content::Source::Openalex,
                "unpaywall" => fetch_content::Source::Unpaywall,
                _ => fetch_content::Source::Auto,
            };
            match fetch_content::fetch_content(
                &store,
                config.unpaywall_email.as_deref(),
                &fc.id,
                source,
                fc.require_pdf,
                fc.force,
            )? {
                fetch_content::Outcome::Stored { bytes_len, mime } => {
                    println!("stored: {} bytes, mime={}", bytes_len, mime);
                }
                fetch_content::Outcome::AlreadyPresent => {
                    println!("already-present: fulltext payload already in store (use --force to re-fetch)");
                }
                fetch_content::Outcome::LinkOnly { url } => {
                    println!("link-only: {}", url);
                }
                fetch_content::Outcome::NoOaFound { reason } => {
                    return Err(format!("no-oa-found: {}", reason).into());
                }
            }
        }
    }

    Ok(())
}

fn render_neighborhood(value: &serde_json::Value, opts: &OutputOpts) {
    if opts.text {
        let empty = vec![];
        let nodes = value["nodes"].as_array().unwrap_or(&empty);
        for node in nodes {
            let id = node["canonical_id"].as_str().unwrap_or("");
            let in_degree = node["in_degree"].as_u64().unwrap_or(0);
            let title = node["attrs"]["title"]
                .as_str()
                .or_else(|| node["attrs"]["display_name"].as_str())
                .unwrap_or("");
            println!("{}\t{}\t{}", id, in_degree, title);
        }
        let node_count = nodes.len();
        let edge_count = value["edges"].as_array().map(|e| e.len()).unwrap_or(0);
        let truncated = value["truncated"].as_bool().unwrap_or(false);
        if truncated {
            eprintln!("{} node(s), {} edge(s) [truncated]", node_count, edge_count);
        } else {
            eprintln!("{} node(s), {} edge(s)", node_count, edge_count);
        }
    } else {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    }
}

fn render_stats(value: &serde_json::Value, opts: &OutputOpts) {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
        return;
    }

    let n = |key: &str| value[key].as_u64().unwrap_or(0);

    // Render a grouped breakdown as indented `key  count` lines, count desc.
    let tally = |out: &mut String, label: &str, key: &str| {
        let empty = vec![];
        let rows = value[key].as_array().unwrap_or(&empty);
        let _ = writeln!(out, "{label}:");
        if rows.is_empty() {
            let _ = writeln!(out, "  (none)");
        }
        for row in rows {
            let k = row["key"].as_str().unwrap_or("");
            let c = row["count"].as_u64().unwrap_or(0);
            let _ = writeln!(out, "  {k:<16} {c}");
        }
    };

    let mut out = String::new();
    let _ = writeln!(out, "works:            {}", n("works"));
    let _ = writeln!(out, "  described:      {}", n("works_described"));
    let _ = writeln!(out, "  stubs:          {}", n("works_stub"));
    let _ = writeln!(out, "nodes (live):     {}", n("nodes_total"));
    let _ = writeln!(out, "tombstones:       {}", n("tombstones"));
    let _ = writeln!(out, "edges:            {}", n("edges_total"));
    let _ = writeln!(out);
    tally(&mut out, "nodes by kind", "nodes_by_kind");
    let _ = writeln!(out);
    tally(&mut out, "edges by relation", "edges_by_relation");
    let _ = writeln!(out);
    tally(&mut out, "assertions by source", "assertions_by_source");
    print!("{out}");
}

fn push_batch_to_store(store: &StoreClient, batch: &PushBatch) -> PushSummary {
    let mut nodes_pushed = 0usize;
    let mut skipped_unmappable = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (entity, record) in &batch.records {
        match to_work_record(*entity, record) {
            None => skipped_unmappable += 1,
            Some(work_record) => match store.put_work(&work_record) {
                Ok(_) => nodes_pushed += 1,
                Err(e) => errors.push(format!("push node failed: {e}")),
            },
        }
    }

    let mut edges_pushed = 0u64;
    if !batch.edges.is_empty() {
        let edge_values = to_edges(&batch.edges);
        match store.put_edges(&edge_values) {
            Ok(n) => edges_pushed = n,
            Err(e) => errors.push(format!("push edges failed: {e}")),
        }
    }

    PushSummary { nodes_pushed, edges_pushed, skipped_unmappable, errors }
}

fn report_push_summary(s: &PushSummary) {
    eprintln!(
        "push: {} node(s) stored, {} edge(s) stored, {} skipped (unmappable kind)",
        s.nodes_pushed, s.edges_pushed, s.skipped_unmappable
    );
    for e in &s.errors {
        eprintln!("push error: {e}");
    }
}

fn push_s2_batch_to_store(store: &StoreClient, batch: &S2PushBatch) -> S2PushSummary {
    let mut nodes_pushed = 0usize;
    let mut skipped_unmappable = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (entity, record) in &batch.records {
        match s2_to_work_record(*entity, record) {
            None => skipped_unmappable += 1,
            Some(work_record) => match store.put_work(&work_record) {
                Ok(_) => nodes_pushed += 1,
                Err(e) => errors.push(format!("push node failed: {e}")),
            },
        }
    }

    let mut edges_pushed = 0u64;
    if !batch.edges.is_empty() {
        let edge_values = s2_to_edges(&batch.edges);
        match store.put_edges(&edge_values) {
            Ok(n) => edges_pushed = n,
            Err(e) => errors.push(format!("push edges failed: {e}")),
        }
    }

    S2PushSummary { nodes_pushed, edges_pushed, skipped_unmappable, errors }
}

fn report_s2_push_summary(s: &S2PushSummary) {
    eprintln!(
        "push: {} node(s) stored, {} edge(s) stored, {} skipped (unmappable kind)",
        s.nodes_pushed, s.edges_pushed, s.skipped_unmappable
    );
    for e in &s.errors {
        eprintln!("push error: {e}");
    }
}

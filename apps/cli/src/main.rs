use braincrawl_cli::arxiv::ArxivProvider;
use braincrawl_cli::cli::{ArxivCmd, Cli, CrossrefCmd, GraphCmd, Namespace, OpencitationsCmd, OpenalexCmd, OutputOpts, SemanticscholarCmd, StoreCmd};
use std::io::Write as IoWrite;
use braincrawl_cli::pdf_text;
use std::fmt::Write as _;
use braincrawl_cli::config::Config;
use braincrawl_cli::fetch_content;
use braincrawl_cli::openalex::client::OpenAlexClient;
use braincrawl_cli::openalex::OpenAlexProvider;
use braincrawl_cli::output::{Envelope, QueryMeta, render};
use braincrawl_cli::provider::{Emission, Provider, ProviderCmd, PushSummary};
use braincrawl_cli::semanticscholar::client::SemanticScholarClient;
use braincrawl_cli::semanticscholar::SemanticScholarProvider;
use braincrawl_cli::refs_backfill::crossref::CrossrefClient;
use braincrawl_cli::refs_backfill::mapping::{doi_edges, extract_doi_from_work};
use braincrawl_cli::refs_backfill::opencitations::OpenCitationsClient;
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
        Namespace::L3(l3) => {
            braincrawl_cli::l3::dispatch(l3.cmd, &config, &opts)?;
        }
        Namespace::Openalex(oa) => {
            let provider = OpenAlexProvider::new(OpenAlexClient::new(config.openalex_api_key));
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let cmd = match oa.cmd {
                OpenalexCmd::Get { id } => ProviderCmd::Get { id },
                OpenalexCmd::Search { entity, query } => {
                    ProviderCmd::Search { entity: Some(entity), query }
                }
                OpenalexCmd::Find { entity, filters } => ProviderCmd::Find { entity, filters },
                OpenalexCmd::Autocomplete { entity, q } => ProviderCmd::Autocomplete { entity, q },
                OpenalexCmd::CitedBy { id } => ProviderCmd::CitedBy { id },
                OpenalexCmd::Refs { id } => ProviderCmd::Refs { id },
            };
            run_provider(&provider, cmd, &store, &opts)?;
        }
        Namespace::Semanticscholar(s2) => {
            let provider = SemanticScholarProvider::new(SemanticScholarClient::new(config.semanticscholar_api_key));
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let cmd = match s2.cmd {
                SemanticscholarCmd::Get { id } => ProviderCmd::Get { id },
                SemanticscholarCmd::Search { entity, query } => {
                    ProviderCmd::Search { entity: Some(entity), query }
                }
                SemanticscholarCmd::CitedBy { id } => ProviderCmd::CitedBy { id },
                SemanticscholarCmd::Refs { id } => ProviderCmd::Refs { id },
            };
            run_provider(&provider, cmd, &store, &opts)?;
        }
        Namespace::Crossref(cr) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let cr_client = CrossrefClient::new(config.crossref_mailto.clone());
            match cr.cmd {
                CrossrefCmd::Refs { id } => {
                    let Some(work) = store.get_work(&id)? else {
                        return Err(format!("work not found in store: {id}").into());
                    };
                    let Some(citing_doi) = extract_doi_from_work(&work) else {
                        return Err(format!(
                            "no DOI known for {id} — Crossref/OpenCitations are DOI-keyed"
                        )
                        .into());
                    };
                    let (cited_dois, skipped) = cr_client.get_references(&citing_doi)?;
                    let edges = doi_edges(&citing_doi, &cited_dois, "crossref");
                    let count = cited_dois.len() as u64;
                    let results: Vec<serde_json::Value> = cited_dois
                        .iter()
                        .map(|d| serde_json::json!({"id": format!("doi:{d}")}))
                        .collect();
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("crossref:refs".to_string()),
                            resolved_filter: Some(format!("doi:{citing_doi}")),
                            url: None,
                        },
                        count,
                        returned: results.len(),
                        truncated: false,
                        next_cursor: None,
                        results,
                    };
                    render(&envelope, &opts);
                    if !opts.skip_push {
                        match store.put_edges(&edges) {
                            Ok(n) => eprintln!(
                                "push: {} edge(s) stored, {} reference(s) skipped (no DOI)",
                                n, skipped
                            ),
                            Err(e) => {
                                eprintln!("push error: {e}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!(
                            "skip-push: {} edge(s) found, {} reference(s) skipped (no DOI)",
                            edges.len(),
                            skipped
                        );
                    }
                }
            }
        }
        Namespace::Opencitations(oc) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let oc_client = OpenCitationsClient::new();
            match oc.cmd {
                OpencitationsCmd::Refs { id } => {
                    let Some(work) = store.get_work(&id)? else {
                        return Err(format!("work not found in store: {id}").into());
                    };
                    let Some(citing_doi) = extract_doi_from_work(&work) else {
                        return Err(format!(
                            "no DOI known for {id} — Crossref/OpenCitations are DOI-keyed"
                        )
                        .into());
                    };
                    let cited_dois = oc_client.get_references(&citing_doi)?;
                    let edges = doi_edges(&citing_doi, &cited_dois, "opencitations");
                    let count = cited_dois.len() as u64;
                    let results: Vec<serde_json::Value> = cited_dois
                        .iter()
                        .map(|d| serde_json::json!({"id": format!("doi:{d}")}))
                        .collect();
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("opencitations:refs".to_string()),
                            resolved_filter: Some(format!("doi:{citing_doi}")),
                            url: None,
                        },
                        count,
                        returned: results.len(),
                        truncated: false,
                        next_cursor: None,
                        results,
                    };
                    render(&envelope, &opts);
                    if !opts.skip_push {
                        match store.put_edges(&edges) {
                            Ok(n) => eprintln!("push: {} edge(s) stored", n),
                            Err(e) => {
                                eprintln!("push error: {e}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("skip-push: {} edge(s) found", edges.len());
                    }
                }
            }
        }
        Namespace::ExtractText(args) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            use braincrawl_cli::store_client::ContentOutcome;
            match store.get_content(&args.id, "fulltext")? {
                ContentOutcome::Bytes { bytes, mime } => {
                    if !mime.contains("pdf") && !bytes.starts_with(b"%PDF") {
                        return Err(format!(
                            "fulltext artifact for {} is not a PDF (mime={})",
                            args.id, mime
                        )
                        .into());
                    }
                    let text = pdf_text::extract_text(&bytes)?;
                    print!("{text}");
                    eprintln!("extracted: {} chars", text.len());
                }
                ContentOutcome::Absent => {
                    return Err(format!(
                        "no fulltext artifact in store for {}; run fetch-content first",
                        args.id
                    )
                    .into());
                }
                ContentOutcome::Pending => {
                    return Err(format!(
                        "fulltext for {} is still being fetched",
                        args.id
                    )
                    .into());
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
                    println!("already-present: fulltext artifact already in store (use --force to re-fetch)");
                }
                fetch_content::Outcome::NoOaFound { reason } => {
                    return Err(format!("no-oa-found: {}", reason).into());
                }
            }
        }
        Namespace::FetchPdf(args) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let source = match args.from.as_str() {
                "openalex" => fetch_content::Source::Openalex,
                "unpaywall" => fetch_content::Source::Unpaywall,
                _ => fetch_content::Source::Auto,
            };
            let (bytes, mime, url) = fetch_content::fetch_artifact_bytes(
                &store,
                config.unpaywall_email.as_deref(),
                &args.id,
                source,
                args.require_pdf,
            )?;
            if let Some(path) = &args.output {
                std::fs::write(path, &bytes)?;
            } else {
                std::io::stdout().lock().write_all(&bytes)?;
            }
            eprintln!("fetched: {} bytes, mime={}, url={}", bytes.len(), mime, url);
        }
        Namespace::PushPdf(args) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let bytes: Vec<u8> = if let Some(path) = &args.file {
                std::fs::read(path)?
            } else {
                use std::io::Read as _;
                let mut buf = Vec::new();
                std::io::stdin().lock().read_to_end(&mut buf)?;
                buf
            };
            if bytes.is_empty() {
                return Err("no input bytes (provide a file arg or pipe bytes on stdin)".into());
            }
            let mime = args.mime.as_deref().unwrap_or_else(|| fetch_content::sniff_mime(&bytes));
            store.put_content(
                &args.id,
                "fulltext",
                bytes.clone(),
                mime,
                args.source.as_deref(),
                args.source_url.as_deref(),
            )?;
            eprintln!("pushed: {} bytes, mime={}", bytes.len(), mime);
        }
        Namespace::Arxiv(arxiv) => {
            let provider = ArxivProvider::new();
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            let cmd = match arxiv.cmd {
                ArxivCmd::Get { id } => braincrawl_cli::provider::ProviderCmd::Get { id },
                ArxivCmd::Search { query } => braincrawl_cli::provider::ProviderCmd::Search {
                    entity: None,
                    query,
                },
            };
            run_provider(&provider, cmd, &store, &opts)?;
        }
        Namespace::GetPdf(args) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            use braincrawl_cli::store_client::ContentOutcome;
            match store.get_content(&args.id, "fulltext")? {
                ContentOutcome::Bytes { bytes, mime } => {
                    if let Some(path) = &args.output {
                        std::fs::write(path, &bytes)?;
                    } else {
                        std::io::stdout().lock().write_all(&bytes)?;
                    }
                    eprintln!("read: {} bytes, mime={}", bytes.len(), mime);
                }
                ContentOutcome::Absent => {
                    return Err(format!(
                        "no fulltext artifact in store for {}; run fetch-content first",
                        args.id
                    )
                    .into());
                }
                ContentOutcome::Pending => {
                    return Err(format!(
                        "fulltext for {} is still being fetched",
                        args.id
                    )
                    .into());
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

fn run_provider(
    p: &dyn Provider,
    cmd: ProviderCmd,
    store: &StoreClient,
    opts: &OutputOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    let (envelope, emission) = p.dispatch(cmd, opts)?;
    render(&envelope, opts);
    if !opts.skip_push {
        let summary = push_emission(store, &emission);
        report_push_summary(&summary);
        if !summary.errors.is_empty() {
            std::process::exit(1);
        }
    }
    Ok(())
}

fn push_emission(store: &StoreClient, em: &Emission) -> PushSummary {
    let mut nodes_pushed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for work_record in &em.records {
        match serde_json::to_value(work_record) {
            Err(e) => errors.push(format!("serialize failed: {e}")),
            Ok(v) => match store.put_work(&v) {
                Ok(_) => nodes_pushed += 1,
                Err(e) => errors.push(format!("push node failed: {e}")),
            },
        }
    }

    let mut edges_pushed = 0u64;
    if !em.edges.is_empty() {
        let edge_values: Vec<serde_json::Value> = em
            .edges
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        match store.put_edges(&edge_values) {
            Ok(n) => edges_pushed = n,
            Err(e) => errors.push(format!("push edges failed: {e}")),
        }
    }

    PushSummary {
        nodes_pushed,
        edges_pushed,
        skipped_unmappable: em.skipped_unmappable,
        errors,
    }
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

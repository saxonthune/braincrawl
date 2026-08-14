use braincrawl_cli::arxiv::ArxivProvider;
use braincrawl_cli::cli::{ArxivCmd, CatalogCmd, ChunkArgs, Cli, CrossrefCmd, LibraryCmd, Namespace, OpencitationsCmd, OpenalexCmd, OutlineArgs, OutputOpts, PaginateArgs, ReadArgs, SemanticscholarCmd, StoreCmd};
use std::io::Write as IoWrite;
use braincrawl_cli::chunk;
use braincrawl_cli::locator::{self, Locator, PageRange};
use braincrawl_cli::outline::{self, Outline};
use braincrawl_cli::pages::{self, Pages};
use braincrawl_cli::pdf_text;
use std::fmt::Write as _;
use braincrawl_cli::config::{Config, StoreRef};
use braincrawl_cli::fetch_content;
use braincrawl_cli::storesync;
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

/// The version currently held for `role` on `id`, or `None` if the role holds
/// nothing (or the work is unknown) — used to record an accurate `derived_from`.
fn current_artifact_version(store: &StoreClient, id: &str, role: &str) -> Option<u32> {
    use braincrawl_cli::store_client::ArtifactListing;
    match store.list_artifacts(id, Some(role), false) {
        Ok(ArtifactListing::Held(list)) => list
            .iter()
            .find(|a| a["role"] == role)
            .and_then(|a| a["version"].as_u64())
            .map(|v| v as u32),
        _ => None,
    }
}

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
        Namespace::Catalog(catalog) => {
            let client = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            match catalog.cmd {
                CatalogCmd::Have { ids } => {
                    let present = client.have(&ids)?;
                    let count = present.len() as u64;
                    let results = present
                        .into_iter()
                        .map(|s| serde_json::json!({ "id": s }))
                        .collect();
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("catalog:have".to_string()),
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
                CatalogCmd::Get { id } => {
                    let result = client.get_work(&id)?;
                    let results = result.map(|v| vec![v]).unwrap_or_default();
                    let count = results.len() as u64;
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("catalog:get".to_string()),
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
                CatalogCmd::Neighborhood { seeds, dir, depth, max_nodes } => {
                    let value = client.neighborhood(&seeds, &dir, depth, max_nodes)?;
                    render_neighborhood(&value, &opts);
                }
                CatalogCmd::Stats => {
                    let value = client.stats()?;
                    render_stats(&value, &opts);
                }
                CatalogCmd::Put { file } => {
                    let bytes: Vec<u8> = if let Some(path) = &file {
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
                    let source = file.as_deref().unwrap_or("stdin");
                    let emission: Emission = serde_json::from_slice(&bytes)
                        .map_err(|e| format!("failed to parse emission from {source}: {e}"))?;
                    let summary = push_emission(&client, &emission);
                    report_push_summary(&summary);
                    if !summary.errors.is_empty() {
                        std::process::exit(1);
                    }
                }
                CatalogCmd::Search { author, title, year, with_artifact } => {
                    if author.is_none() && title.is_none() && year.is_none() && with_artifact.is_none() {
                        return Err(
                            "catalog search needs at least one of --author, --title, --year, --with-artifact".into(),
                        );
                    }
                    let limit = if opts.all { 200 } else { opts.limit.unwrap_or(25) as u32 };
                    let results = client.search_works(
                        author.as_deref(),
                        title.as_deref(),
                        year,
                        with_artifact.as_deref(),
                        limit,
                    )?;
                    let filter_desc: Vec<String> = [
                        author.map(|a| format!("author~{a}")),
                        title.map(|t| format!("title~{t}")),
                        year.map(|y| format!("year={y}")),
                        with_artifact.map(|r| format!("with-artifact={r}")),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    let count = results.len() as u64;
                    let envelope = Envelope {
                        query: QueryMeta {
                            entity: Some("catalog:search".to_string()),
                            resolved_filter: Some(filter_desc.join(" ")),
                            url: None,
                        },
                        count,
                        returned: results.len(),
                        truncated: count == limit as u64,
                        next_cursor: None,
                        results,
                    };
                    render(&envelope, &opts);
                }
                CatalogCmd::Add { aliases, title, authors, year, kind } => {
                    let record = braincrawl_cli::provider::build_manual_work_record(
                        &aliases, title, authors, year, &kind,
                    )?;
                    let emission = Emission {
                        records: vec![record],
                        edges: vec![],
                        skipped_unmappable: 0,
                    };
                    let summary = push_emission(&client, &emission);
                    report_push_summary(&summary);
                    if !summary.errors.is_empty() {
                        std::process::exit(1);
                    }
                    for id in &summary.pushed_ids {
                        println!("{id}");
                    }
                }
            }
        }
        Namespace::Collection(collection) => {
            braincrawl_cli::l3::dispatch(collection.cmd, &config, &opts)?;
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
        Namespace::Library(library) => {
            let store = StoreClient::new(&config.server_url)
                .with_token(config.auth_token.clone());
            match library.cmd {
                LibraryCmd::ExtractText(args) => {
                    use braincrawl_cli::store_client::ContentOutcome;
                    match store.get_content(&args.id, &args.from)? {
                        ContentOutcome::Bytes { bytes, mime } => {
                            if !mime.contains("pdf") && !bytes.starts_with(b"%PDF") {
                                return Err(format!(
                                    "{} artifact for {} is not a PDF (mime={})",
                                    args.from, args.id, mime
                                )
                                .into());
                            }
                            let text = pdf_text::extract_text(&bytes)?;
                            if args.stdout {
                                print!("{text}");
                                eprintln!("extracted: {} chars", text.len());
                            } else {
                                if !args.force {
                                    if let Ok(ContentOutcome::Bytes { .. }) =
                                        store.get_content(&args.id, &args.role)
                                    {
                                        eprintln!(
                                            "already-present: {} artifact already in store (use --force to re-extract)",
                                            args.role
                                        );
                                        return Ok(());
                                    }
                                }
                                let text_len = text.len();
                                let derived_from = current_artifact_version(&store, &args.id, &args.from)
                                    .map(|v| (args.from.as_str(), v));
                                store.put_content_with_fetched_at(
                                    &args.id,
                                    &args.role,
                                    text.into_bytes(),
                                    "text/plain",
                                    Some("extract-text"),
                                    None,
                                    &braincrawl_cli::store_client::rfc3339_now(),
                                    derived_from,
                                )?;
                                eprintln!("extracted: {} chars, stored at role {}", text_len, args.role);
                            }
                        }
                        ContentOutcome::Absent => {
                            return Err(format!(
                                "no {} artifact in store for {}; run library fetch first",
                                args.from, args.id
                            )
                            .into());
                        }
                        ContentOutcome::Pending => {
                            return Err(format!(
                                "{} for {} is still being fetched",
                                args.from, args.id
                            )
                            .into());
                        }
                    }
                }
                LibraryCmd::Fetch(fc) => {
                    let source = match fc.from.as_str() {
                        "openalex" => fetch_content::Source::Openalex,
                        "unpaywall" => fetch_content::Source::Unpaywall,
                        _ => fetch_content::Source::Auto,
                    };
                    if fc.stdout || fc.output.is_some() {
                        let (bytes, mime, url) = fetch_content::fetch_artifact_bytes(
                            &store,
                            config.unpaywall_email.as_deref(),
                            &fc.id,
                            source,
                            fc.require_pdf,
                        )?;
                        if let Some(path) = &fc.output {
                            std::fs::write(path, &bytes)?;
                        } else {
                            std::io::stdout().lock().write_all(&bytes)?;
                        }
                        eprintln!("fetched: {} bytes, mime={}, url={}", bytes.len(), mime, url);
                    } else {
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
                }
                LibraryCmd::Put(args) => {
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
                        &args.role,
                        bytes.clone(),
                        mime,
                        args.source.as_deref(),
                        args.source_url.as_deref(),
                    )?;
                    eprintln!("pushed: {} bytes, mime={}", bytes.len(), mime);
                }
                LibraryCmd::Get(args) => {
                    use braincrawl_cli::store_client::ContentOutcome;
                    match store.get_content(&args.id, &args.role)? {
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
                                "no artifact with role '{}' in store for {}",
                                args.role, args.id
                            )
                            .into());
                        }
                        ContentOutcome::Pending => {
                            return Err(format!(
                                "artifact with role '{}' for {} is still being fetched",
                                args.role, args.id
                            )
                            .into());
                        }
                    }
                }
                LibraryCmd::Chunk(args) => {
                    run_chunk(&config, &args)?;
                }
                LibraryCmd::Paginate(args) => {
                    run_paginate(&store, &args)?;
                }
                LibraryCmd::Outline(args) => {
                    run_outline(&store, &args)?;
                }
                LibraryCmd::Read(args) => {
                    run_read(&store, &args)?;
                }
                LibraryCmd::List(args) => {
                    use braincrawl_cli::store_client::ArtifactListing;
                    let mut results = match store.list_artifacts(
                        &args.id,
                        args.role.as_deref(),
                        args.all_versions,
                    )? {
                        ArtifactListing::Held(results) => results,
                        ArtifactListing::UnknownWork => {
                            return Err(format!(
                                "unknown work: nothing in the Catalog under '{}' \
                                 (a work that is present but holds no artifacts \
                                 lists zero results instead)",
                                args.id
                            )
                            .into());
                        }
                    };
                    for artifact in &mut results {
                        if let Some(obj) = artifact.as_object_mut() {
                            obj.remove("r2_key");
                            obj.remove("content_hash");
                        }
                    }
                    let count = results.len() as u64;
                    if opts.text && !opts.json {
                        print_artifact_tree(&results);
                    } else {
                        let ordered = artifact_tree_order(&results)
                            .into_iter()
                            .map(|(_depth, a, _dangling)| a.clone())
                            .collect();
                        let envelope = Envelope {
                            query: QueryMeta {
                                entity: Some("library:list".to_string()),
                                resolved_filter: Some(args.id),
                                url: None,
                            },
                            count,
                            returned: results.len(),
                            truncated: false,
                            next_cursor: None,
                            results: ordered,
                        };
                        render(&envelope, &opts);
                    }
                }
            }
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
        Namespace::Web => {
            braincrawl_cli::web::print_url(&config);
        }
        Namespace::Doctor => {
            let checks = braincrawl_cli::doctor::run(&config);
            braincrawl_cli::doctor::render(&checks, &opts);
            if braincrawl_cli::doctor::any_failed(&checks) {
                std::process::exit(1);
            }
        }
        Namespace::Store(store_args) => {
            run_store_cmd(store_args.cmd, &config, &opts)?;
        }
        Namespace::Rename(args) => {
            braincrawl_cli::rename::run(args, &opts)?;
        }
    }

    Ok(())
}

fn run_chunk(config: &Config, args: &ChunkArgs) -> Result<(), Box<dyn std::error::Error>> {
    use braincrawl_cli::store_client::ContentOutcome;

    if args.id.is_some() == args.file.is_some() {
        return Err(
            "chunk requires exactly one of a work id or --file (not both, not neither)".into(),
        );
    }

    let store = StoreClient::new(&config.server_url).with_token(config.auth_token.clone());

    let bytes: Vec<u8> = if let Some(path) = &args.file {
        std::fs::read(path)?
    } else {
        let id = args.id.as_ref().unwrap();
        match store.get_content(id, &args.from)? {
            ContentOutcome::Bytes { bytes, mime } => {
                if !mime.contains("pdf") && !bytes.starts_with(b"%PDF") {
                    return Err(format!(
                        "{} artifact for {} is not a PDF (mime={})",
                        args.from, id, mime
                    )
                    .into());
                }
                bytes
            }
            ContentOutcome::Absent => {
                return Err(format!(
                    "no {} artifact in store for {}; run fetch-content first",
                    args.from, id
                )
                .into());
            }
            ContentOutcome::Pending => {
                return Err(format!("{} for {} is still being fetched", args.from, id).into());
            }
        }
    };

    let pages = pdf_text::extract_pages(&bytes)?;
    let pages_total = pages.len();
    let unfaithful = chunk::unfaithful_pages(&pages);

    if !unfaithful.is_empty() && !args.allow_partial {
        return Err(format!(
            "{}/{} pages have no text layer (scanned/image-only); chunk provides no OCR. \
             Re-run with --allow-partial to chunk the rest.",
            unfaithful.len(),
            pages_total
        )
        .into());
    }

    let mut working_pages = pages;
    for &page_num in &unfaithful {
        working_pages[page_num - 1] = String::new();
    }

    let chunks = chunk::chunk_pages(&working_pages, args.max_tokens, args.overlap);
    let pages_faithful = pages_total - unfaithful.len();
    let chunk_count = chunks.len();

    let output = serde_json::json!({
        "work": args.id,
        "chunker": {
            "max_tokens": args.max_tokens,
            "overlap": args.overlap,
            "tokenizer": "cl100k_base",
        },
        "pages_total": pages_total,
        "pages_faithful": pages_faithful,
        "pages_skipped": unfaithful,
        "chunks": chunks,
    });

    if args.stdout || args.file.is_some() {
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let id = args.id.as_ref().unwrap();
    if !args.force {
        if let Ok(ContentOutcome::Bytes { .. }) = store.get_content(id, &args.role) {
            eprintln!(
                "already-present: {} artifact already in store (use --force to re-chunk)",
                args.role
            );
            return Ok(());
        }
    }

    let json_bytes = serde_json::to_vec(&output)?;
    // `--file` reads an external PDF with nothing stored under `--from`, so there is
    // no parent artifact to record; only the store-backed `id` path has one.
    let derived_from = if args.file.is_some() {
        None
    } else {
        current_artifact_version(&store, id, &args.from).map(|v| (args.from.as_str(), v))
    };
    store.put_content_with_fetched_at(
        id,
        &args.role,
        json_bytes,
        "application/json",
        Some("chunk"),
        None,
        &braincrawl_cli::store_client::rfc3339_now(),
        derived_from,
    )?;
    eprintln!(
        "chunked: {} chunk(s), pages_faithful {}/{}",
        chunk_count, pages_faithful, pages_total
    );
    Ok(())
}

fn run_paginate(store: &StoreClient, args: &PaginateArgs) -> Result<(), Box<dyn std::error::Error>> {
    use braincrawl_cli::store_client::ContentOutcome;

    if !args.force {
        if let Ok(ContentOutcome::Bytes { .. }) = store.get_content(&args.id, &args.role) {
            eprintln!(
                "already-present: {} artifact already in store (use --force to re-paginate)",
                args.role
            );
            return Ok(());
        }
    }

    let bytes = match store.get_content(&args.id, &args.from)? {
        ContentOutcome::Bytes { bytes, mime } => {
            if !mime.contains("pdf") && !bytes.starts_with(b"%PDF") {
                return Err(format!(
                    "{} artifact for {} is not a PDF (mime={})",
                    args.from, args.id, mime
                )
                .into());
            }
            bytes
        }
        ContentOutcome::Absent => {
            return Err(format!(
                "no {} artifact in store for {}; run library fetch first",
                args.from, args.id
            )
            .into());
        }
        ContentOutcome::Pending => {
            return Err(format!("{} for {} is still being fetched", args.from, args.id).into());
        }
    };

    let raw_pages = pdf_text::extract_pages(&bytes)?;
    let page_count = raw_pages.len();
    let anchored = !args.anchors.is_empty();
    let records = if anchored {
        pages::anchor_folios(&raw_pages, &args.anchors)
    } else {
        pages::detect_folios(&raw_pages)
    };
    let breaks = if anchored { Vec::new() } else { pages::validate_folios(&records) };

    if !breaks.is_empty() {
        let gap_pages: usize = breaks
            .iter()
            .map(|b| match b {
                pages::FolioBreak::Gap { pdf_pages } => pdf_pages.len(),
                pages::FolioBreak::Contradiction { .. } => 0,
            })
            .sum();
        let contradictions: Vec<usize> = breaks
            .iter()
            .filter_map(|b| match b {
                pages::FolioBreak::Contradiction { pdf_page, .. } => Some(*pdf_page),
                pages::FolioBreak::Gap { .. } => None,
            })
            .collect();
        eprintln!(
            "folio breaks: {} gap page(s), {} contradiction(s) at pdf page(s) {:?}",
            gap_pages,
            contradictions.len(),
            contradictions
        );
    }

    let folio_method = match (anchored, records.iter().any(|r| r.folio.is_some())) {
        (true, _) => pages::FolioMethod::Anchored,
        (false, true) => pages::FolioMethod::Detected,
        (false, false) => pages::FolioMethod::None,
    };

    // Detection that read no folio at all is a failure wearing a success's
    // clothes: every page-addressed read against the result would miss. Refuse
    // it by default rather than store a mapping the operator cannot use.
    if folio_method == pages::FolioMethod::None && !args.allow_no_folios {
        return Err(format!(
            "no folio could be read from any of the {page_count} page(s) of {}. \
             Declare the mapping instead, e.g. --anchor 1=1 --anchor 2=3 \
             (each anchor governs pages up to the next). \
             Pass --allow-no-folios to store a pages artifact addressable only by pdf page.",
            args.from
        )
        .into());
    }

    for run in pages::folio_runs(&records) {
        eprintln!("  {run}");
    }
    let unmapped = records.iter().filter(|r| r.folio.is_none()).count();
    if unmapped > 0 {
        eprintln!("  {unmapped} page(s) with no folio");
    }

    let pages_artifact = Pages {
        source_role: args.from.clone(),
        page_count,
        folio_method,
        pages: records,
    };

    if args.stdout {
        println!("{}", serde_json::to_string_pretty(&pages_artifact)?);
        return Ok(());
    }

    let json_bytes = serde_json::to_vec(&pages_artifact)?;
    let derived_from = current_artifact_version(store, &args.id, &args.from).map(|v| (args.from.as_str(), v));
    store.put_content_with_fetched_at(
        &args.id,
        &args.role,
        json_bytes,
        "application/json",
        Some("paginate"),
        None,
        &braincrawl_cli::store_client::rfc3339_now(),
        derived_from,
    )?;
    eprintln!("paginated: {} page(s), stored at role {}", page_count, args.role);
    Ok(())
}

fn run_outline(store: &StoreClient, args: &OutlineArgs) -> Result<(), Box<dyn std::error::Error>> {
    use braincrawl_cli::store_client::ContentOutcome;

    if !args.force {
        if let Ok(ContentOutcome::Bytes { .. }) = store.get_content(&args.id, &args.role) {
            eprintln!(
                "already-present: {} artifact already in store (use --force to overwrite)",
                args.role
            );
            return Ok(());
        }
    }

    let raw = std::fs::read(&args.from_file)
        .map_err(|e| format!("failed to read {}: {e}", args.from_file))?;
    let outline_doc: Outline = serde_json::from_slice(&raw)
        .map_err(|e| format!("failed to parse {} as an outline: {e}", args.from_file))?;

    let pages_artifact = match store.get_content(&args.id, &outline_doc.source_role)? {
        ContentOutcome::Bytes { bytes, .. } => {
            serde_json::from_slice::<Pages>(&bytes).map_err(|e| format!("stored {} artifact is not a valid pages document: {e}", outline_doc.source_role))?
        }
        ContentOutcome::Absent => {
            return Err(format!(
                "no {} artifact in store for {}; run library paginate first",
                outline_doc.source_role, args.id
            )
            .into());
        }
        ContentOutcome::Pending => {
            return Err(format!("{} for {} is still being fetched", outline_doc.source_role, args.id).into());
        }
    };

    if let Err(errors) = outline::validate(&outline_doc, pages_artifact.page_count) {
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("outline validation failed:\n  {}", messages.join("\n  ")).into());
    }

    let section_count = outline_doc.sections.len();
    let json_bytes = serde_json::to_vec(&outline_doc)?;
    let derived_from = current_artifact_version(store, &args.id, &outline_doc.source_role)
        .map(|v| (outline_doc.source_role.as_str(), v));
    store.put_content_with_fetched_at(
        &args.id,
        &args.role,
        json_bytes,
        "application/json",
        Some("outline"),
        None,
        &braincrawl_cli::store_client::rfc3339_now(),
        derived_from,
    )?;
    eprintln!("outlined: {} section(s), stored at role {}", section_count, args.role);
    Ok(())
}

fn parse_range(s: &str) -> PageRange {
    match s.split_once('-') {
        Some((a, b)) => PageRange { start: a.trim().to_string(), end: b.trim().to_string() },
        None => PageRange { start: s.trim().to_string(), end: s.trim().to_string() },
    }
}

fn run_read(store: &StoreClient, args: &ReadArgs) -> Result<(), Box<dyn std::error::Error>> {
    use braincrawl_cli::store_client::ContentOutcome;

    let pages_artifact: Pages = match store.get_content(&args.id, &args.pages_role)? {
        ContentOutcome::Bytes { bytes, .. } => serde_json::from_slice(&bytes)
            .map_err(|e| format!("stored {} artifact is not a valid pages document: {e}", args.pages_role))?,
        ContentOutcome::Absent => {
            return Err(format!(
                "no {} artifact in store for {}; run library paginate first",
                args.pages_role, args.id
            )
            .into());
        }
        ContentOutcome::Pending => {
            return Err(format!("{} for {} is still being fetched", args.pages_role, args.id).into());
        }
    };

    let outline_artifact: Option<Outline> = match store.get_content(&args.id, &args.outline_role) {
        Ok(ContentOutcome::Bytes { bytes, .. }) => Some(
            serde_json::from_slice(&bytes)
                .map_err(|e| format!("stored {} artifact is not a valid outline document: {e}", args.outline_role))?,
        ),
        _ => None,
    };

    let locator = if let Some(printed) = &args.printed {
        Locator::Printed(parse_range(printed))
    } else if let Some(pdf) = &args.pdf {
        Locator::Pdf(parse_range(pdf))
    } else if let Some(section) = &args.section {
        Locator::Section { id: section.clone(), head: args.first, tail: args.last }
    } else if let Some(find) = &args.find {
        Locator::Quote(find.clone())
    } else {
        unreachable!("clap enforces exactly one of --printed/--pdf/--section/--find")
    };

    let span = locator::resolve(&locator, &pages_artifact, outline_artifact.as_ref())?;

    let by_pdf_page: std::collections::HashMap<usize, &pages::PageRecord> =
        pages_artifact.pages.iter().map(|p| (p.pdf_page, p)).collect();

    for pdf_page in &span.pdf_pages {
        let Some(record) = by_pdf_page.get(pdf_page) else { continue };
        match &record.folio {
            Some(folio) => println!("=== p.{folio} (pdf {pdf_page}) ==="),
            None => println!("=== pdf {pdf_page} (no folio) ==="),
        }
        println!("{}", record.text);
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

fn client_for(store: &StoreRef) -> StoreClient {
    StoreClient::new(&store.url).with_token(store.auth_token.clone())
}

fn store_label(store: &StoreRef) -> String {
    match &store.name {
        Some(name) => name.clone(),
        None => store.url.clone(),
    }
}

fn run_store_cmd(
    cmd: StoreCmd,
    config: &Config,
    opts: &OutputOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        StoreCmd::List => {
            if config.stores.is_empty() {
                eprintln!("no named stores configured — add [stores.<name>] tables (url, auth_token) to the config file");
                eprintln!("effective server_url: {}", config.server_url);
                return Ok(());
            }
            for (name, store) in &config.stores {
                let marker = if config.active_store.as_deref() == Some(name) { "*" } else { " " };
                let token = if store.auth_token.is_some() { "token" } else { "no token" };
                println!("{marker} {name:<12} {} ({token})", store.url);
            }
            if config.active_store.is_none() {
                eprintln!("no active_store set — commands use server_url: {}", config.server_url);
            }
            Ok(())
        }
        StoreCmd::Use { name } => {
            let target = config
                .stores
                .get(&name)
                .ok_or_else(|| format!("unknown store '{name}' — run `braincrawl store list`"))?
                .clone();
            let previous = config.active_store_ref();
            let path = Config::set_active_store(&name)?;
            eprintln!("active store: {name} ({}) — written to {}", target.url, path.display());
            // Show what each side holds so switching away from data is visible.
            let mut sides = vec![&target];
            if previous.url.trim_end_matches('/') != target.url.trim_end_matches('/') {
                sides.push(&previous);
            }
            for store in sides {
                match client_for(store).stats() {
                    Ok(stats) => eprintln!(
                        "{}: {} works, {} edges, {} nodes",
                        store_label(store),
                        stats["works"].as_u64().unwrap_or(0),
                        stats["edges_total"].as_u64().unwrap_or(0),
                        stats["nodes_total"].as_u64().unwrap_or(0),
                    ),
                    Err(_) => eprintln!("{}: unreachable", store_label(store)),
                }
            }
            Ok(())
        }
        StoreCmd::Diff { a, b } => {
            let a_ref = config.store_ref(&a)?;
            let b_ref = match b {
                Some(spec) => config.store_ref(&spec)?,
                None => config.active_store_ref(),
            };
            let report = storesync::diff(
                &client_for(&a_ref),
                &store_label(&a_ref),
                &client_for(&b_ref),
                &store_label(&b_ref),
            )?;
            storesync::print_diff(&report);
            let l3 = storesync::l3_diff(
                &client_for(&a_ref),
                &store_label(&a_ref),
                &client_for(&b_ref),
                &store_label(&b_ref),
            )?;
            storesync::print_l3_diff(&l3, &store_label(&a_ref), &store_label(&b_ref));
            Ok(())
        }
        StoreCmd::Sync { source, dest, dry_run, doc, force } => {
            let src_ref = config.store_ref(&source)?;
            let dst_ref = config.store_ref(&dest)?;
            if src_ref.url.trim_end_matches('/') == dst_ref.url.trim_end_matches('/') {
                return Err("source and destination are the same store".into());
            }
            eprintln!(
                "sync {} → {}{}{}",
                store_label(&src_ref),
                store_label(&dst_ref),
                doc.as_deref().map(|d| format!(" (doc {d})")).unwrap_or_default(),
                if dry_run { " (dry run)" } else { "" }
            );
            // A --doc run is a targeted Research Document transfer; the
            // catalog phases don't apply.
            if doc.is_none() {
                let report =
                    storesync::sync(&client_for(&src_ref), &client_for(&dst_ref), dry_run)?;
                storesync::print_report(&report, dry_run);
            }
            let l3_report = storesync::l3_sync(
                &client_for(&src_ref),
                &src_ref.url,
                &client_for(&dst_ref),
                &dst_ref.url,
                dry_run,
                doc.as_deref(),
                force,
            )?;
            storesync::print_l3_report(&l3_report, dry_run);
            if !dry_run && doc.is_none() {
                let stats = client_for(&dst_ref).stats()?;
                eprintln!("--- destination stats ---");
                render_stats(&stats, opts);
            }
            Ok(())
        }
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
    let _ = writeln!(out);
    let _ = writeln!(out, "storage:");
    let _ = writeln!(out, "  library:        {}", human_bytes(n("library_bytes")));
    let _ = writeln!(out, "  catalog:        {}", human_bytes(n("catalog_bytes")));
    let _ = writeln!(out, "  total:          {}", human_bytes(n("total_bytes")));
    print!("{out}");
}

/// Order artifacts into a tree: roots (no `derived_from`, or a `derived_from`
/// that names a (role, version) absent from `artifacts`) first in role order,
/// each immediately followed by its own children recursively, also in role
/// order. `dangling` is `Some(parent_label)` when the entry's recorded parent
/// was not found among `artifacts` — it is still shown at depth 0.
fn artifact_tree_order(
    artifacts: &[serde_json::Value],
) -> Vec<(usize, &serde_json::Value, Option<String>)> {
    let key = |a: &serde_json::Value| -> Option<(String, u64)> {
        Some((a["role"].as_str()?.to_string(), a["version"].as_u64()?))
    };
    let present: std::collections::HashSet<(String, u64)> =
        artifacts.iter().filter_map(key).collect();

    let parent_key = |a: &serde_json::Value| -> Option<(String, u64)> {
        let role = a["derived_from_role"].as_str()?;
        let version = a["derived_from_version"].as_u64()?;
        Some((role.to_string(), version))
    };

    let mut children: std::collections::HashMap<(String, u64), Vec<&serde_json::Value>> =
        std::collections::HashMap::new();
    let mut roots: Vec<(&serde_json::Value, Option<String>)> = Vec::new();

    for a in artifacts {
        match parent_key(a) {
            Some(pk) if present.contains(&pk) => {
                children.entry(pk).or_default().push(a);
            }
            Some((role, version)) => {
                roots.push((a, Some(format!("{role} v{version}"))));
            }
            None => roots.push((a, None)),
        }
    }

    let sort_key = |a: &&serde_json::Value| {
        (
            a["role"].as_str().unwrap_or("").to_string(),
            std::cmp::Reverse(a["version"].as_u64().unwrap_or(0)),
        )
    };
    roots.sort_by_key(|(a, _)| sort_key(a));
    for siblings in children.values_mut() {
        siblings.sort_by_key(sort_key);
    }

    let mut out = Vec::new();
    for (root, dangling) in roots {
        push_subtree(root, dangling, 0, &children, &mut out);
    }
    out
}

fn push_subtree<'a>(
    node: &'a serde_json::Value,
    dangling: Option<String>,
    depth: usize,
    children: &std::collections::HashMap<(String, u64), Vec<&'a serde_json::Value>>,
    out: &mut Vec<(usize, &'a serde_json::Value, Option<String>)>,
) {
    out.push((depth, node, dangling));
    if let (Some(role), Some(version)) = (node["role"].as_str(), node["version"].as_u64()) {
        if let Some(kids) = children.get(&(role.to_string(), version)) {
            for kid in kids {
                push_subtree(kid, None, depth + 1, children, out);
            }
        }
    }
}

/// `library list --text` — a tree instead of the flat one-line-per-result
/// rendering the generic envelope printer gives (artifacts have no `id`/`title`
/// for it to display anyway). A dangling parent (pointing at a role/version not
/// present in this listing) is shown at the top level, annotated "unknown".
fn print_artifact_tree(artifacts: &[serde_json::Value]) {
    for (depth, a, dangling) in artifact_tree_order(artifacts) {
        let role = a["role"].as_str().unwrap_or("?");
        let version = a["version"].as_u64().unwrap_or(0);
        let indent = "  ".repeat(depth);
        let marker = if depth == 0 { "" } else { "└─ " };
        match dangling {
            Some(parent) => {
                println!("{indent}{marker}{role} v{version}  (derived from {parent}, unknown)")
            }
            None => println!("{indent}{marker}{role} v{version}"),
        }
    }
}

/// Format a byte count as a human-readable size, e.g. `1.4 MiB (1462272 bytes)`.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {} ({bytes} bytes)", UNITS[unit])
}

fn run_provider(
    p: &dyn Provider,
    cmd: ProviderCmd,
    store: &StoreClient,
    opts: &OutputOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    let (envelope, emission) = p.dispatch(cmd, opts)?;
    if opts.emission {
        println!("{}", serde_json::to_string_pretty(&emission)?);
    } else {
        render(&envelope, opts);
    }
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
    let mut pushed_ids: Vec<String> = Vec::new();

    for work_record in &em.records {
        match serde_json::to_value(work_record) {
            Err(e) => errors.push(format!("serialize failed: {e}")),
            Ok(v) => match store.put_work(&v) {
                Ok(id) => {
                    nodes_pushed += 1;
                    pushed_ids.push(id);
                }
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
        pushed_ids,
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

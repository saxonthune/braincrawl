use braincrawl_cli::cli::{Cli, Namespace, OpenalexCmd, OutputOpts, StoreCmd};
use braincrawl_cli::config::Config;
use braincrawl_cli::openalex::client::OpenAlexClient;
use braincrawl_cli::openalex::verbs;
use braincrawl_cli::output::{Envelope, QueryMeta, render};
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
            let client = StoreClient::new(&config.server_url);
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
        Namespace::Openalex(oa) => {
            let oa_client = OpenAlexClient::new(config.openalex_api_key);
            let (envelope, _push_batch) = match oa.cmd {
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
        }
    }

    Ok(())
}

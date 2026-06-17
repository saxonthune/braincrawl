use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "braincrawl", about = "braincrawl research graph CLI")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub namespace: Namespace,
}

#[derive(Args)]
pub struct GlobalArgs {
    /// Output as JSON (default)
    #[arg(long, group = "format")]
    pub json: bool,
    /// Output as compact text (one result per line)
    #[arg(long, group = "format")]
    pub text: bool,
    /// Maximum number of results to return
    #[arg(long)]
    pub limit: Option<u64>,
    /// Return all results, ignoring limit
    #[arg(long)]
    pub all: bool,
    /// Comma-separated list of fields to include in output
    #[arg(long, value_delimiter = ',')]
    pub fields: Vec<String>,
    /// Return full record (all fields)
    #[arg(long)]
    pub full: bool,
    /// Do not push results to the local store
    #[arg(long)]
    pub skip_push: bool,
}

/// Extension point for provider namespaces; future phases add variants here.
#[derive(Subcommand)]
pub enum Namespace {
    #[command(hide = true, about = "Dev round-trip surface for the local store")]
    Store(StoreArgs),
}

#[derive(Args)]
pub struct StoreArgs {
    #[command(subcommand)]
    pub cmd: StoreCmd,
}

#[derive(Subcommand)]
pub enum StoreCmd {
    /// Check which of the given aliases (ns:value) are present in the store
    Have {
        ids: Vec<String>,
    },
    /// Retrieve a work by alias (ns:value)
    Get {
        id: String,
    },
}

/// Parsed output preferences derived from GlobalArgs.
pub struct OutputOpts {
    pub json: bool,
    pub text: bool,
    pub limit: Option<u64>,
    pub all: bool,
    pub fields: Vec<String>,
    pub full: bool,
    pub skip_push: bool,
}

impl From<GlobalArgs> for OutputOpts {
    fn from(g: GlobalArgs) -> Self {
        OutputOpts {
            json: g.json,
            text: g.text,
            limit: g.limit,
            all: g.all,
            fields: g.fields,
            full: g.full,
            skip_push: g.skip_push,
        }
    }
}

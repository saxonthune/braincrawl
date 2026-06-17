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
    #[arg(long, group = "format", global = true)]
    pub json: bool,
    /// Output as compact text (one result per line)
    #[arg(long, group = "format", global = true)]
    pub text: bool,
    /// Maximum number of results to return
    #[arg(long, global = true)]
    pub limit: Option<u64>,
    /// Return all results, ignoring limit
    #[arg(long, global = true)]
    pub all: bool,
    /// Comma-separated list of fields to include in output
    #[arg(long, value_delimiter = ',', global = true)]
    pub fields: Vec<String>,
    /// Return full record (all fields)
    #[arg(long, global = true)]
    pub full: bool,
    /// Do not push results to the local store
    #[arg(long, global = true)]
    pub skip_push: bool,
    /// Include reconstructed abstract text (bulky; off by default)
    #[arg(long = "abstract", global = true)]
    pub include_abstract: bool,
}

/// Extension point for provider namespaces; future phases add variants here.
#[derive(Subcommand)]
pub enum Namespace {
    #[command(hide = true, about = "Dev round-trip surface for the local store")]
    Store(StoreArgs),
    #[command(about = "Query the OpenAlex scholarly-data API")]
    Openalex(OpenalexArgs),
    #[command(about = "Query the Semantic Scholar graph API")]
    Semanticscholar(SemanticscholarArgs),
    #[command(about = "Query the braincrawl neutral graph (store-only, no provider)")]
    Graph(GraphArgs),
    #[command(about = "Show aggregate statistics about the metadata network")]
    Stats,
}

#[derive(Args)]
pub struct GraphArgs {
    #[command(subcommand)]
    pub cmd: GraphCmd,
}

#[derive(Subcommand)]
pub enum GraphCmd {
    /// Bounded neighborhood traversal from one or more seed ids
    Neighborhood {
        /// Seed ids in ns:value form (e.g. openalex:W2031938753 doi:10.x/y)
        seeds: Vec<String>,
        /// Traversal direction: forward (src→dst) or backward (dst→src)
        #[arg(long, default_value = "forward")]
        dir: String,
        /// Max BFS depth from the seeds
        #[arg(long, default_value_t = 1)]
        depth: u32,
        /// Max nodes in the returned subgraph
        #[arg(long = "max-nodes", default_value_t = 200)]
        max_nodes: u32,
    },
}

#[derive(Args)]
pub struct OpenalexArgs {
    #[command(subcommand)]
    pub cmd: OpenalexCmd,
}

#[derive(Subcommand)]
pub enum OpenalexCmd {
    /// Fetch a single entity by ID (entity type inferred from ID prefix)
    Get {
        /// OpenAlex ID (W…/A…/S…/I…/T…/P…/F…/C…) or external ID (doi:…/orcid:…/issn:…/ror:…)
        id: String,
    },
    /// Full-text search over an entity collection
    Search {
        /// Entity type (works, authors, sources, institutions, topics, keywords, publishers, funders)
        entity: String,
        /// Search query string
        query: String,
    },
    /// Filter an entity collection by key:value expressions
    Find {
        /// Entity type (works, authors, sources, institutions, topics, keywords, publishers, funders)
        entity: String,
        /// Filter expressions in `key:value` form (e.g. `publication_year:2020` `is_oa:true`)
        filters: Vec<String>,
    },
    /// Autocomplete entity names by prefix
    Autocomplete {
        /// Entity type (works, authors, sources, institutions, topics, publishers, funders)
        entity: String,
        /// Prefix query string
        q: String,
    },
    /// List works that cite the given work
    #[command(name = "cited-by")]
    CitedBy {
        /// OpenAlex work ID (W…) or external ID (doi:…)
        id: String,
    },
    /// List works referenced by the given work
    Refs {
        /// OpenAlex work ID (W…) or external ID (doi:…)
        id: String,
    },
}

#[derive(Args)]
pub struct SemanticscholarArgs {
    #[command(subcommand)]
    pub cmd: SemanticscholarCmd,
}

#[derive(Subcommand)]
pub enum SemanticscholarCmd {
    /// Fetch a single paper or author by ID (entity type inferred from ID prefix)
    Get {
        /// Semantic Scholar ID (doi:…/arxiv:…/corpusid:…/s2:…/s2author:…/40-char hex)
        id: String,
    },
    /// Full-text search over papers or authors
    Search {
        /// Entity type (papers, authors)
        entity: String,
        /// Search query string
        query: String,
    },
    /// List papers that cite the given paper
    #[command(name = "cited-by")]
    CitedBy {
        /// Paper ID (doi:…/arxiv:…/corpusid:…/s2:…/40-char hex)
        id: String,
    },
    /// List papers referenced by the given paper
    Refs {
        /// Paper ID (doi:…/arxiv:…/corpusid:…/s2:…/40-char hex)
        id: String,
    },
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
    pub include_abstract: bool,
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
            include_abstract: g.include_abstract,
        }
    }
}

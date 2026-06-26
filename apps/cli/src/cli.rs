use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "braincrawl", version, about = "braincrawl research graph CLI")]
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
    #[command(about = "Backfill backward references from Crossref's deposited reference list")]
    Crossref(CrossrefArgs),
    #[command(about = "Backfill backward references from the OpenCitations COCI index")]
    Opencitations(OpencitationsArgs),
    #[command(name = "fetch-content", about = "Acquire a work's fulltext artifact from the open web into the store")]
    FetchContent(FetchContentArgs),
    #[command(name = "extract-text", about = "Extract text from a work's stored fulltext PDF artifact")]
    ExtractText(ExtractTextArgs),
    #[command(name = "fetch-pdf", about = "Resolve and download a work's fulltext artifact to stdout (no store write)")]
    FetchPdf(FetchPdfArgs),
    #[command(name = "push", about = "Store bytes from stdin or a file as an artifact of the given role")]
    Push(PushArgs),
    #[command(name = "get", about = "Read a stored artifact's bytes to stdout")]
    Get(GetArgs),
    #[command(about = "Query the braincrawl neutral graph (store-only, no provider)")]
    Graph(GraphArgs),
    #[command(about = "Show aggregate statistics about the metadata network")]
    Stats,
    #[command(about = "Manage the consolidated L3 document store (local files, no provider)")]
    L3(L3Args),
    #[command(about = "Query the arXiv preprint API")]
    Arxiv(ArxivArgs),
}

#[derive(Args)]
pub struct L3Args {
    #[command(subcommand)]
    pub cmd: L3Cmd,
}

#[derive(Subcommand)]
pub enum L3Cmd {
    /// Create a new L3 doc in the store; prints its absolute path to stdout
    New {
        /// Doc slug (kebab-case) — the primary key and filename stem
        doc: String,
        /// Body-convention label recorded in frontmatter (free string; `spine`/`dialectical` get a skeleton)
        #[arg(long, default_value = "freeform")]
        schema: String,
        /// Human title for the H1 heading (defaults to the doc slug)
        #[arg(long)]
        title: Option<String>,
        /// Overwrite if a doc with this slug already exists
        #[arg(long)]
        force: bool,
    },
    /// Print the absolute path of an existing L3 doc to stdout
    Path {
        /// Doc slug
        doc: String,
    },
    /// List all L3 docs in the store
    List,
    /// Lint a doc (or --all) against the envelope contract; advisory, never fails
    Check {
        /// Doc slug (omit with --all)
        #[arg(required_unless_present = "all")]
        doc: Option<String>,
        /// Check every doc in the store
        #[arg(long)]
        all: bool,
    },
    /// Regenerate INDEX.md from every doc's frontmatter
    Index,
    /// Adopt an existing markdown file into the store, normalizing its envelope
    Import {
        /// Path to the existing .md / .l3.md file
        file: String,
        /// Doc slug override (default: existing `doc`/`domain` frontmatter, else filename stem)
        #[arg(long)]
        doc: Option<String>,
        /// Schema label to set when the file declares none (default: freeform)
        #[arg(long)]
        schema: Option<String>,
        /// Remove the source file after a successful import
        #[arg(long)]
        mv: bool,
    },
    /// Delete a doc from the store and reindex
    Rm {
        /// Doc slug
        doc: String,
    },
}

#[derive(Args)]
pub struct FetchContentArgs {
    /// Work id in ns:value form (e.g. openalex:W2165758805 or doi:10.x/y)
    pub id: String,
    /// Where to resolve the artifact URL from (auto|openalex|unpaywall)
    #[arg(long, default_value = "auto")]
    pub from: String,
    /// Re-fetch even if a fulltext artifact already exists
    #[arg(long)]
    pub force: bool,
    /// Only accept a PDF artifact (reject HTML or other content types)
    #[arg(long = "require-pdf")]
    pub require_pdf: bool,
}

#[derive(Args)]
pub struct ExtractTextArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
}

#[derive(Args)]
pub struct FetchPdfArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Where to resolve the artifact URL from (auto|openalex|unpaywall)
    #[arg(long, default_value = "auto")]
    pub from: String,
    /// Only accept a PDF artifact (reject HTML or other content types)
    #[arg(long = "require-pdf")]
    pub require_pdf: bool,
    /// Write bytes to this path instead of stdout
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct PushArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Read bytes from this file instead of stdin
    pub file: Option<String>,
    /// Content mime type (default: sniff %PDF, else application/octet-stream)
    #[arg(long)]
    pub mime: Option<String>,
    /// Optional source label recorded with the artifact
    #[arg(long)]
    pub source: Option<String>,
    /// Optional source URL recorded with the artifact
    #[arg(long = "source-url")]
    pub source_url: Option<String>,
    /// Artifact role slug (e.g. fulltext, abstract, map)
    #[arg(long, default_value = "fulltext")]
    pub role: String,
}

#[derive(Args)]
pub struct GetArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Write bytes to this path instead of stdout
    #[arg(long, short = 'o')]
    pub output: Option<String>,
    /// Artifact role slug (e.g. fulltext, abstract, map)
    #[arg(long, default_value = "fulltext")]
    pub role: String,
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
pub struct CrossrefArgs {
    #[command(subcommand)]
    pub cmd: CrossrefCmd,
}

#[derive(Subcommand)]
pub enum CrossrefCmd {
    /// List works the given work references (via Crossref deposited reference list)
    Refs {
        /// Work id in ns:value form (resolved to its DOI)
        id: String,
    },
}

#[derive(Args)]
pub struct OpencitationsArgs {
    #[command(subcommand)]
    pub cmd: OpencitationsCmd,
}

#[derive(Subcommand)]
pub enum OpencitationsCmd {
    /// List works the given work references (via OpenCitations COCI index)
    Refs {
        /// Work id in ns:value form (resolved to its DOI)
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

#[derive(Args)]
pub struct ArxivArgs {
    #[command(subcommand)]
    pub cmd: ArxivCmd,
}

#[derive(Subcommand)]
pub enum ArxivCmd {
    /// Fetch a single arXiv work by ID (e.g. arxiv:2301.07041 or 2301.07041)
    Get {
        /// arXiv ID (arxiv:…, bare numeric, old-style hep-th/…, or full URL)
        id: String,
    },
    /// Search arXiv by query string
    Search {
        /// Search query; use field operators (ti:/au:/cat:) or plain text (wrapped as all:…)
        query: String,
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

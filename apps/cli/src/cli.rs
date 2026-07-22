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
    /// Emit the store-ready emission frame instead of the display envelope
    #[arg(long, global = true, conflicts_with = "text")]
    pub emission: bool,
}

/// Extension point for provider namespaces; future phases add variants here.
#[derive(Subcommand)]
pub enum Namespace {
    #[command(about = "Artifact bytes held against a work (Layer 1)")]
    Library(LibraryArgs),
    #[command(about = "Work metadata, aliases, and citation edges (Layer 2)")]
    Catalog(CatalogArgs),
    #[command(about = "Research Documents — the consolidated store (Layer 3)")]
    Collection(CollectionArgs),
    #[command(about = "Query the OpenAlex scholarly-data API")]
    Openalex(OpenalexArgs),
    #[command(about = "Query the Semantic Scholar graph API")]
    Semanticscholar(SemanticscholarArgs),
    #[command(about = "Backfill backward references from Crossref's deposited reference list")]
    Crossref(CrossrefArgs),
    #[command(about = "Backfill backward references from the OpenCitations COCI index")]
    Opencitations(OpencitationsArgs),
    #[command(about = "Query the arXiv preprint API")]
    Arxiv(ArxivArgs),
    #[command(about = "Print the Web UI URL for the configured server")]
    Web,
    #[command(name = "migrate-store", about = "Replay the local SQLite + blob corpus into a remote store over its HTTP API")]
    MigrateStore(MigrateStoreArgs),
    #[command(about = "Rename a work file to its canonical bibliographic filename")]
    Rename(RenameArgs),
}

#[derive(Args)]
pub struct RenameArgs {
    /// Path to the file to rename (extension is preserved)
    pub file: String,
    /// First author's surname (may contain spaces/particles, e.g. "Van De Mieroop")
    #[arg(long)]
    pub author: String,
    /// The work has more than one author (renders the EtAl marker)
    #[arg(long = "et-al")]
    pub et_al: bool,
    /// Publication year
    #[arg(long)]
    pub year: u32,
    /// Work title (slugified for the filename)
    #[arg(long)]
    pub title: String,
    /// Print the proposed new path; do not rename
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MigrateStoreArgs {
    /// Path to the local SQLite database (default: $HOME/.local/share/braincrawl/braincrawl.db)
    #[arg(long)]
    pub db: Option<String>,
    /// Path to the local blob root directory (default: $HOME/.local/share/braincrawl/blobs)
    #[arg(long)]
    pub blobs: Option<String>,
    /// Print the plan counts without pushing any work, edge, or artifact
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct ChunkArgs {
    /// Work id in ns:value form (stored fulltext → writes the chunks artifact)
    pub id: Option<String>,
    /// External PDF file — PIPE-ONLY: emits JSON to stdout, never stored
    #[arg(long)]
    pub file: Option<String>,
    /// Output artifact role slug
    #[arg(long, default_value = "chunks")]
    pub role: String,
    #[arg(long = "max-tokens", default_value_t = 512)]
    pub max_tokens: usize,
    #[arg(long, default_value_t = 64)]
    pub overlap: usize,
    /// Stream JSON to stdout instead of storing
    #[arg(long)]
    pub stdout: bool,
    /// Chunk faithful pages and skip image-only ones instead of refusing
    #[arg(long = "allow-partial")]
    pub allow_partial: bool,
    /// Re-chunk even if a chunks artifact already exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct CollectionArgs {
    #[command(subcommand)]
    pub cmd: L3Cmd,
}

#[derive(Subcommand)]
pub enum L3Cmd {
    /// Create a new L3 doc in the store; prints its absolute path to stdout
    New {
        /// Doc slug (kebab-case) — the primary key and filename stem
        doc: String,
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
    /// Lint a doc (or --all) against the required frontmatter; advisory, never fails
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
    /// Adopt an existing markdown file into the store, normalizing its frontmatter
    Import {
        /// Path to the existing .md / .l3.md file
        file: String,
        /// Doc slug override (default: existing `doc`/`domain` frontmatter, else filename stem)
        #[arg(long)]
        doc: Option<String>,
        /// Remove the source file after a successful import
        #[arg(long)]
        mv: bool,
    },
    /// Delete a doc from the store and reindex
    Rm {
        /// Doc slug
        doc: String,
    },
    /// Assign `^r-…` anchors for every `##` heading that lacks one
    AssignIds {
        /// Report what would be assigned; write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// List every node with a `reading` property and its catalog work, grouped by role
    ReadingList,
    /// Push local doc(s) to the worker's consolidated L3 store
    Push {
        /// Doc slug (omit to push every push-candidate doc)
        doc: Option<String>,
        /// Print the plan without transferring or touching sync state
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Bypass the worker's warning bounce (400) on push
        #[arg(long)]
        force: bool,
    },
    /// Pull doc(s) from the worker's consolidated L3 store into the local repo
    Pull {
        /// Doc slug (omit to pull every pull-candidate doc)
        doc: Option<String>,
        /// Print the plan without transferring or touching sync state
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
}

#[derive(Args)]
pub struct LibraryArgs {
    #[command(subcommand)]
    pub cmd: LibraryCmd,
}

#[derive(Subcommand)]
pub enum LibraryCmd {
    #[command(name = "put", about = "Store bytes from stdin or a file as an artifact of the given role")]
    Put(PushArgs),
    #[command(name = "get", about = "Read a stored artifact's bytes to stdout")]
    Get(GetArgs),
    #[command(name = "fetch", about = "Acquire a work's fulltext artifact from the open web into the store")]
    Fetch(FetchContentArgs),
    #[command(name = "extract-text", about = "Extract text from a work's stored fulltext PDF artifact")]
    ExtractText(ExtractTextArgs),
    #[command(name = "chunk", about = "Partition a work's stored fulltext into a citation-carrying chunks artifact")]
    Chunk(ChunkArgs),
    #[command(name = "list", about = "List every artifact a work holds — role, version, size, mime, provenance")]
    List(LibraryListArgs),
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
    /// Emit bytes to stdout instead of storing
    #[arg(long)]
    pub stdout: bool,
    /// Write bytes to this path instead of storing
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct ExtractTextArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Artifact role slug to store the extracted text at
    #[arg(long, default_value = "text")]
    pub role: String,
    /// Re-extract even if the role already holds an artifact
    #[arg(long)]
    pub force: bool,
    /// Emit text to stdout instead of storing
    #[arg(long)]
    pub stdout: bool,
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
pub struct LibraryListArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Restrict to a single artifact role
    #[arg(long)]
    pub role: Option<String>,
    /// Include superseded versions, not just the current one per role
    #[arg(long = "all-versions")]
    pub all_versions: bool,
}

#[derive(Args)]
pub struct CatalogArgs {
    #[command(subcommand)]
    pub cmd: CatalogCmd,
}

#[derive(Subcommand)]
pub enum CatalogCmd {
    /// Retrieve a work by alias (ns:value)
    Get {
        id: String,
    },
    /// Check which of the given aliases (ns:value) are present in the store
    Have {
        ids: Vec<String>,
    },
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
    /// Show aggregate statistics about the metadata network
    Stats,
    /// Write works and edges from an emission frame (stdin or a file) to the catalog
    Put {
        /// Read the emission from this file instead of stdin
        file: Option<String>,
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
    pub emission: bool,
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
            emission: g.emission,
        }
    }
}

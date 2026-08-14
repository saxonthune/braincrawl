/**
 * braincrawl CLI, modeled in TypeSpec as a protocol-neutral interface. No
 * @route/@get HTTP decorators: this is the "fundamental spec" layer, before any
 * connector (HTTP or CLI) binding.
 *
 * The command tree is organized by storage layer: `library` (Layer 1, artifact
 * bytes), `catalog` (Layer 2, metadata and citation edges), and `collection`
 * (Layer 3, research documents).
 *
 * NOTE: TypeSpec core has no first-class `resource` keyword the way Smithy
 * does. Resource-lifecycle helpers (@resource, @key, ResourceRead, ...) live in
 * the `@typespec/rest` library and are REST-flavored. To keep the comparison
 * protocol-neutral, operations are grouped into plain `interface` blocks here.
 */
namespace Braincrawl;

/**
 * A work id in `ns:value` form (e.g. `openalex:W123`, `doi:10.x/y`). Every id
 * parameter accepts any external identifier; braincrawl resolves it to a
 * canonical id internally.
 */
scalar WorkId extends string;

/**
 * An artifact role slug — `abstract`, `fulltext`, `text`, `chunks`, or any
 * other `[a-z0-9_-]+` name. A work holds one artifact per (role, version).
 */
scalar Role extends string;

/** A Research Document slug in kebab-case — the document's primary key. */
scalar DocSlug extends string;

enum OutputFormat {
  json,
  text,
}

/** Upstream read-only providers. Coverage is NOT uniform — see interfaces. */
enum Provider {
  openalex,
  semanticscholar,
  arxiv,
  crossref,
  opencitations,
}

enum OpenAlexEntity {
  works,
  authors,
  sources,
  institutions,
  topics,
  keywords,
  publishers,
  funders,
}

/**
 * NOTE: a different entity vocabulary from OpenAlex's — the two providers do
 * not agree on what a searchable collection is called.
 */
enum SemanticScholarEntity {
  papers,
  authors,
}

/** Where to resolve a fulltext artifact URL from. */
enum FromSource {
  auto,
  openalex,
  unpaywall,
}

enum Direction {
  forward,
  backward,
}

/**
 * Global output flags — top-level in clap, shared by every subcommand, so here
 * spread (`...CommonOutput`) into every operation's parameter list.
 */
model CommonOutput {
  /** --json (default) / --text */
  format?: OutputFormat;

  /** --limit N */
  limit?: int32;

  /** --all (ignore limit; paginate to exhaustion) */
  all?: boolean;

  /** --fields a,b,c */
  fields?: string;

  /** --full */
  full?: boolean;

  /** --skip-push (do not write provider results to the catalog) */
  skipPush?: boolean;

  /** --abstract (irregular: noun-shaped flag name) */
  `abstract`?: boolean;

  /**
   * --emission (emit the Emission wire frame instead of the display envelope,
   * so the result can be piped into Catalog.put). Ignores `fields` and
   * conflicts with `text` format: the Emission is a wire frame, not a display
   * shape.
   */
  emission?: boolean;
}

/** Provenance-bearing record emitted on stdout; opaque here. */
model WorkRecord {
  id: WorkId;
}

model Presence {
  id: WorkId;
  present: boolean;
}

model Subgraph {}

// ---------------------------------------------------------------------------
// The Emission — the wire frame between a provider read and a catalog write
// ---------------------------------------------------------------------------

/** An external identifier: a namespace and a value. */
model Alias {
  namespace: string;
  value: string;
}

/** A catalog-ready work, already lowered to the store's neutral vocabulary. */
model WorkInput {
  source: string;
  kind: string;
  aliases: Alias[];
  attrs?: unknown;
}

/** A catalog-ready citation edge. */
model EdgeInput {
  src: Alias;
  dst: Alias;
  relation: string;
  source: string;
  attrs?: unknown;
  fetchedAt: string;
}

/**
 * Everything a provider read produces for the catalog, fully lowered. This is
 * the output half of the subprocess plugin protocol, and the input Catalog.put
 * accepts — which is what makes a provider read decomposable into a lookup and
 * a store write.
 */
model Emission {
  records: WorkInput[];
  edges: EdgeInput[];

  /** Records the provider returned whose kind has no catalog mapping. */
  skippedUnmappable?: int32;
}

// ---------------------------------------------------------------------------
// Library (Layer 1) — artifact bytes held against a work
// ---------------------------------------------------------------------------

/** An artifact descriptor: what a work holds at a given role and version. */
model Artifact {
  role: Role;
  version: int32;
  byteSize: int64;
  mime: string;
  source?: string;
  sourceUrl?: string;
  fetchedAt: string;

  /** Whether this is the version `library get` serves for the role. */
  isCurrent: boolean;
}

interface Library {
  /**
   * library put <id> [file] — store bytes from stdin or a file as an artifact
   * at the given role, minting a new version rather than overwriting the old.
   */
  put(
    id: WorkId,

    /** optional positional FILE (else stdin) */
    file?: string,

    /** --mime (default: sniff `%PDF`, else application/octet-stream) */
    mime?: string,

    /** --role (default `fulltext`) */
    role?: Role,

    source?: string,
    sourceUrl?: string,
    ...CommonOutput,
  ): Artifact;

  /**
   * library get <id> — read a stored artifact's bytes to stdout or a file.
   * Serves the current version of the role.
   */
  get(
    id: WorkId,

    /** -o / --output (sink: file else stdout) */
    outputPath?: string,

    /** --role (default `fulltext`) */
    role?: Role,

    ...CommonOutput,
  ): bytes;

  /**
   * library list <id> — list every artifact the work holds: role, version,
   * size, mime, provenance, and which version is current. Without this the
   * roles a work holds are only reachable by guessing their names.
   */
  list(
    id: WorkId,

    /** --role (restrict to one role; otherwise every role) */
    role?: Role,

    /** --all-versions (include superseded versions, not just current) */
    allVersions?: boolean,

    ...CommonOutput,
  ): Artifact[];

  /**
   * library fetch <id> — acquire a work's fulltext from the open web and store
   * it at role `fulltext`.
   */
  fetch(
    id: WorkId,
    from?: FromSource,

    /** --force (re-fetch even if the role already holds an artifact) */
    force?: boolean,

    /** --require-pdf (reject HTML and other content types) */
    requirePdf?: boolean,

    /** --stdout (emit bytes instead of storing) */
    stdout?: boolean,

    /** -o / --output (write bytes to a path instead of storing) */
    outputPath?: string,

    ...CommonOutput,
  ): Artifact | bytes;

  /**
   * library extract-text <id> — extract text from the work's stored fulltext
   * PDF and store it at role `text`. Refuses input with no text layer rather
   * than emitting unfaithful text — no OCR.
   */
  extractText(
    id: WorkId,

    /** --role (default `text`) */
    role?: Role,

    /** --force (re-extract even if the role already holds an artifact) */
    force?: boolean,

    /** --stdout (emit text instead of storing) */
    stdout?: boolean,

    ...CommonOutput,
  ): Artifact;

  /**
   * library chunk <id> — partition a work's stored fulltext into a
   * citation-carrying `chunks` artifact (structure-first, token-capped, small
   * overlap; each chunk carries provenance for citing back into the source).
   *
   * An unstored external `file` is PIPE-ONLY: it emits to stdout and is never
   * stored (to store it, put the PDF first). Refuses scanned or image-only
   * input rather than emitting chunks it cannot stand behind — no OCR.
   */
  chunk(
    id?: WorkId,

    /** optional external PDF file — pipe-only: emits to stdout, never stored */
    file?: string,

    /** --role (default `chunks`) */
    role?: Role,

    maxTokens?: int32,
    overlap?: int32,

    /** --stdout (stream JSON instead of storing; forced when `file` is used) */
    stdout?: boolean,

    /** --allow-partial (chunk faithful pages, skip image-only ones) */
    allowPartial?: boolean,

    /** --force (re-chunk even if the role already holds an artifact) */
    force?: boolean,

    ...CommonOutput,
  ): Artifact;
}

// ---------------------------------------------------------------------------
// Catalog (Layer 2) — metadata, aliases, citation edges
// ---------------------------------------------------------------------------

model PutResult {
  nodesWritten: int32;
  edgesWritten: int32;
  skippedUnmappable: int32;
}

interface Catalog {
  /** catalog get <id> — read a work by any external id. */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /**
   * catalog put — read an Emission from stdin or a file and write its works and
   * edges to the catalog. The write atom the provider reads compose from: a
   * provider read with `--skip-push --emission` piped into this operation is
   * equivalent to the same provider read with its default push.
   */
  put(
    /** optional positional FILE (else stdin) */
    file?: string,

    /** the parsed frame read from that source */
    emission?: Emission,

    ...CommonOutput,
  ): PutResult;

  /** catalog have <ids>... — report which of the given ids are present. */
  have(ids: WorkId[], ...CommonOutput): Presence[];

  /**
   * catalog neighborhood <seeds>... — bounded traversal of the citation graph.
   * The `dir` parameter is the ONLY explicit direction in the surface; the
   * provider cited-by/refs pairs imply it in the verb instead.
   */
  neighborhood(
    seeds: WorkId[],
    dir?: Direction,
    depth?: int32,
    maxNodes?: int32,
    ...CommonOutput,
  ): Subgraph;

  /** catalog stats — aggregate counts over the metadata network. */
  stats(...CommonOutput): {};
}

// ---------------------------------------------------------------------------
// Collection (Layer 3) — the consolidated research-document store
// ---------------------------------------------------------------------------

model AssignedAnchor {
  doc: DocSlug;
  headingLine: int32;
  id: string;
  title: string;

  /** The `^t-<slug>` temporary anchor (without `^`) this id resolved, when the
   * id came from a hand-written temporary anchor rather than an anchor-less
   * heading. */
  temp?: string;
}

model DocSummary {
  doc: DocSlug;
  title?: string;
  updated?: string;
}

model ReadingEntry {
  doc: DocSlug;
  anchor: string;
  work: WorkId;
  role?: string;
}

interface Collection {
  /** collection new <doc> — create a Research Document; prints its path. */
  new(
    doc: DocSlug,

    /** --title (H1 heading; defaults to the slug) */
    title?: string,

    /** --force (overwrite an existing document with this slug) */
    force?: boolean,

    ...CommonOutput,
  ): string;

  /** collection path <doc> — print an existing document's absolute path. */
  path(doc: DocSlug, ...CommonOutput): string;

  /** collection rm <doc> — delete a document and reindex. */
  rm(doc: DocSlug, ...CommonOutput): {};

  /** collection list — list every document in the store. */
  list(...CommonOutput): DocSummary[];

  /**
   * collection check [doc] [--all] — lint against the required frontmatter.
   * Advisory: reports findings and never fails.
   */
  check(doc?: DocSlug, all?: boolean, ...CommonOutput): string[];

  /** collection index — regenerate INDEX.md from every document's frontmatter. */
  index(...CommonOutput): {};

  /**
   * collection import <file> — adopt an existing markdown file, normalizing its
   * frontmatter.
   */
  import(
    file: string,

    /** --doc (slug override; default from frontmatter, else filename stem) */
    doc?: DocSlug,

    /** --mv (remove the source file after a successful import) */
    mv?: boolean,

    ...CommonOutput,
  ): DocSlug;

  /**
   * collection assign-ids [--dry-run] — mint `^r-…` anchors for every `##`
   * heading that lacks one and append them to the heading line. The one
   * operation in this layer that writes to document bodies: node identity is
   * tooling-owned, and anchors are store-global so they never need rewriting.
   * Also resolves every hand-written `^t-<slug>` temporary anchor to a fresh
   * store-global `^r-…`, rewriting the heading and every `[[^t-<slug>]]`
   * reference to it across the whole store in the same pass.
   */
  assignIds(dryRun?: boolean, ...CommonOutput): AssignedAnchor[];

  /**
   * collection reading-list — list every node carrying a `reading` property
   * alongside its catalog work, grouped by role.
   */
  readingList(...CommonOutput): ReadingEntry[];
}

// ---------------------------------------------------------------------------
// Providers — upstream reads. Results are written to the catalog by default;
// `--skip-push` suppresses the write and `--emission` yields the wire frame.
//
// NOTE: the provider is encoded in the INTERFACE name, not a parameter — the
// "provider-as-branch" smell. The `refs`/`citedBy`/`get` operations recur
// identically across the interfaces below: prime candidates to collapse into
// e.g. `refs(id, provider: Provider)`.
// ---------------------------------------------------------------------------

interface OpenAlex {
  /** openalex get <id> — fetch one entity, inferring its type from the id. */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /** openalex search <entity> <query> */
  search(entity: OpenAlexEntity, query: string, ...CommonOutput): WorkRecord[];

  /** openalex find <entity> <filters>... */
  find(entity: OpenAlexEntity, filters: string[], ...CommonOutput): WorkRecord[];

  /** openalex autocomplete <entity> <q> */
  autocomplete(entity: OpenAlexEntity, q: string, ...CommonOutput): WorkRecord[];

  /**
   * openalex cited-by <id> — forward citations. Direction is implied by the
   * verb; only Catalog.neighborhood exposes an explicit `dir`.
   */
  citedBy(id: WorkId, ...CommonOutput): WorkRecord[];

  /** openalex refs <id> — backward references. */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

interface SemanticScholar {
  /** semanticscholar get <id> */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /**
   * semanticscholar search <entity> <query>
   * NO find/autocomplete twin — a coverage gap against OpenAlex.
   */
  search(
    entity: SemanticScholarEntity,
    query: string,
    ...CommonOutput,
  ): WorkRecord[];

  /** semanticscholar cited-by <id> */
  citedBy(id: WorkId, ...CommonOutput): WorkRecord[];

  /** semanticscholar refs <id> */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

interface Arxiv {
  /**
   * arxiv get <id> — accepts `arxiv:…`, a bare numeric id, an old-style
   * `hep-th/…` id, or a full URL.
   */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /**
   * arxiv search <query> — no entity parameter: arXiv has one searchable
   * collection, so the query carries field operators (`ti:`/`au:`/`cat:`).
   */
  search(query: string, ...CommonOutput): WorkRecord[];
}

interface Crossref {
  /** crossref refs <id> — from Crossref's deposited reference list. */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

interface OpenCitations {
  /** opencitations refs <id> — from the OpenCitations COCI index. */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

// ---------------------------------------------------------------------------
// Operations that belong to no layer
// ---------------------------------------------------------------------------

/** web — prints `<server_url>/web`, the Web UI URL for the configured server. */
op web(): string;

/**
 * A store spec: a configured store name from `[stores.<name>]` in the config
 * file, or a bare base URL.
 */
scalar StoreSpec extends string;

/**
 * store — named stores: list them, switch the active one, diff two stores,
 * and sync one into another over the `/export/*` + write routes.
 */
interface StoreOps {
  /** store list — configured stores and which one is active. */
  list(): {};

  /** store use <name> — rewrite `active_store` in the config file. */
  use(@doc("Store name from [stores.<name>]") name: string): {};

  /** store diff <a> [b] — counts per side and which works each side lacks. */
  diff(a: StoreSpec, @doc("Default: the active store") b?: StoreSpec): {};

  /**
   * store sync <source> <dest> — replay everything the destination lacks
   * from the source: catalog works/edges/artifacts, then Research Documents
   * node-by-node (rules in crates/l3-sync). Idempotent; both directions use
   * the same path.
   */
  sync(
    source: StoreSpec,
    dest: StoreSpec,

    /** --dry-run (print plan counts; push nothing) */
    dryRun?: boolean,

    /** --doc (restrict to one Research Document; skips the catalog phases) */
    doc?: DocSlug,

    /** --force (with --doc: bypass the destination's parse-warning bounce) */
    force?: boolean,
  ): {};
}

/**
 * rename <file> — rename a work file to its canonical bibliographic filename.
 *
 * NOTE: operates on a loose file on disk, not on anything the store holds — the
 * one operation with no store contact at all.
 */
op rename(
  file: string,

  /** --author (first author's surname; may contain spaces or particles) */
  author: string,

  /** --et-al */
  etAl?: boolean,

  year: int32,

  /** --title (slugified for the filename) */
  title: string,

  /** --dry-run (print the proposed path; rename nothing) */
  dryRun?: boolean,
): string;

/**
 * braincrawl CLI — current surface ("so far"), modeled in TypeSpec as a
 * protocol-neutral interface. No @route/@get HTTP decorators: this is the
 * "fundamental spec" layer, before any connector (HTTP or CLI) binding.
 *
 * NOTE: TypeSpec core has no first-class `resource` keyword the way Smithy
 * does. Resource-lifecycle helpers (@resource, @key, ResourceRead, ...) live in
 * the `@typespec/rest` library and are REST-flavored. To keep the comparison
 * protocol-neutral, operations are grouped into plain `interface` blocks here.
 */
namespace Braincrawl;

scalar WorkId extends string;

enum OutputFormat {
  json,
  text,
}

/** Upstream read-only providers. Coverage is NOT uniform — see interfaces. */
enum Provider {
  openalex,
  semanticscholar,
  crossref,
  opencitations,
}

enum Entity {
  works,
  authors,
  sources,
  concepts,
}

enum Rights {
  open,
  link_only,
  restricted,
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

  /** --skip-push */
  skipPush?: boolean;

  /** --abstract (irregular: noun-shaped flag name) */
  `abstract`?: boolean;
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
// Store / Work — keyed by a single work id
// ---------------------------------------------------------------------------

interface Store {
  /** store get <id> — local store only */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /** store have <ids>... — presence check */
  have(ids: WorkId[], ...CommonOutput): Presence[];
}

/**
 * Provider operations. NOTE: provider is encoded in the INTERFACE name, not a
 * parameter — the "provider-as-branch" smell. The `refs`/`citedBy`/`get` ops
 * recur identically across the four provider interfaces below: prime
 * candidates to collapse into e.g. `refs(id, provider: Provider)`.
 */
interface OpenAlex {
  /** openalex get <id> */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /** openalex cited-by <id> — direction implied by the verb */
  citedBy(id: WorkId, ...CommonOutput): WorkRecord[];

  /** openalex refs <id> */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];

  /** openalex search <entity> <query> */
  search(entity: Entity, query: string, ...CommonOutput): WorkRecord[];

  /** openalex find <entity> <filters>... */
  find(entity: Entity, filters: string[], ...CommonOutput): WorkRecord[];

  /** openalex autocomplete <entity> <q> */
  autocomplete(entity: Entity, q: string, ...CommonOutput): WorkRecord[];
}

interface SemanticScholar {
  /** semanticscholar get <id> */
  get(id: WorkId, ...CommonOutput): WorkRecord;

  /** semanticscholar cited-by <id> */
  citedBy(id: WorkId, ...CommonOutput): WorkRecord[];

  /** semanticscholar refs <id> */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];

  /** semanticscholar search <entity> <query>
   *  NO find/autocomplete twin — a coverage gap vs OpenAlex. */
  search(entity: Entity, query: string, ...CommonOutput): WorkRecord[];
}

interface Crossref {
  /** crossref refs <id> */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

interface OpenCitations {
  /** opencitations refs <id> */
  refs(id: WorkId, ...CommonOutput): WorkRecord[];
}

// ---------------------------------------------------------------------------
// Content lifecycle (stub -> ... -> fulltext -> text)
// ---------------------------------------------------------------------------

interface Content {
  /** fetch-content <id> — acquire fulltext into the store */
  fetchContent(
    id: WorkId,
    from?: FromSource,
    force?: boolean,
    requirePdf?: boolean,
    ...CommonOutput
  ): WorkRecord;

  /** extract-text <id> */
  extractText(id: WorkId, ...CommonOutput): WorkRecord;

  /** fetch-pdf <id> — to stdout/file, no store write */
  fetchPdf(
    id: WorkId,
    from?: FromSource,
    requirePdf?: boolean,

    /** -o / --output (sink: file else stdout) */
    outputPath?: string,

    ...CommonOutput
  ): bytes;

  /** push-pdf <id> [file] — store bytes from stdin/file */
  pushPdf(
    id: WorkId,

    /** optional positional FILE (else stdin) */
    file?: string,

    mime?: string,
    rights?: Rights,
    source?: string,
    sourceUrl?: string,
    ...CommonOutput
  ): WorkRecord;

  /** get-pdf <id> — stored bytes to stdout/file */
  getPdf(id: WorkId, outputPath?: string, ...CommonOutput): bytes;
}

// ---------------------------------------------------------------------------
// Graph + misc
// ---------------------------------------------------------------------------

interface Graph {
  /**
   * graph neighborhood <seeds>... --dir --depth --max-nodes
   * (the ONLY command with an explicit direction parameter)
   */
  neighborhood(
    seeds: WorkId[],
    dir?: Direction,
    depth?: int32,
    maxNodes?: int32,
    ...CommonOutput
  ): Subgraph;
}

/** stats — aggregate counts over the metadata network. */
op stats(...CommonOutput): {};

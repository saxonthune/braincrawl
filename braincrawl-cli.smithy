$version: "2.0"

namespace braincrawl.cli

/// braincrawl CLI — current surface ("so far"), modeled as a protocol-neutral
/// resource interface in Smithy. Provider-specific operations
/// (OpenAlex*/SemanticScholar*/Crossref*/OpenCitations*) are kept separate to
/// mirror today's command tree; the irregularities they create are flagged in
/// comments so the model doubles as a rationalization worksheet.
service Braincrawl {
    version: "0.1.0"
    resources: [
        Work
        Graph
    ]
    operations: [
        // Catalog search/filter. NOTE: provider is baked into the op NAME today
        // rather than being a parameter — the "provider-as-branch" smell.
        OpenAlexSearch
        OpenAlexFind
        OpenAlexAutocomplete
        SemanticScholarSearch
        Stats
    ]
}

// ---------------------------------------------------------------------------
// Shared vocabulary
// ---------------------------------------------------------------------------

/// A work id in `ns:value` form (e.g. `openalex:W123`, `doi:10.x/y`).
string WorkId

list WorkIdList {
    member: WorkId
}

list StringList {
    member: String
}

enum OutputFormat {
    JSON = "json"
    TEXT = "text"
}

/// Upstream read-only providers. NOTE: coverage is NOT uniform — search/find/
/// autocomplete exist for OpenAlex, only search for S2, neither for the rest.
enum Provider {
    OPENALEX = "openalex"
    SEMANTIC_SCHOLAR = "semanticscholar"
    CROSSREF = "crossref"
    OPENCITATIONS = "opencitations"
}

enum Entity {
    WORKS = "works"
    AUTHORS = "authors"
    SOURCES = "sources"
    CONCEPTS = "concepts"
}

enum Rights {
    OPEN = "open"
    LINK_ONLY = "link_only"
    RESTRICTED = "restricted"
}

/// Where to resolve a fulltext artifact URL from.
enum FromSource {
    AUTO = "auto"
    OPENALEX = "openalex"
    UNPAYWALL = "unpaywall"
}

enum Direction {
    FORWARD = "forward"
    BACKWARD = "backward"
}

/// Global output options. In clap these are top-level flags shared by EVERY
/// subcommand, so here they are a mixin applied to every operation input.
@mixin
structure CommonOutput {
    /// --json (default) / --text
    format: OutputFormat
    /// --limit N
    limit: Integer
    /// --all  (ignore limit; paginate to exhaustion)
    all: Boolean
    /// --fields a,b,c
    fields: String
    /// --full
    full: Boolean
    /// --skip-push  (do not write results to the local store)
    skipPush: Boolean
    /// --abstract  (irregular: noun-shaped flag; reads better as include-abstract)
    abstractText: Boolean
}

/// The provenance-bearing record emitted on stdout. Opaque here; every field
/// would carry per-source provenance + freshness in the real schema.
structure WorkRecord {
    id: WorkId
}

list WorkList {
    member: WorkRecord
}

structure Presence {
    id: WorkId
    present: Boolean
}

list PresenceList {
    member: Presence
}

structure Subgraph {}

structure Bytes {}

// ---------------------------------------------------------------------------
// Work resource — everything keyed by a single work id
// ---------------------------------------------------------------------------

resource Work {
    identifiers: { id: WorkId }
    read: StoreGet
    operations: [
        OpenAlexGet
        OpenAlexCitedBy
        OpenAlexRefs
        SemanticScholarGet
        SemanticScholarCitedBy
        SemanticScholarRefs
        CrossrefRefs
        OpenCitationsRefs
        FetchContent
        ExtractText
        Chunk
        FetchPdf
        PushPdf
        GetPdf
    ]
    collectionOperations: [
        StoreHave
    ]
}

/// `store get <id>` — read from the local store only.
@readonly
operation StoreGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `store have <ids>...` — presence check over the local store.
@readonly
operation StoreHave {
    input := with [CommonOutput] {
        @required
        ids: WorkIdList
    }
    output := {
        results: PresenceList
    }
}

/// `openalex get <id>` — fetch one entity upstream. NOTE: duplicates StoreGet's
/// intent against a different source; the source is the op name, not a param.
@readonly
operation OpenAlexGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `openalex cited-by <id>` — forward citations. NOTE: direction is implied by
/// the verb; only Graph/Neighborhood exposes an explicit --dir.
@readonly
operation OpenAlexCitedBy {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

/// `openalex refs <id>` — backward references.
@readonly
operation OpenAlexRefs {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

/// `semanticscholar get <id>`
@readonly
operation SemanticScholarGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `semanticscholar cited-by <id>`
@readonly
operation SemanticScholarCitedBy {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

/// `semanticscholar refs <id>`
@readonly
operation SemanticScholarRefs {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

/// `crossref refs <id>`
@readonly
operation CrossrefRefs {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

/// `opencitations refs <id>`
@readonly
operation OpenCitationsRefs {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        works: WorkList
    }
}

// ^ The four *Refs operations above are byte-for-byte identical except for the
//   provider — the single strongest argument for collapsing them into one
//   `Refs(id, provider: Provider)`. Same story for *CitedBy and *Get.

/// `fetch-content <id>` — acquire fulltext into the store.
operation FetchContent {
    input := with [CommonOutput] {
        @required
        id: WorkId
        from: FromSource
        force: Boolean
        requirePdf: Boolean
    }
    output := {
        work: WorkRecord
    }
}

/// `extract-text <id>` — extract text from a stored PDF payload.
operation ExtractText {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `chunk <id>` — partition a work's stored fulltext into a citation-carrying
/// `chunks` artifact (structure-first, token-capped, small overlap; each chunk
/// carries provenance). One of many secondary derived artifacts a work may hold —
/// not part of any abstract/fulltext dichotomy. Store target is a PUSHED work only;
/// an unstored external `file` is pipe-only (stdout, never stored). Reports whether
/// it can faithfully chunk (born-digital text layer) and refuses scanned/image-only
/// input rather than emitting unfaithful chunks — no OCR.
operation Chunk {
    input := with [CommonOutput] {
        id: WorkId
        /// optional external PDF file — pipe-only: emits to stdout, never stored
        file: String
        /// output artifact role (default "chunks")
        role: String
        maxTokens: Integer
        overlap: Integer
        /// stream JSON to stdout instead of storing (forced when `file` is used)
        stdout: Boolean
        /// re-chunk even if a chunks artifact already exists
        force: Boolean
    }
    output := {
        work: WorkRecord
    }
}

/// `fetch-pdf <id>` — resolve + download to stdout/file (no store write).
@readonly
operation FetchPdf {
    input := with [CommonOutput] {
        @required
        id: WorkId
        from: FromSource
        requirePdf: Boolean
        /// -o / --output  (sink: file else stdout — see "where does the signal land")
        outputPath: String
    }
    output := {
        bytes: Bytes
    }
}

/// `push-pdf <id> [file]` — store bytes from stdin/file as the fulltext payload.
operation PushPdf {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// optional positional FILE (else stdin)
        file: String
        mime: String
        rights: Rights
        source: String
        sourceUrl: String
    }
    output := {
        work: WorkRecord
    }
}

/// `get-pdf <id>` — read stored fulltext bytes to stdout/file.
@readonly
operation GetPdf {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// -o / --output
        outputPath: String
    }
    output := {
        bytes: Bytes
    }
}

// ---------------------------------------------------------------------------
// Graph resource
// ---------------------------------------------------------------------------

resource Graph {
    operations: [
        Neighborhood
    ]
}

/// `graph neighborhood <seeds>... --dir --depth --max-nodes`
@readonly
operation Neighborhood {
    input := with [CommonOutput] {
        @required
        seeds: WorkIdList
        /// --dir  (the ONLY command with an explicit direction)
        dir: Direction
        /// --depth
        depth: Integer
        /// --max-nodes
        maxNodes: Integer
    }
    output := {
        subgraph: Subgraph
    }
}

// ---------------------------------------------------------------------------
// Catalog search — provider baked into op name; S2 lacks find/autocomplete
// ---------------------------------------------------------------------------

/// `openalex search <entity> <query>`
@readonly
operation OpenAlexSearch {
    input := with [CommonOutput] {
        @required
        entity: Entity
        @required
        query: String
    }
    output := {
        results: WorkList
    }
}

/// `openalex find <entity> <filters>...`
@readonly
operation OpenAlexFind {
    input := with [CommonOutput] {
        @required
        entity: Entity
        @required
        filters: StringList
    }
    output := {
        results: WorkList
    }
}

/// `openalex autocomplete <entity> <q>`
@readonly
operation OpenAlexAutocomplete {
    input := with [CommonOutput] {
        @required
        entity: Entity
        @required
        q: String
    }
    output := {
        results: WorkList
    }
}

/// `semanticscholar search <entity> <query>`  (no find/autocomplete twin — a gap)
@readonly
operation SemanticScholarSearch {
    input := with [CommonOutput] {
        @required
        entity: Entity
        @required
        query: String
    }
    output := {
        results: WorkList
    }
}

/// `stats` — aggregate counts over the metadata network.
@readonly
operation Stats {
    input := with [CommonOutput] {}
    output := {}
}

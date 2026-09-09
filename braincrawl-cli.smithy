$version: "2.0"

namespace braincrawl.cli

/// braincrawl CLI, modeled as a protocol-neutral resource interface in Smithy.
///
/// The command tree is organized by storage layer: `library` (Layer 1, artifact
/// bytes), `catalog` (Layer 2, metadata and citation edges), and `collection`
/// (Layer 3, research documents). Provider-specific operations are kept separate
/// to mirror the command tree; the irregularities they create are flagged in
/// comments so the model doubles as a rationalization worksheet.
service Braincrawl {
    version: "0.1.0"
    resources: [
        Library
        Catalog
        Collection
    ]
    operations: [
        // Provider reads. NOTE: the provider is baked into the operation NAME
        // rather than being a parameter — the "provider-as-branch" smell.
        OpenAlexGet
        OpenAlexSearch
        OpenAlexFind
        OpenAlexAutocomplete
        OpenAlexCitedBy
        OpenAlexRefs
        SemanticScholarGet
        SemanticScholarSearch
        SemanticScholarCitedBy
        SemanticScholarRefs
        ArxivGet
        ArxivSearch
        CrossrefRefs
        OpenCitationsRefs
        Web
        StoreList
        StoreUse
        StoreDiff
        StoreSync
        Rename
    ]
}

// ---------------------------------------------------------------------------
// Shared vocabulary
// ---------------------------------------------------------------------------

/// A work id in `ns:value` form (e.g. `openalex:W123`, `doi:10.x/y`). Every id
/// parameter accepts any external identifier; braincrawl resolves it to a
/// canonical id internally.
string WorkId

/// An artifact role slug — `abstract`, `fulltext`, `text`, `chunks`, or any
/// other `[a-z0-9_-]+` name. A work holds one artifact per (role, version).
string Role

/// A Research Document slug in kebab-case — the document's primary key.
string DocSlug

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

/// Upstream read-only providers. NOTE: coverage is NOT uniform — get/search/
/// find/autocomplete/cited-by/refs exist for OpenAlex, a subset for Semantic
/// Scholar and arXiv, and refs alone for Crossref and OpenCitations.
enum Provider {
    OPENALEX = "openalex"
    SEMANTIC_SCHOLAR = "semanticscholar"
    ARXIV = "arxiv"
    CROSSREF = "crossref"
    OPENCITATIONS = "opencitations"
}

enum OpenAlexEntity {
    WORKS = "works"
    AUTHORS = "authors"
    SOURCES = "sources"
    INSTITUTIONS = "institutions"
    TOPICS = "topics"
    KEYWORDS = "keywords"
    PUBLISHERS = "publishers"
    FUNDERS = "funders"
}

/// NOTE: a different entity vocabulary from OpenAlex's — the two providers do
/// not agree on what a searchable collection is called.
enum SemanticScholarEntity {
    PAPERS = "papers"
    AUTHORS = "authors"
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
    /// --skip-push  (do not write provider results to the catalog)
    skipPush: Boolean
    /// --abstract  (irregular: noun-shaped flag; reads better as include-abstract)
    abstractText: Boolean
    /// --emission  (emit the Emission wire frame instead of the display
    /// envelope, so the result can be piped into CatalogPut). Ignores `fields`
    /// and conflicts with TEXT format: the Emission is a wire frame, not a
    /// display shape.
    emission: Boolean
}

/// The provenance-bearing record emitted on stdout. Opaque here; every field
/// carries per-source provenance and freshness in the real schema.
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
// The Emission — the wire frame between a provider read and a catalog write
// ---------------------------------------------------------------------------

/// An external identifier: a namespace and a value.
structure Alias {
    @required
    namespace: String
    @required
    value: String
}

list AliasList {
    member: Alias
}

/// A catalog-ready work, already lowered to the store's neutral vocabulary.
structure WorkInput {
    @required
    source: String
    @required
    kind: String
    @required
    aliases: AliasList
    attrs: Document
}

list WorkInputList {
    member: WorkInput
}

/// A catalog-ready citation edge.
structure EdgeInput {
    @required
    src: Alias
    @required
    dst: Alias
    @required
    relation: String
    @required
    source: String
    attrs: Document
    @required
    fetchedAt: String
}

list EdgeInputList {
    member: EdgeInput
}

/// Everything a provider read produces for the catalog, fully lowered. This is
/// the output half of the subprocess plugin protocol, and the input CatalogPut
/// accepts — which is what makes a provider read decomposable into a lookup and
/// a store write.
structure Emission {
    @required
    records: WorkInputList
    @required
    edges: EdgeInputList
    /// Records the provider returned whose kind has no catalog mapping.
    skippedUnmappable: Integer
}

// ---------------------------------------------------------------------------
// Library (Layer 1) — artifact bytes held against a work
// ---------------------------------------------------------------------------

/// An artifact descriptor: what a work holds at a given role and version.
structure Artifact {
    @required
    role: Role
    @required
    version: Integer
    @required
    byteSize: Long
    @required
    mime: String
    source: String
    sourceUrl: String
    @required
    fetchedAt: String
    /// Whether this is the version LibraryGet serves for the role.
    @required
    isCurrent: Boolean
}

list ArtifactList {
    member: Artifact
}

resource Library {
    identifiers: { id: WorkId }
    operations: [
        LibraryPut
        LibraryGet
        LibraryList
        LibraryFetch
        LibraryExtractText
        LibraryChunk
    ]
}

/// `library put <id> [file]` — store bytes from stdin or a file as an artifact
/// at the given role. Minting a new version rather than overwriting the old one.
operation LibraryPut {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// optional positional FILE (else stdin)
        file: String
        /// --mime  (default: sniff `%PDF`, else application/octet-stream)
        mime: String
        /// --role  (default `fulltext`)
        role: Role
        source: String
        sourceUrl: String
    }
    output := {
        artifact: Artifact
    }
}

/// `library get <id>` — read a stored artifact's bytes to stdout or a file.
/// Serves the current version of the role.
@readonly
operation LibraryGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// -o / --output  (sink: file else stdout)
        outputPath: String
        /// --role  (default `fulltext`)
        role: Role
    }
    output := {
        bytes: Bytes
    }
}

/// `library list <id>` — list every artifact the work holds: role, version,
/// size, mime, provenance, and which version is current. Without this the roles
/// a work holds are only reachable by guessing their names.
@readonly
operation LibraryList {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// --role  (restrict to one role; otherwise every role)
        role: Role
        /// --all-versions  (include superseded versions, not just current)
        allVersions: Boolean
    }
    output := {
        artifacts: ArtifactList
    }
}

/// `library fetch <id>` — acquire a work's fulltext from the open web and store
/// it at role `fulltext`.
operation LibraryFetch {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// --from  (auto|openalex|unpaywall)
        from: FromSource
        /// --force  (re-fetch even if the role already holds an artifact)
        force: Boolean
        /// --require-pdf  (reject HTML and other content types)
        requirePdf: Boolean
        /// --stdout  (emit bytes instead of storing)
        stdout: Boolean
        /// -o / --output  (write bytes to a path instead of storing)
        outputPath: String
    }
    output := {
        artifact: Artifact
        bytes: Bytes
    }
}

/// `library extract-text <id>` — extract text from the work's stored fulltext
/// PDF and store it at role `text`. Refuses input with no text layer rather
/// than emitting unfaithful text — no OCR.
operation LibraryExtractText {
    input := with [CommonOutput] {
        @required
        id: WorkId
        /// --role  (default `text`)
        role: Role
        /// --force  (re-extract even if the role already holds an artifact)
        force: Boolean
        /// --stdout  (emit text instead of storing)
        stdout: Boolean
    }
    output := {
        artifact: Artifact
    }
}

/// `library chunk <id>` — partition a work's stored fulltext into a
/// citation-carrying `chunks` artifact (structure-first, token-capped, small
/// overlap; each chunk carries provenance for citing back into the source).
/// An unstored external `file` is PIPE-ONLY: it emits to stdout and is never
/// stored. Refuses scanned or image-only input rather than emitting chunks it
/// cannot stand behind — no OCR.
operation LibraryChunk {
    input := with [CommonOutput] {
        id: WorkId
        /// optional external PDF file — pipe-only: emits to stdout, never stored
        file: String
        /// --role  (default `chunks`)
        role: Role
        maxTokens: Integer
        overlap: Integer
        /// --stdout  (stream JSON instead of storing; forced when `file` is used)
        stdout: Boolean
        /// --allow-partial  (chunk faithful pages, skip image-only ones)
        allowPartial: Boolean
        /// --force  (re-chunk even if the role already holds an artifact)
        force: Boolean
    }
    output := {
        artifact: Artifact
    }
}

// ---------------------------------------------------------------------------
// Catalog (Layer 2) — metadata, aliases, citation edges
// ---------------------------------------------------------------------------

resource Catalog {
    identifiers: { id: WorkId }
    read: CatalogGet
    collectionOperations: [
        CatalogPut
        CatalogHave
        CatalogNeighborhood
        CatalogStats
    ]
}

/// `catalog get <id>` — read a work by any external id.
@readonly
operation CatalogGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `catalog put` — read an Emission from stdin or a file and write its works
/// and edges to the catalog. The write atom the provider reads compose from:
/// a provider read with `--skip-push --emission` piped into this operation is
/// equivalent to the same provider read with its default push.
operation CatalogPut {
    input := with [CommonOutput] {
        /// optional positional FILE (else stdin)
        file: String
        /// the parsed frame read from that source
        emission: Emission
    }
    output := {
        nodesWritten: Integer
        edgesWritten: Integer
        skippedUnmappable: Integer
    }
}

/// `catalog have <ids>...` — report which of the given ids are already present.
@readonly
operation CatalogHave {
    input := with [CommonOutput] {
        @required
        ids: WorkIdList
    }
    output := {
        results: PresenceList
    }
}

/// `catalog neighborhood <seeds>...` — bounded traversal of the citation graph.
@readonly
operation CatalogNeighborhood {
    input := with [CommonOutput] {
        @required
        seeds: WorkIdList
        /// --dir  (the ONLY operation with an explicit direction; the provider
        /// cited-by/refs pairs imply it in the verb instead)
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

/// `catalog stats` — aggregate counts over the metadata network.
@readonly
operation CatalogStats {
    input := with [CommonOutput] {}
    output := {}
}

// ---------------------------------------------------------------------------
// Collection (Layer 3) — the consolidated research-document store
// ---------------------------------------------------------------------------

structure AssignedAnchor {
    doc: DocSlug
    headingLine: Integer
    id: String
    title: String
}

list AssignedAnchorList {
    member: AssignedAnchor
}

structure DocSummary {
    doc: DocSlug
    title: String
    updated: String
}

list DocSummaryList {
    member: DocSummary
}

structure ReadingEntry {
    doc: DocSlug
    anchor: String
    work: WorkId
    role: String
}

list ReadingEntryList {
    member: ReadingEntry
}

resource Collection {
    identifiers: { doc: DocSlug }
    operations: [
        CollectionNew
        CollectionPath
        CollectionRm
    ]
    collectionOperations: [
        CollectionList
        CollectionCheck
        CollectionIndex
        CollectionImport
        CollectionAssignIds
        CollectionReadingList
    ]
}

/// `collection new <doc>` — create a Research Document; prints its path.
operation CollectionNew {
    input := with [CommonOutput] {
        @required
        doc: DocSlug
        /// --title  (H1 heading; defaults to the slug)
        title: String
        /// --force  (overwrite an existing document with this slug)
        force: Boolean
    }
    output := {
        path: String
    }
}

/// `collection path <doc>` — print an existing document's absolute path.
@readonly
operation CollectionPath {
    input := with [CommonOutput] {
        @required
        doc: DocSlug
    }
    output := {
        path: String
    }
}

/// `collection rm <doc>` — delete a document and reindex.
operation CollectionRm {
    input := with [CommonOutput] {
        @required
        doc: DocSlug
    }
    output := {}
}

/// `collection list` — list every document in the store.
@readonly
operation CollectionList {
    input := with [CommonOutput] {}
    output := {
        docs: DocSummaryList
    }
}

/// `collection check [doc] [--all]` — lint against the required frontmatter.
/// Advisory: reports findings and never fails.
@readonly
operation CollectionCheck {
    input := with [CommonOutput] {
        /// doc slug (omit with --all)
        doc: DocSlug
        /// --all  (check every document)
        all: Boolean
    }
    output := {
        findings: StringList
    }
}

/// `collection index` — regenerate INDEX.md from every document's frontmatter.
operation CollectionIndex {
    input := with [CommonOutput] {}
    output := {}
}

/// `collection import <file>` — adopt an existing markdown file, normalizing
/// its frontmatter.
operation CollectionImport {
    input := with [CommonOutput] {
        @required
        file: String
        /// --doc  (slug override; default from frontmatter, else filename stem)
        doc: DocSlug
        /// --mv  (remove the source file after a successful import)
        mv: Boolean
    }
    output := {
        doc: DocSlug
    }
}

/// `collection assign-ids [--dry-run]` — mint `^r-…` anchors for every `##`
/// heading that lacks one and append them to the heading line. The one
/// operation in this layer that writes to document bodies: node identity is
/// tooling-owned, and anchors are store-global so they never need rewriting.
operation CollectionAssignIds {
    input := with [CommonOutput] {
        /// --dry-run  (report what would be assigned; write nothing)
        dryRun: Boolean
    }
    output := {
        assigned: AssignedAnchorList
    }
}

/// `collection reading-list` — list every node carrying a `reading` property
/// alongside its catalog work, grouped by role.
@readonly
operation CollectionReadingList {
    input := with [CommonOutput] {}
    output := {
        entries: ReadingEntryList
    }
}

// ---------------------------------------------------------------------------
// Providers — upstream reads. Results are written to the catalog by default;
// `--skip-push` suppresses the write and `--emission` yields the wire frame.
// ---------------------------------------------------------------------------

/// `openalex get <id>` — fetch one entity, inferring its type from the id.
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

/// `openalex search <entity> <query>` — full-text search over a collection.
@readonly
operation OpenAlexSearch {
    input := with [CommonOutput] {
        @required
        entity: OpenAlexEntity
        @required
        query: String
    }
    output := {
        results: WorkList
    }
}

/// `openalex find <entity> <filters>...` — filter by `key:value` expressions.
@readonly
operation OpenAlexFind {
    input := with [CommonOutput] {
        @required
        entity: OpenAlexEntity
        @required
        filters: StringList
    }
    output := {
        results: WorkList
    }
}

/// `openalex autocomplete <entity> <q>` — complete entity names by prefix.
@readonly
operation OpenAlexAutocomplete {
    input := with [CommonOutput] {
        @required
        entity: OpenAlexEntity
        @required
        q: String
    }
    output := {
        results: WorkList
    }
}

/// `openalex cited-by <id>` — forward citations. NOTE: direction is implied by
/// the verb; only CatalogNeighborhood exposes an explicit --dir.
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

/// `semanticscholar search <entity> <query>`. NOTE: no find/autocomplete twin —
/// a coverage gap against OpenAlex.
@readonly
operation SemanticScholarSearch {
    input := with [CommonOutput] {
        @required
        entity: SemanticScholarEntity
        @required
        query: String
    }
    output := {
        results: WorkList
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

/// `arxiv get <id>` — accepts `arxiv:…`, a bare numeric id, an old-style
/// `hep-th/…` id, or a full URL.
@readonly
operation ArxivGet {
    input := with [CommonOutput] {
        @required
        id: WorkId
    }
    output := {
        work: WorkRecord
    }
}

/// `arxiv search <query>`. NOTE: no entity parameter — arXiv has one searchable
/// collection, so the query carries field operators (`ti:`/`au:`/`cat:`) instead.
@readonly
operation ArxivSearch {
    input := with [CommonOutput] {
        @required
        query: String
    }
    output := {
        results: WorkList
    }
}

/// `crossref refs <id>` — from Crossref's deposited reference list.
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

/// `opencitations refs <id>` — from the OpenCitations COCI index.
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

// ---------------------------------------------------------------------------
// Operations that belong to no layer
// ---------------------------------------------------------------------------

/// `web` — print `<server_url>/web`, the Web UI URL for the configured server.
@readonly
operation Web {
    input := {}
    output := {
        url: String
    }
}

/// `store list` — configured stores ([stores.<name>] in the config file) and
/// which one is active.
operation StoreList {
    input := {}
    output := {}
}

/// `store use <name>` — switch the active store; rewrites `active_store` in
/// the config file and reports what each side holds.
operation StoreUse {
    input := {
        @required
        name: String
    }
    output := {}
}

/// `store diff <a> [b]` — counts per side and which works each side lacks.
/// A store spec is a configured name or a bare base URL; `b` defaults to the
/// active store.
operation StoreDiff {
    input := {
        @required
        a: String
        b: String
    }
    output := {}
}

/// `store sync <source> <dest>` — replay everything the destination lacks
/// from the source: catalog works/edges/artifacts over `/export/*` plus the
/// ordinary write routes, then Research Documents node-by-node (rules in
/// crates/l3-sync). Idempotent; both directions use the same path.
operation StoreSync {
    input := {
        @required
        source: String
        @required
        dest: String
        /// --dry-run  (print plan counts; push nothing)
        dryRun: Boolean
        /// --doc  (restrict to one Research Document; skips the catalog phases)
        doc: DocSlug
        /// --force  (with --doc: bypass the destination's parse-warning bounce)
        force: Boolean
    }
    output := {}
}

/// `rename <file>` — rename a work file to its canonical bibliographic
/// filename. NOTE: operates on a loose file on disk, not on anything the store
/// holds — the one operation with no store contact at all.
operation Rename {
    input := with [CommonOutput] {
        @required
        file: String
        /// --author  (first author's surname; may contain spaces or particles)
        @required
        author: String
        /// --et-al
        etAl: Boolean
        @required
        year: Integer
        /// --title  (slugified for the filename)
        @required
        title: String
        /// --dry-run  (print the proposed path; rename nothing)
        dryRun: Boolean
    }
    output := {
        path: String
    }
}

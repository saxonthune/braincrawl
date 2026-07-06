# Chunk a stored fulltext into a citation-carrying AI projection

## Motivation

The Library stores raw fulltext PDFs (role `fulltext`); `extract-text` pulls flat text
but only prints it — nothing persists an AI-friendly, retrieval-ready projection. For
textbook-scale sources (e.g. De Landa on `openalex:W1484319374`) we need the work cut
into chunks carrying **page-number provenance** so an agent's answer can cite back into
the book ("p. 42"). This is the projection half of the AI-friendly-storage need; the
store-side search verb that retrieves over these chunks is a SEPARATE task.

Roles are already open-ended (a `chunks` role round-trips through put_content/get_content —
verified). The grammar for the new verb is already added to `braincrawl-cli.tsp` and
`braincrawl-cli.smithy` (`Content`); do NOT re-edit the grammar docs.

## Do NOT

- Do NOT add OCR, or any image-to-text path. Scanned/image-only PDFs are refused (or
  partially skipped with `--allow-partial`), never OCR'd.
- Do NOT store anything when input is an external `--file`: file input is PIPE-ONLY,
  emits JSON to stdout, never writes the store. Storing requires a pushed work id.
- Do NOT use semantic-chunking libraries. Use the `text-splitter` crate only.
- Do NOT re-edit `braincrawl-cli.tsp` / `braincrawl-cli.smithy` — grammar already added.
- Do NOT invent a `payloads.kind` enum or constrain roles — roles are free-form slugs.
- Do NOT fetch anything from a provider; this verb is store-local + local file only.

## Plan

### 1. Add the `text-splitter` dependency

`apps/cli/Cargo.toml`: add `text-splitter` with a tokenizer feature so `--max-tokens` is
genuine token counts (e.g. `text-splitter = { version = "0.x", features = ["tiktoken-rs"] }`
plus `tiktoken-rs`). Use `cl100k_base`. Pick the current published version.

### 2. Paginated PDF text extraction

`apps/cli/src/pdf_text.rs`: add `extract_pages(bytes) -> Result<Vec<String>>` returning
one String per page. `pdf-extract 0.7` exposes `OutputDev::begin_page(page_num, …)`;
implement a custom `OutputDev` (model it on the crate's `PlainTextOutput`) that pushes a
new page buffer on `begin_page` and appends text to the current page. Reuse `normalize`
per page. Keep the existing `extract_text` (flat) intact — `extract-text` still uses it.

### 3. Chunk module

New `apps/cli/src/chunk.rs` (add `pub mod chunk;` to `apps/cli/src/lib.rs`):

- `chunk_pages(pages: &[String], max_tokens, overlap) -> Vec<ChunkRecord>`.
- Build one joined string plus a char-range→page map (page N spans `[start,end)` in the
  joined text). Split the joined string with `text-splitter`'s `TextSplitter` configured
  `ChunkConfig::new(max_tokens).with_overlap(overlap)` and a tiktoken sizer. Use
  `chunk_indices()` to get each chunk's byte offset, map offset→`page_start`, and the
  chunk's end offset→`page_end`.
- `ChunkRecord { seq, text, page_start, page_end, token_count, section_path }`.
  `section_path` is best-effort: empty `[]` unless markdown-style headings are detectable
  (flat PDF text usually has none — leave empty, do not fabricate).
- Faithfulness: a page whose normalized text has fewer than 5 non-whitespace chars is
  "no text layer". Return the list of such page numbers alongside the chunks.

### 4. CLI wiring

`apps/cli/src/cli.rs`: add `Chunk(ChunkArgs)` to the `Namespace` enum
(`#[command(about = "Partition a work's stored fulltext into a citation-carrying chunks artifact")]`)
and:

```rust
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
```

### 5. Handler

`apps/cli/src/main.rs`, `Namespace::Chunk(args)`:

- Exactly one source: require `id` XOR `file`. If `file` is set, force stdout (never
  store); if `id` is set, read stored fulltext via `store.get_content(id, "fulltext")`
  (same outcomes as `ExtractText`: error on Absent/Pending, reject non-PDF).
- `extract_pages` → faithfulness check. If any page has no text layer and NOT
  `--allow-partial`: return an error naming the count, e.g.
  `"42/330 pages have no text layer (scanned/image-only); chunk provides no OCR. Re-run with --allow-partial to chunk the rest."`. With `--allow-partial`: drop those pages, keep chunking, record them.
- `chunk_pages` → build the output object:
  `{ work, chunker:{max_tokens,overlap,tokenizer}, pages_total, pages_faithful, pages_skipped:[…], chunks:[…] }`.
- If storing (`id`, no `--stdout`): `put_content(id, &args.role, json_bytes, "application/json", source="chunk")`; respect `--force` (without it, if a current `chunks` payload exists, skip with a message — mirror `fetch-content` semantics). Print a stderr summary (chunk count, pages_faithful/total). If `--stdout` or `file`: print the JSON to stdout.

### 6. Docs (counterpart of the code change)

- `.rhidoc/02-architecture/01-core/01-layers.md` (doc02.01.01): rewrite the
  `abstract | fulltext` framing. `kind`/role is an **open-ended slug**: a work holds raw
  fulltext plus arbitrarily many secondary derived/compressed artifacts (`chunks` is the
  first). Update the `kind` column description and the "abstract vs fulltext vs nothing"
  bullet accordingly. Keep it declarative; do not narrate the change.
- `.rhidoc/01-product/01-glossary.md` (doc01.01): add a verbalized-fact entry — `chunk`
  is a self-contained CLI tool braincrawl provides that partitions a stored fulltext into
  citation-carrying pieces (page provenance), a secondary derived artifact of a work.
- Run `rhidoc manifest` if the workspace has that regeneration step; otherwise leave the
  MANIFEST as-is (do not hand-edit it).

## Files to Modify

- `apps/cli/Cargo.toml` — add `text-splitter` (+ tiktoken feature).
- `apps/cli/src/pdf_text.rs` — add `extract_pages` + a paginated `OutputDev`.
- `apps/cli/src/chunk.rs` — NEW: chunking + `ChunkRecord` + faithfulness.
- `apps/cli/src/lib.rs` — `pub mod chunk;`.
- `apps/cli/src/cli.rs` — `ChunkArgs` + `Namespace::Chunk`.
- `apps/cli/src/main.rs` — `Namespace::Chunk` handler.
- `.rhidoc/02-architecture/01-core/01-layers.md` — open-ended roles rewrite.
- `.rhidoc/01-product/01-glossary.md` — `chunk` glossary entry.

## Verification

```bash
cargo build
cargo test -p braincrawl-cli
# happy path: stored born-digital book → chunks with page numbers, round-trips
cargo run -p braincrawl-cli -- chunk openalex:W1484319374 --stdout --max-tokens 400 \
  | python3 -c 'import sys,json; d=json.load(sys.stdin); c=d["chunks"][0]; print("chunks",len(d["chunks"]),"pages",d["pages_total"],"first_chunk_pages",c["page_start"],c["page_end"]); assert c["page_start"]>=1 and len(d["chunks"])>0'
# store + read back
cargo run -p braincrawl-cli -- chunk openalex:W1484319374 --force >/dev/null 2>&1
cargo run -p braincrawl-cli -- get openalex:W1484319374 --role chunks 2>/dev/null | head -c 200
```

## Out of Scope

- Store-side search / retrieval over chunks (separate task — the retrieval surface).
- Embeddings / vector index.
- LLM contextual-retrieval per-chunk headers.
- Section/heading provenance beyond best-effort empty `section_path`.
- Artifact delete (the store has no delete endpoint — `DELETE` returns 405; separate task).
- Merging the two OpenAlex records for the De Landa book.

## Notes

- The `chunks` role round-trips today (tested); no schema/server change needed.
- A leftover `chunks` v1 (`"roundtrip probe"`) sits on `openalex:W1484319374`; the first
  real `chunk --force` supersedes it (v2 `is_current`). No delete path exists to remove v1.
- `text-splitter` `chunk_indices()` returns byte offsets — map via the page char-range map;
  watch multi-byte UTF-8 (the De Landa extraction contains non-ASCII).
- The De Landa extraction has intra-word spacing artifacts ("trave llin g"); that is an
  extractor-quality issue, out of scope — chunk the text as extracted.

## Surface after this phase

- New CLI verb `chunk` (`ChunkArgs` in `apps/cli/src/cli.rs`, `Namespace::Chunk` handler in
  `apps/cli/src/main.rs`): stores a `chunks` JSON artifact for a stored work id, or streams
  to stdout for `--file`/`--stdout`; refuses no-text-layer PDFs unless `--allow-partial`.
- `apps/cli/src/chunk.rs`: `chunk_pages(&[String], max_tokens, overlap) -> Vec<ChunkRecord>`
  and `ChunkRecord { seq, text, page_start, page_end, token_count, section_path }`.
- `apps/cli/src/pdf_text.rs`: `extract_pages(&[u8]) -> Result<Vec<String>>` (per-page);
  existing `extract_text` (flat) unchanged and still used by `extract-text`.
- `chunks` artifact JSON shape: `{ work, chunker, pages_total, pages_faithful, pages_skipped, chunks[] }`.
- Roles remain open-ended free-form slugs; no `kind` enum introduced.

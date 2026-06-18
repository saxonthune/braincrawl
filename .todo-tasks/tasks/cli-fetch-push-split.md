# Split fetch-content into composable download + push + read-back (Unix flow)

## Motivation

`fetch-content` does too much in one opaque step: it resolves the OA URL,
downloads the bytes, and writes them to the shared L1 corpus all at once — the
only feedback is `stored: 787145 bytes, mime=application/pdf`. The user never
sees the artifact before it lands in the shared store, can't verify it's the
right file (vs. a JS-walled 403 page), and can't decide *whether* to push.

This splits the pipeline into three composable primitives so download (a
read-only fetch the user can inspect) is separated from push (a deliberate write
to the shared corpus), with a read-back verb to complete the loop:

```
braincrawl fetch-pdf openalex:W2304167012 > paper.pdf      # web → stdout, no store write
braincrawl push-pdf  openalex:W2304167012 < paper.pdf      # stdin → store
braincrawl get-pdf   openalex:W2304167012 > again.pdf      # store → stdout
braincrawl fetch-pdf openalex:W2304167012 | braincrawl push-pdf openalex:W2304167012  # piped
```

`fetch-content` stays as the all-in-one convenience, unchanged.

## Do NOT

- Do NOT modify the behavior of `fetch-content` (`Namespace::FetchContent` /
  `fetch_content::fetch_content`). Leave its orchestration, idempotency, LinkOnly
  fallback, and output exactly as-is. The only allowed change to existing logic is
  promoting two private helpers to `pub` (see Plan step 1).
- Do NOT rewrite `fetch-content` on top of the new primitives. That is extra risk
  for no scope benefit; it already shares the helper functions.
- Do NOT give `fetch-pdf` a store-write, an idempotency check, or a LinkOnly
  fallback. It is read-only: resolve URL → download → emit bytes. If there is no
  downloadable artifact URL, it errors out (LinkOnly is a store-write concept that
  belongs only to `fetch-content`).
- Do NOT print anything other than the raw artifact bytes to **stdout** in
  `fetch-pdf` and `get-pdf`. All diagnostics (resolved URL, mime, byte count)
  go to **stderr** via `eprintln!`, so a pipe / redirect stays byte-clean. Follow
  the `extract-text` precedent (text to stdout, `eprintln!("extracted: …")` to
  stderr).
- Do NOT add PDF→text extraction or async fetch-queue plumbing — separate tasks
  (`cli-pdf-extraction` already merged; `local-fetch-queue` is pending).
- Do NOT invent a new store HTTP method. `push-pdf` uses the existing
  `StoreClient::put_content`; `get-pdf` uses the existing `StoreClient::get_content`.

## Plan

### 1. Expose the resolve + download primitives (`apps/cli/src/fetch_content.rs`)

The file already factors the two halves as private helpers:
`resolve_artifact_url(work, from, email)` and `download_artifact(url) -> (bytes, mime)`.

- Change `fn resolve_artifact_url` and `fn download_artifact` to `pub fn` so the new
  download verb can call them. (`is_valid_pdf`, `resolve_oa_url_from_attrs`,
  `extract_doi` are already `pub`.)
- Add a new orchestration function that the `fetch-pdf` verb calls — it mirrors
  the first half of `fetch_content()` but stops before the store write:

  ```rust
  /// Resolve and download the artifact for `id` WITHOUT writing to the store.
  /// Returns (bytes, mime, artifact_url). Errors if no downloadable URL exists
  /// (no LinkOnly fallback — that belongs to fetch_content). Applies the same
  /// %PDF validation as fetch_content when require_pdf / mime / .pdf suffix.
  pub fn fetch_artifact_bytes(
      store: &StoreClient,
      unpaywall_email: Option<&str>,
      id: &str,
      from: Source,
      require_pdf: bool,
  ) -> Result<(Vec<u8>, String, String), Box<dyn std::error::Error>>
  ```

  Body: `store.get_work(id)?` (error "work not found in store: {id}" on None) →
  `resolve_artifact_url(&work, &from, unpaywall_email)?` (error "no downloadable
  artifact URL found for {id}" on None) → `download_artifact(&url)?` → reuse the
  exact `require_pdf || mime.contains("pdf") || url ends_with(".pdf")` validation
  block from `fetch_content` (factor the validation into a small shared
  `pub fn validate_pdf_if_expected(bytes, mime, url, require_pdf) -> Result<(), _>`
  if it reduces duplication; otherwise inline-copy it).

- Add a mime-sniff helper for `push-pdf`:

  ```rust
  /// Best-effort mime for raw bytes with no HTTP response to read: %PDF magic →
  /// application/pdf, otherwise application/octet-stream.
  pub fn sniff_mime(bytes: &[u8]) -> &'static str
  ```

- Add `#[cfg(test)]` unit tests in the existing `mod tests`: `sniff_mime` returns
  `application/pdf` for `b"%PDF-1.7"` and `application/octet-stream` for other
  bytes; (if you factor it) `validate_pdf_if_expected` rejects HTML when
  `require_pdf` and accepts a `%PDF` body.

### 2. Register the three verbs (`apps/cli/src/cli.rs`)

Add three `Namespace` variants next to `FetchContent` / `ExtractText`:

```rust
#[command(name = "fetch-pdf", about = "Resolve and download a work's fulltext artifact to stdout (no store write)")]
FetchPdf(FetchPdfArgs),
#[command(name = "push-pdf", about = "Store fulltext bytes from stdin or a file as a work's fulltext payload")]
PushPdf(PushPdfArgs),
#[command(name = "get-pdf", about = "Read a work's stored fulltext payload bytes to stdout")]
GetPdf(GetPdfArgs),
```

With arg structs (follow the `FetchContentArgs` / `ExtractTextArgs` style):

```rust
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
pub struct PushPdfArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Read bytes from this file instead of stdin
    pub file: Option<String>,
    /// Content mime type (default: sniff %PDF, else application/octet-stream)
    #[arg(long)]
    pub mime: Option<String>,
    /// Rights value: open | link_only | restricted
    #[arg(long, default_value = "open")]
    pub rights: String,
    /// Optional source label recorded with the payload
    #[arg(long)]
    pub source: Option<String>,
    /// Optional source URL recorded with the payload
    #[arg(long = "source-url")]
    pub source_url: Option<String>,
}

#[derive(Args)]
pub struct GetPdfArgs {
    /// Work id in ns:value form (e.g. openalex:W2304167012)
    pub id: String,
    /// Write bytes to this path instead of stdout
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}
```

### 3. Dispatch the three verbs (`apps/cli/src/main.rs`)

Add three match arms. Import the `Namespace::{FetchPdf, PushPdf, GetPdf}` variants
in the existing `use braincrawl_cli::cli::{…}` list.

- **FetchPdf**: build `StoreClient`, map `args.from` to `Source` (copy the
  `match fc.from.as_str()` block from the FetchContent arm), call
  `fetch_content::fetch_artifact_bytes(...)`. On success write the bytes either to
  `args.output` (via `std::fs::write`) or to stdout as raw bytes — use
  `std::io::Write` on `std::io::stdout().lock()` (`stdout.write_all(&bytes)?`), NOT
  `println!`. Log `eprintln!("fetched: {} bytes, mime={}, url={}", …)` to stderr.

- **PushPdf**: read input bytes — if `args.file` is `Some`, `std::fs::read(path)?`;
  else read all of stdin into a `Vec<u8>` (`std::io::Read::read_to_end` on
  `std::io::stdin().lock()`). Error if the buffer is empty
  ("no input bytes (provide a file arg or pipe bytes on stdin)"). Resolve mime:
  `args.mime.as_deref().unwrap_or_else(|| fetch_content::sniff_mime(&bytes))`.
  Call `store.put_content(&args.id, "fulltext", bytes, mime, &args.rights,
  args.source.as_deref(), args.source_url.as_deref())?`. Report
  `eprintln!("pushed: {} bytes, mime={}, rights={}", …)` to stderr.

- **GetPdf**: build `StoreClient`, `store.get_content(&args.id, "fulltext")?`, match
  on `ContentOutcome` exactly like the `ExtractText` arm does, but for the `Bytes`
  case write raw bytes to `args.output` or stdout (`write_all`, not `print!`) and
  log `eprintln!("read: {} bytes, mime={}", …)`. Reuse the same error messages as
  ExtractText for `Absent` / `Pending` / `Restricted` / `Redirect`.

## Files to Modify

- `apps/cli/src/fetch_content.rs` — promote `resolve_artifact_url`/`download_artifact`
  to `pub`; add `fetch_artifact_bytes`, `sniff_mime` (+ optional
  `validate_pdf_if_expected`); add unit tests for `sniff_mime` (+ validation).
- `apps/cli/src/cli.rs` — add `FetchPdf`/`PushPdf`/`GetPdf` `Namespace` variants and
  their `Args` structs.
- `apps/cli/src/main.rs` — import the three variants; add three dispatch arms.

## Verification

```bash
cargo build --bin braincrawl
cargo test -p braincrawl-cli
cargo run --bin braincrawl -- fetch-pdf --help
cargo run --bin braincrawl -- push-pdf --help
cargo run --bin braincrawl -- get-pdf --help
```

The `--help` invocations confirm the verbs registered and clap parses their args.
Live end-to-end (`fetch-pdf | push-pdf`, then `get-pdf`) needs a running server with
a seeded work and is a manual check — not part of the gate.

## Out of Scope

- PDF→text extraction (`extract-text`, already merged).
- Async fetch-queue plumbing (`local-fetch-queue`, pending).
- Removing or refactoring `fetch-content`.
- Content kinds other than `fulltext` (the three verbs hardcode `"fulltext"`,
  matching `extract-text`).

## Notes

- Stdout-purity is the subtle risk: `fetch-pdf` and `get-pdf` MUST use
  `write_all` on a locked stdout handle and route every human-readable line to
  stderr, or piping into `push-pdf` will corrupt the payload. `extract-text` is the
  precedent to copy.
- Naming asymmetry is intentional: `fetch-pdf` reaches the **web**, `get-pdf` reads
  the **store**. This mirrors `fetch-content` (web) vs the store round-trip.
- `push-pdf` accepts any `--rights`; the server validates. Default `open` matches
  what `fetch-content` writes for a downloaded artifact.

## Surface after this phase

- New CLI verbs: `braincrawl fetch-pdf <id> [--from] [--require-pdf] [-o path]`,
  `braincrawl push-pdf <id> [file] [--mime] [--rights] [--source] [--source-url]`,
  `braincrawl get-pdf <id> [-o path]`. All operate on content kind `"fulltext"`.
- `fetch_content::fetch_artifact_bytes(store, email, id, from, require_pdf) ->
  Result<(Vec<u8>, String, String)>` (bytes, mime, url) — read-only resolve+download.
- `fetch_content::sniff_mime(&[u8]) -> &'static str`.
- `fetch_content::resolve_artifact_url` and `fetch_content::download_artifact` are
  now `pub`.
- `fetch-content` is unchanged and still does the all-in-one resolve+download+store
  with idempotency and LinkOnly fallback.
- `extract-text` is unchanged.
</content>
</invoke>

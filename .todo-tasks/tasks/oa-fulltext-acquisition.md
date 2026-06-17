# Acquire open-access fulltext artifacts from the open web into L1

## Motivation

L1 promises "abstract now, fulltext on demand." The storage substrate already exists
end to end — `PayloadKind::Fulltext`, `put_content`/`get_content`, `Rights`
(Open/Restricted/LinkOnly), blob backends, versioning, `ContentOutcome` — and the
server already exposes `PUT`/`GET /works/*path/content/{kind}`. What is missing is the
**acquisition path**: a CLI verb that goes to the open web, downloads a work's
fulltext artifact, and stores it. The OA URL is already in our stored OpenAlex attrs
(`open_access.oa_url`, `primary_location.pdf_url`), so for OpenAlex-sourced works we can
acquire with no metadata round-trip; Unpaywall is a DOI-keyed fallback.

## Do NOT

- Do NOT name the verb or payload after `pdf`. The artifact is not always a PDF (PDF,
  HTML, or JATS-XML). The verb is **`fetch-content`**; store the raw bytes as a
  `Fulltext` payload with the response's true mime.
- Do NOT add a `content` sub-namespace. There is only one genuinely-new operation, so
  add `fetch-content` as a single top-level `Namespace` variant. Local content reads stay
  conceptually under `store` (not in scope to add here).
- Do NOT do text extraction. Storing the raw artifact is the whole job; normalized-text
  extraction under an `/extracted/` keypath is a deliberate future task.
- Do NOT add outbound HTTP to the server. The fetch runs CLI-side and PUTs bytes to the
  existing content endpoint — the server stays a pure offline store.
- Do NOT wire in grey sources (LibGen / Anna's Archive / Sci-Hub).
- Do NOT modify the OpenAlex or Semantic Scholar provider modules. This is additive.

## Plan

### 1. `store_client.rs` — content methods
Add two methods to `StoreClient` (mirror the existing `get_work`/`put_work` style,
including `apply_auth` and the `ClientError::Server` non-2xx mapping):

- `put_content(&self, alias: &str, kind: &str, bytes: Vec<u8>, mime: &str, rights: &str, source: Option<&str>, source_url: Option<&str>) -> Result<()>`
  → `PUT {base}/works/{alias}/content/{kind}` with the raw `bytes` as the body and
  **query params** `mime`, `rights`, and (when present) `source`, `source_url`. Server
  returns 200 on success. (See `apps/server/src/lib.rs` `handler_works_put`: params are
  `mime` [required], `rights` [required], `source`, `source_url`, `fetched_at`.)
  Set `fetched_at` to an RFC3339 now (reuse the timestamp helper pattern already in the
  openalex `mapping.rs`).
- `get_content(&self, alias: &str, kind: &str) -> Result<ContentOutcome>` where
  `ContentOutcome` is a small CLI-local enum { Bytes{bytes,mime}, Redirect(String),
  Pending, Restricted, Absent }. Map the server responses: 200→Bytes (read
  `content-type` header + body), 3xx/Location→Redirect, 202→Pending, 451→Restricted,
  404→Absent. Use a non-redirect-following client for this call (or inspect status before
  the client auto-follows) so a `RedirectUrl` is observable rather than transparently followed.

Confirm the exact accepted `rights` strings by reading `parse_rights` in
`apps/server/src/lib.rs` and pass those literal forms (do not guess — use what
`parse_rights` accepts; expected: open / restricted / link_only or similar).

### 2. New acquisition module `apps/cli/src/fetch_content.rs`
Register it in `apps/cli/src/lib.rs` alongside the existing `pub mod openalex;` /
`pub mod semanticscholar;` / `pub mod store_client;` declarations.

`pub fn fetch_content(store: &StoreClient, unpaywall_email: Option<&str>, id: &str, from: Source, require_pdf: bool, force: bool) -> Result<Outcome>`:

1. **Idempotency:** unless `force`, call `store.get_content(id, "fulltext")`; if it returns
   `Bytes`, short-circuit with an "already present" outcome (no network).
2. **Resolve an artifact URL:**
   - `auto`/`openalex`: `store.get_work(id)` → from `WorkView.attrs` try, in order,
     `attrs.primary_location.pdf_url`, `attrs.open_access.oa_url`,
     `attrs.best_oa_location.pdf_url` (all may be absent/null). `WorkView.attrs` is the
     merged provider attrs (confirmed: `get_work` merges assertions into `attrs`).
   - `auto` fallback / `unpaywall`: if no stored URL and the work has a `doi:` alias
     (read from `WorkView.aliases`, namespace `doi`), and `unpaywall_email` is set, GET
     `https://api.unpaywall.org/v2/{doi}?email={email}` and read
     `best_oa_location.url_for_pdf` (fall back to `best_oa_location.url`). If no email,
     skip Unpaywall with a clear message.
3. **Download:** HTTP GET with redirects followed, a 60s timeout, a descriptive
   User-Agent (e.g. `braincrawl/<ver> (+fetch-content)`), and a size cap (default 50 MB —
   reject larger). Capture the response `content-type` as the mime.
4. **Validate:** if `require_pdf` (or the resolved URL/mime indicates PDF), verify the body
   starts with the `%PDF` magic bytes; reject HTML/error bodies masquerading as PDFs with a
   clear error.
5. **Store:** `store.put_content(id, "fulltext", bytes, mime, "<open-rights-literal>", source, Some(resolved_url))`
   where `source` is `"openalex-oa"` or `"unpaywall"`.
6. **No OA available:** if a landing page is known but no downloadable artifact (or only a
   paywalled location), store a `LinkOnly` payload — `put_content` with the
   link-only rights literal, an empty/zero-length body is NOT appropriate; instead pass the
   landing URL as `source_url` and skip bytes by using a dedicated link-only path. If the
   server's content PUT requires a body, store no bytes by sending an empty body with
   `rights=link_only` and `source_url=<landing>` so a later `get_content` returns Redirect.
   (Verify against `put_content`/`get_content` semantics in `crates/core/src/usecases.rs`:
   LinkOnly records the descriptor + `source_url`, no blob.)

### 3. CLI surface — `cli.rs`
Add a `Namespace::FetchContent(FetchContentArgs)` variant (after `Semanticscholar`, before
`Graph`, to minimize churn) with:
```
#[derive(Args)] pub struct FetchContentArgs {
    /// Work id in ns:value form (e.g. openalex:W2165758805 or doi:10.x/y)
    pub id: String,
    /// Where to resolve the artifact URL from
    #[arg(long, default_value = "auto")] pub from: String,   // auto|openalex|unpaywall
    /// Re-fetch even if a fulltext payload already exists
    #[arg(long)] pub force: bool,
    /// Only accept a PDF artifact
    #[arg(long = "require-pdf")] pub require_pdf: bool,
}
```
Give the command `#[command(name = "fetch-content", about = "Acquire a work's fulltext artifact from the open web into the store")]`.

### 4. Dispatch — `main.rs`
Add a `Namespace::FetchContent(fc)` arm: build a `StoreClient` (with auth token, like the
other arms), call `fetch_content::fetch_content(...)`, and render a concise outcome
(stored / already-present / link-only / no-oa-found) via the existing `render`/`Envelope`
or a small status print consistent with the other arms.

### 5. Config — `config.rs`
Add `unpaywall_email: Option<String>` to both `Config` and `ConfigFile`, resolved in
`Config::resolve()` as `std::env::var("BRAINCRAWL_UNPAYWALL_EMAIL").ok().or_else(|| file.unpaywall_email)`.

## Files to Modify

- `apps/cli/src/store_client.rs` — `put_content`, `get_content`, local `ContentOutcome` enum.
- `apps/cli/src/fetch_content.rs` — NEW acquisition module.
- `apps/cli/src/lib.rs` — register `pub mod fetch_content;`.
- `apps/cli/src/cli.rs` — `Namespace::FetchContent` + `FetchContentArgs`.
- `apps/cli/src/main.rs` — dispatch arm + import.
- `apps/cli/src/config.rs` — `unpaywall_email`.
- Inline `#[cfg(test)] mod tests` in `store_client.rs` and/or `fetch_content.rs`.

## Verification

```bash
cargo test -p braincrawl-cli --lib
cargo build --release --bin braincrawl
# Unit tests must cover: URL resolution precedence (primary_location.pdf_url >
# open_access.oa_url > best_oa_location.pdf_url), the %PDF magic-byte validation
# (accept a %PDF body, reject an <html> body), and the store_client query-param
# construction for put_content. Network calls in fetch_content must be structured so
# the URL-resolution and validation logic is unit-testable WITHOUT live HTTP (e.g. split
# "resolve url from WorkView json" and "validate bytes" into pure functions).
```

## Out of Scope

- Text extraction → normalized-text payload under an `/extracted/` keypath (future task).
- Grey-source acquisition (LibGen / Anna's Archive / Sci-Hub) — policy decision.
- Server-side fetch (`POST …/fetch-content` to dedup across consumers) — revisit later.
- Internet Archive / HathiTrust monograph acquisition — separate provider-shaped task.
- A `store content <id>` local-read verb — separate small task; not needed to ship acquisition.

## Notes

- Unpaywall is a fallback, not the primary path — the OA URL already rides in stored
  OpenAlex attrs (curated push keeps `open_access` + `primary_location`).
- The `Rights` enum already models Open vs LinkOnly vs Restricted — lean on it; do not
  invent new payload state.
- Keep network code thin and the decision logic (URL resolution, validation) in pure
  functions so the verification gate can test it without hitting the internet.
- Match the established provider-module conventions (`openalex/`, `semanticscholar/`):
  blocking reqwest, `ClientError` mapping, RFC3339 timestamp helper.

## Surface after this phase

- `StoreClient::put_content(alias, kind, bytes, mime, rights, source, source_url)` and
  `StoreClient::get_content(alias, kind) -> ContentOutcome` exist on the CLI store client.
- A CLI-local `ContentOutcome` enum { Bytes, Redirect, Pending, Restricted, Absent }.
- `pub mod fetch_content;` with `fetch_content(...)` acquisition entry point.
- `braincrawl fetch-content <id> [--from auto|openalex|unpaywall] [--force] [--require-pdf]`
  is a working top-level CLI command that stores a `Fulltext` payload (Open) or a
  LinkOnly redirect.
- `Config.unpaywall_email` (env `BRAINCRAWL_UNPAYWALL_EMAIL` > config.toml `unpaywall_email`).
- Unchanged and still relied upon: the OpenAlex and Semantic Scholar provider modules,
  the server content endpoints, and local store reads under `store`.

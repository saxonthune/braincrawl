# Local background fetch queue

## Motivation

Fetching is synchronous today. Slow, rate-limited, per-work HTTP (fulltext
acquisition, Crossref/OpenCitations backward-ref backfill) should run in the
background. Build a local-only async fetch process on proven prior art (the
database-backed job queue — `que`/Oban/River), with trait seams that partition
responsibilities so cleanly the same decomposition later maps onto Cloudflare
Queues + Durable Objects (doc02.02.00) without reshaping boundaries.

The deliverable is the **trait partition**, proven end-to-end by a worker that
claims jobs from SQLite and runs per-kind handlers. Core already uses narrow
`#[async_trait(?Send)]` seams (`crates/core/src/traits.rs`) and already has
`Coordinator` (per-work lock, line 129) and `Clock` (line 117) — reuse them.

## Architecture (where each piece lives — doc02.04)

- **core** is HTTP-free and platform-agnostic: it defines the queue *traits*, the
  job *types*, and the worker *driver algorithm* (`tick`). No reqwest, no Tokio
  spawning.
- **store-sqlite** binds the queue traits to a `fetch_jobs` table.
- **apps/server** (the entry point — it may name infrastructure) holds the
  concrete `FetchHandler` impls that do outbound HTTP, the enqueue HTTP endpoint,
  and the spawned worker loop.
- **CLI provider forks stay untouched.** Outbound-fetch logic in the CLI
  (`fetch_content.rs`, `refs_backfill/*`) is *reference* for the server-side
  handlers; do not move or delete it.

## Do NOT

- Do NOT put `reqwest` or any outbound HTTP in `crates/core`. Core defines the
  `FetchHandler` trait; concrete handlers that hit upstreams live in `apps/server`.
- Do NOT spawn Tokio tasks inside core. Core exposes a `tick(...)` async fn; the
  app owns the spawn-loop-and-sleep.
- Do NOT modify the CLI provider forks (`apps/cli/src/fetch_content.rs`,
  `apps/cli/src/refs_backfill/*`, `apps/cli/src/openalex/*`). Converting CLI
  commands to enqueue-by-default is a later phase (see Out of Scope).
- Do NOT invent a new per-work lock — reuse `Coordinator::with_lock`.
- Do NOT invent a new clock — reuse `Clock::now_rfc3339`.
- Do NOT add a `RateLimiter` trait. Leave a clear comment where it would slot into
  `tick`, but no seam without a policy in this slice.
- Do NOT use `SELECT`-then-`UPDATE` for claiming (race). Use claim-by-update.
- Do NOT add an abstract handler — abstracts are a separate inline change.

## Plan

### 1. Job types — `crates/core/src/types.rs`

Add, following the existing type style (serde, `DomainError` already present):

- `JobId(String)` (newtype over a GUID string).
- `JobKind` enum: `Fulltext`, `Refs`. Add `as_str`/`FromStr` (mirror `PayloadKind`
  conventions in this file).
- `JobState` enum: `Pending`, `Running`, `Done`, `Failed`.
- `JobSpec { kind: JobKind, target_id: String, params: serde_json::Value }` — what
  a producer hands to enqueue. `target_id` is any external id (resolved by the
  handler, like the rest of the API).
- `Job { id: JobId, kind: JobKind, target_id: String, params: serde_json::Value,
  attempts: u32 }` — what the worker receives on claim.

### 2. Queue traits — `crates/core/src/traits.rs`

Add three `#[async_trait(?Send)]` traits, matching the file's narrow-seam style:

- `JobEnqueuer` (producer seam, write-only):
  `async fn enqueue(&self, spec: JobSpec) -> Result<JobId, DomainError>;`
  Idempotent on `(kind, target_id)` while a job is `Pending`/`Running` — coalesce
  duplicate enqueues (get-or-create, like `get_or_create_alias`). Returns the
  existing or newly-minted `JobId`.

- `JobQueue` (worker lifecycle seam):
  ```
  async fn claim(&self, limit: u32, now: &str) -> Result<Vec<Job>, DomainError>;
  async fn complete(&self, id: &JobId) -> Result<(), DomainError>;
  async fn retry(&self, id: &JobId, run_after: &str, err: &str) -> Result<(), DomainError>;
  async fn fail(&self, id: &JobId, err: &str) -> Result<(), DomainError>;
  ```

- `FetchHandler` (per-kind unit of work; the only seam that knows an upstream):
  ```
  fn kind(&self) -> JobKind;
  async fn handle(&self, job: &Job) -> Result<(), DomainError>;
  ```

### 3. Worker driver — `crates/core/src/worker.rs` (new module, add `mod` to lib.rs)

A free async fn, generic over the traits, that runs **one tick**:

```
pub async fn tick<Q, C, Clk>(
    queue: &Q, coord: &C, clock: &Clk,
    handlers: &[Box<dyn FetchHandler>],
    batch: u32, policy: &BackoffPolicy,
) -> Result<usize, DomainError>
```

- `claim(batch, clock.now_rfc3339())`.
- For each job: take `coord.with_lock(&job.target_id)` (per-work dedup), find the
  handler whose `kind()` matches (else `fail` with "no handler"), call
  `handle(&job)`.
  - Ok → `queue.complete`.
  - Err → if `job.attempts + 1 >= policy.max_attempts` → `queue.fail`; else
    `queue.retry(id, run_after = now + backoff(attempts), err)`.
- `// RateLimiter would gate here` comment before dispatch.
- Return count of jobs processed (0 ⇒ caller can sleep longer).

Add `BackoffPolicy { max_attempts: u32, base_secs: u64, factor: u32 }` and a pure
`backoff(attempts, &policy) -> u64` (capped exponential). Compute `run_after` as
an RFC3339 string offset from `clock.now_rfc3339()` (parse via `chrono` if already
a dep; otherwise keep the offset arithmetic in the sqlite layer and pass seconds —
prefer whichever avoids adding a dep).

### 4. Migration — `migrations/0003_jobs.sql` + register in `crates/sql/src/lib.rs`

`fetch_jobs` table:
```
CREATE TABLE fetch_jobs (
  id          TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,
  target_id   TEXT NOT NULL,
  params      TEXT NOT NULL DEFAULT '{}',
  state       TEXT NOT NULL DEFAULT 'pending',
  attempts    INTEGER NOT NULL DEFAULT 0,
  run_after   TEXT NOT NULL,
  last_error  TEXT,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_fetch_jobs_active
  ON fetch_jobs(kind, target_id) WHERE state IN ('pending','running');
CREATE INDEX ix_fetch_jobs_claimable ON fetch_jobs(state, run_after);
```
Register `("0003_jobs", MIGRATION_0003)` in `migrations()`. Add a `pub mod job`
in `crates/sql/src/lib.rs` with the query constants (enqueue get-or-create, claim,
complete, retry, fail), `?`-positional, matching the existing modules' style.

Claim-by-update (SQLite-safe):
```
UPDATE fetch_jobs SET state='running', updated_at=?
WHERE id IN (
  SELECT id FROM fetch_jobs
  WHERE state='pending' AND run_after <= ?
  ORDER BY created_at LIMIT ?
)
RETURNING id, kind, target_id, params, attempts;
```

### 5. Backend impl — `crates/backends/store-sqlite/src/lib.rs`

Implement `JobEnqueuer` and `JobQueue` for `SqliteStore` using the `braincrawl_sql::job` constants, following the existing impls in this file (same connection/locking and `DomainError` mapping). Enqueue mints a `JobId` via the store's id gen and uses `INSERT … ON CONFLICT(kind,target_id) WHERE active DO NOTHING` then `SELECT` to return the winner (get-or-create), mirroring `get_or_create_alias`.

### 6. Concrete handlers — `apps/server/src/handlers/` (new module)

- `FulltextHandler` — `kind() == Fulltext`. Port the upstream resolution +
  download flow from `apps/cli/src/fetch_content.rs` (OpenAlex/Unpaywall URL
  resolution → GET bytes), but write via the in-process store's `put_content`
  instead of the CLI `StoreClient`. Reqwest lives here, not in core.
- `RefsHandler` — `kind() == Refs`. Port the Crossref/OpenCitations DOI →
  reference-DOIs flow from `apps/cli/src/refs_backfill/*`, building edges and
  writing via `put_edges`.
- Keep each handler thin: resolve → fetch → write. Map upstream/network failures
  to `DomainError` so the worker's retry path engages.

### 7. Wire it up — `apps/server/src/lib.rs` + `apps/server-bin/src/main.rs`

- Add a `POST /jobs` handler in `apps/server/src/lib.rs` that reads a `JobSpec`
  and calls `store.enqueue(...)`, returning the `JobId` (gated by the existing
  auth middleware like other handlers).
- In `apps/server-bin/src/main.rs`, after building the store, construct the
  `handlers` vec (`FulltextHandler`, `RefsHandler`) and `tokio::spawn` a loop that
  calls `braincrawl_core::worker::tick(...)` and sleeps (e.g. 1s when it processed
  0 jobs, immediately re-polls when it processed a full batch). Use a sane
  `BackoffPolicy` default (e.g. max_attempts 5, base 30s, factor 2).

## Files to Modify

- `crates/core/src/types.rs` — job types.
- `crates/core/src/traits.rs` — `JobEnqueuer`, `JobQueue`, `FetchHandler`.
- `crates/core/src/worker.rs` — new: `tick`, `BackoffPolicy`, `backoff`.
- `crates/core/src/lib.rs` — `pub mod worker;` and re-exports.
- `migrations/0003_jobs.sql` — new table + indexes.
- `crates/sql/src/lib.rs` — register migration, add `pub mod job` queries.
- `crates/backends/store-sqlite/src/lib.rs` — impl `JobEnqueuer` + `JobQueue`.
- `apps/server/src/handlers/mod.rs` (+ `fulltext.rs`, `refs.rs`) — concrete handlers.
- `apps/server/src/lib.rs` — `POST /jobs` route.
- `apps/server-bin/src/main.rs` — construct handlers + spawn worker loop.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Add unit tests:
- `crates/core` worker test with a fake `JobQueue` + a fake `FetchHandler`: a
  failing handler retries until `max_attempts` then `fail`s; a succeeding handler
  `complete`s. (Use an in-memory test double, not SQLite.)
- `crates/backends/store-sqlite` test: enqueue is idempotent on `(kind,target_id)`
  while active; `claim` returns claimable rows and flips them to `running` so a
  second `claim` does not re-return them; `complete`/`retry`/`fail` move state.

## Out of Scope

- Converting CLI commands (`fetch-content`, `crossref refs`, `opencitations refs`)
  to enqueue-by-default — they keep working synchronously. A later phase flips them
  to `POST /jobs`.
- Abstract persistence (separate inline change; no extra fetch needed).
- A `RateLimiter` trait / per-host budget (leave the comment slot in `tick`).
- Cloudflare Queues / Durable Objects (`store-d1`, `apps/worker`) — this is the
  local stable intermediate form; do not touch those crates.
- Any job-status HTTP read API beyond what `ContentOutcome::Pending` expresses.

## Notes

- `ContentOutcome::Pending` (`crates/core/src/types.rs`) already models "job exists,
  not done" on the read side — handlers don't need to manufacture a new signal.
- The store-mem backend (`crates/backends/store-mem`) need not implement the queue
  traits in this slice; if the build requires it for a shared bound, add a minimal
  in-memory impl rather than widening scope elsewhere.
- Watch the `#[async_trait(?Send)]` / `run_blocking` bridge: the worker loop runs
  on the Tokio runtime in server-bin and drives `!Send` futures the same way the
  HTTP handlers already do.

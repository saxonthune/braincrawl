---
name: braincrawl
description: "Drive the braincrawl research graph — run the local server, build shared L1/L2 coverage with the CLI (OpenAlex search → citation snowball → neighborhood), and maintain a per-domain L3 projection as an annotation-over-reference artifact in your own repo. Use when doing accumulating, coverage-first literature research (e.g. the Mesopotamia case study) instead of one-shot deep research."
---

# braincrawl

braincrawl builds a **reusable academic knowledge graph** and separates *coverage*
(build the graph once — expensive, inventory-shaped) from *decisiveness* (query it —
cheap, repeatable, any lens you choose). See `GOALS.md` and `CASE-STUDY.md` for the why.

Three layers:

| Layer | What it is | Where it lives |
|---|---|---|
| **L1 — Corpus** | works keyed by canonical id + payloads (abstract now, fulltext on demand) | the **server's store** (shared) |
| **L2 — Metadata graph** | nodes + citation edges, lazily accreted from OpenAlex | the **server's store** (shared) |
| **L3 — Consumer projection** | *your* questions, selections, annotations, domain edges | **a file in your own repo** (per-domain) |

L1/L2 are general and shared across every project. L3 is yours. **L3 is
annotation-over-reference: it stores canonical ids + your notes, never copies of the
metadata.** Hydrate L3 from the store at read time so it never drifts from the graph.

> Status note: L1 and L2 are implemented and runnable today (local server + CLI). The
> server has **no `/collections` endpoints yet**, so L3 is not a server feature — you
> maintain it as a flat artifact in your repo (this skill's main job). When server-side
> collections land, the same artifact migrates up.

## 1. Use the one shared server (don't start your own)

There is **one** braincrawl server per machine. It owns the accumulating L1/L2 corpus
(a stable SQLite file + blob dir under `~/.local/share/braincrawl/`). Every consumer
repo points at it so coverage compounds — a second project launching its own binary would
fork the corpus into a per-repo DB and defeat the whole accumulation thesis.

**First thing, every session: check it's up.** It exposes an unauthenticated liveness
probe at `GET /health`:

```bash
curl -fsS http://127.0.0.1:8787/health && echo   # → {"service":"braincrawl","status":"ok"}
```

- **200 / `status: ok`** → use it (go to §2). Don't start anything.
- **connection refused / no response** → the server is down. **Do not silently launch a
  binary from another repo.** Tell the user to start it from the braincrawl repo:

  ```bash
  # run from the braincrawl repo (the server's home)
  just server-start     # build-if-needed + launch in background, auth disabled (localhost)
  just server-status    # up/down + /health
  just server-stop
  ```

  The `just` recipes wrap `scripts/braincrawl-server.sh` (a pidfile-managed
  start/stop/status/restart/logs). Lifecycle is **manual** — start it when you sit down to
  research, stop it when done. The script defaults to a stable DB path and binds
  `127.0.0.1:8787` with auth disabled (localhost dev). Override via `BRAINCRAWL_BIND`,
  `BRAINCRAWL_DB`, `BRAINCRAWL_BLOB_ROOT`, or set `BRAINCRAWL_AUTH_TOKEN` to require a bearer.

Underlying server env vars (if you bypass the script): `BRAINCRAWL_DB` (default
`braincrawl.db`), `BRAINCRAWL_BLOB_ROOT` (default `blobs`), `BRAINCRAWL_BIND` (default
`0.0.0.0:8787`), and auth — `BRAINCRAWL_AUTH_TOKEN=<secret>` **or** `BRAINCRAWL_AUTH_DISABLED=1`.

## 2. Point the CLI at the server

The `braincrawl` CLI talks to the server (`BRAINCRAWL_SERVER_URL`, default
`http://127.0.0.1:8787`) and forks directly to OpenAlex for provider queries. Config
precedence is env > `~/.config/braincrawl/config.toml` > default.

```toml
# ~/.config/braincrawl/config.toml
server_url = "http://127.0.0.1:8787"
# auth_token = "..."                    # match BRAINCRAWL_AUTH_TOKEN if auth is enabled
# openalex_api_key = "..."              # optional; OpenAlex needs no key
# semanticscholar_api_key = "..."       # optional but strongly recommended — see below
```

Global output flags (work on every command): `--text` (one result/line, tab-separated),
`--json` (default, pretty), `--limit N`, `--all`, `--fields a,b,c`, `--full`,
`--abstract`, `--skip-push`.

**Push-to-store is on by default.** Every `openalex` query writes the works (and, for
`cited-by`/`refs`, the citation edges) into L1/L2. That is how coverage accumulates — one
project's reading is the next project's cache. Use `--skip-push` only for throwaway peeks.

### Set up Semantic Scholar

Semantic Scholar (`braincrawl semanticscholar`) works without a key, but the **keyless
shared pool is rate-limited to ~1 req/s and hits hard 429s during `--all` snowballs**.
A free API key raises your quota significantly. Precedence is the same as all other
config: env > `~/.config/braincrawl/config.toml` > none. The env var is
`BRAINCRAWL_SEMANTICSCHOLAR_API_KEY`.

**If the user wants to use `semanticscholar` and no key is configured, walk them through
getting one** rather than silently running keyless. Point at `SETUP.md` (same directory as
this skill) for step-by-step key acquisition, configuration, and verification. Do not just
start snowballing — a keyless `--all` will cascade 429s.

## 3. Build coverage (the funnel = graph traversal)

This is the inventory phase. Do not verify or kill anything here — just accrete.

```bash
# Seed — find landmark works (you don't know the seeds entering a new field)
braincrawl --text --limit 5 openalex search works "salt silt ancient mesopotamian agriculture"
# → W2031938753 "Salt and Silt in Ancient Mesopotamian Agriculture" (the landmark)

# Snowball FORWARD (cited-by) — old/humanities works have almost no backward refs,
# but forward citations are rich. This pushes nodes AND edges into L2.
braincrawl --text --all openalex cited-by W2031938753

# Backward refs when they exist (modern works)
braincrawl --text openalex refs W2031938753

# Read the graph back from the store (no provider call) — ranked by in-degree
braincrawl --text graph neighborhood openalex:W2031938753 --dir backward --depth 1 --max-nodes 50
```

**Reach for Semantic Scholar when OpenAlex coverage is patchy** — older or humanities
works often have missing abstracts and empty `refs` in OpenAlex, but S2 has them.
S2 pushes into the **same** L1/L2 store and merges on DOI via the server, so coverage
compounds across both providers automatically.

```bash
# S2 lookup by DOI or paperId
braincrawl --text semanticscholar get DOI:10.1126/science.aaf2654 --abstract

# Forward snowball via S2 (writes citation edges into L2)
braincrawl --text --all semanticscholar cited-by <paperId>

# Backward refs via S2
braincrawl --text semanticscholar refs <paperId>

# S2 paper search
braincrawl --text semanticscholar search papers "salt silt mesopotamian agriculture"
```

S2 IDs: bare 40-hex `paperId`, `DOI:<doi>`, `ARXIV:<id>`, `CorpusId:<n>`. Bare numerics are
ambiguous — prefix them. Authors: bare `authorId` or `ORCID:<orcid>`.

**Watch for citation drift** (GOALS §"Graph-building strategy"). The most-cited
descendants of an ancient-history paper are often modern plant-biology/agronomy works —
forward-mining by raw influence marches out of the field. Control it at the edge query:
filter forward expansion by concept/topic, e.g.

```bash
# stay in-domain: works citing the landmark, filtered to an OpenAlex concept/topic
braincrawl --text openalex find works "cites:W2031938753" "concepts.id:C<your-field-concept>"
```

Find concept ids with `openalex autocomplete topics "<term>"` or
`openalex search topics "<term>"`. Rank canon by in-degree *within the topic-filtered
subgraph*, not by global citation count.

Other useful verbs: `openalex get <id>` (single entity; id can be `W…/A…/S…` or
`doi:…/orcid:…/issn:…`), `openalex find <entity> <key:value>…`, `openalex search <entity>
<query>`, `openalex autocomplete <entity> <prefix>`. Entities: works, authors, sources,
institutions, topics, keywords, publishers, funders.

## 4. Maintain your L3 projection (the part you own)

Your domain project (e.g. the Mesopotamia game) keeps **one L3 artifact in its own repo** —
not in this repo. It records *your* selections, questions, findings, and domain edges,
each pointing at a **canonical / OpenAlex id** in the shared store. It never copies titles,
abstracts, or metadata except as a human-readable convenience comment — the id is the
source of truth, and you re-hydrate from the store when you need the facts.

Create `research/<domain>.l3.md` in your project repo. Recommended shape:

```markdown
---
domain: mesopotamia-loop-vs-accumulator
braincrawl_server: http://127.0.0.1:8787
updated: 2026-06-17
---

# L3 — Mesopotamia: loop vs accumulator

## Questions (the aim-directed frontier)
- Q1 soil salinization as a one-way accumulator until land abandonment
- Q2 debt accumulation until jubilee/clean-slate reset
- Q3 land concentration until the reset mechanism itself fails

## Selection set  (canonical id → tags · note)
- openalex:W2031938753  #landmark #Q1  Jacobsen & Adams 1958 — salt & silt; the seed
- openalex:W…           #review  #Q2   <one-line why it's in>
- openalex:W…           #primary #Q3

## Domain edges  (src --type--> dst)
- openalex:W2031938753 --supports--> Q1
- openalex:Wxxxx --refutes--> openalex:Wyyyy   # rebuttal found via forward citations
- openalex:Wxxxx --builds-on--> openalex:W2031938753

## Findings / annotations
- [Q1] salinization is loop-breaking, not cyclic — see W2031938753 §… ; confirm against …
- [open] need a review-tier synthesis for Q2; current coverage is primary-only
```

Maintenance loop, each research session:

1. **Ask L3 first.** Read your selection set. Can the open question be answered from ids
   you already hold? `braincrawl graph neighborhood openalex:<id> …` and `openalex get
   <id> --abstract` hydrate them from the store — no re-fetch.
2. **If not, expand coverage** (§3): seed/snowball into L1/L2. New works land in the
   shared store automatically (push-on-by-default).
3. **Project the keepers into L3.** Add the canonical ids that matter to your selection
   set with tags + a one-line note; add domain edges (`supports`/`refutes`/`builds-on`)
   and link them to your questions. Keep notes terse — they annotate, they don't restate.
4. **Decide at query time, not now.** Verification is a *lens you choose later*
   (e.g. "judge this claim by where it sits in the citation network"), never an up-front
   kill-gate. Coverage stays inclusive; decisiveness happens when you query.
5. **Commit `research/<domain>.l3.md`** to your project repo. It is small (ids + notes),
   diffs cleanly, and is the durable record of your domain thinking. The heavy shared
   corpus stays in the server's DB, reused across every project.

### Rules that keep L3 honest
- **Reference, never copy.** Store ids, not metadata. Re-hydrate from the server.
- **Canonical ids are the join key.** Prefer `openalex:W…`; the store resolves DOIs/ISBNs
  to the same canonical node, so any id form is safe but be consistent.
- **One artifact per domain**, in that domain's repo. A second project gets its own L3 and
  inherits whatever shared works already overlap in L1/L2 for free.
- **Edges and questions are yours.** L2 edges are neutral citations; L3 edges
  (`supports`/`refutes`/domain relations) carry your interpretation.

## Quick reference

| Task | Command |
|---|---|
| **Check server is up** | `curl -fsS http://127.0.0.1:8787/health` |
| Start server (braincrawl repo) | `just server-start` |
| Server up/down + health | `just server-status` |
| Stop server | `just server-stop` |
| Find landmark | `braincrawl --text openalex search works "<query>"` |
| Forward snowball | `braincrawl --text --all openalex cited-by <Wid>` |
| Backward refs | `braincrawl --text openalex refs <Wid>` |
| In-domain expand | `braincrawl --text openalex find works "cites:<Wid>" "concepts.id:<Cid>"` |
| Read graph back | `braincrawl --text graph neighborhood openalex:<Wid> --dir backward --depth 1` |
| Hydrate one work | `braincrawl openalex get <Wid> --abstract` |
| Peek store | `braincrawl store get openalex:<Wid>` |
| **S2 lookup** | `braincrawl --text semanticscholar get DOI:<doi> --abstract` |
| **S2 forward snowball** | `braincrawl --text --all semanticscholar cited-by <paperId>` |
| **S2 backward refs** | `braincrawl --text semanticscholar refs <paperId>` |
| **S2 paper search** | `braincrawl --text semanticscholar search papers "<query>"` |

For OpenAlex filter/field details, see the `openalex-reference` skill.

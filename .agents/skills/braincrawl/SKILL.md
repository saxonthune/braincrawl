---
name: braincrawl
description: "Route braincrawl research sessions by the user's question — reuse the Research Collection, map a field, test a hypothesis, read a source, import a PDF, or verify web evidence — while using the shared Library, Catalog, and Research Collection as needed."
---

# braincrawl

braincrawl builds a **reusable academic knowledge graph** and separates *gathering works*
(build the graph once — expensive, inventory-shaped) from *reading and judging* (query it —
cheap, repeatable, any lens you choose). For the why, see `.rhidoc/01-product/02-mental-model.md`
(the agent's role) and `.rhidoc/01-product/01-glossary.md` (the three layers, named).

## Routing — choose the workflow for the question

Read this file for the shared machinery (the three layers, the CLI, and the Research Document
grammar). Choose the workflow from the user's latest request and the relevant conversation
context. Read a companion file only when that workflow calls for it. A session may hand off from
one workflow to another when the question changes or a new evidence gap appears.

| Workflow | Read | Start here when |
|---|---|---|
| **Existing collection** | this file | Continuing a research line, asking what the accumulated Research Collection already says, or updating a known Research Document. |
| **Field mapping** | this file | Mapping a territory, finding the main literature, or inspecting citation coverage and clusters. |
| **Hypothesis or rough model** | this file and, when web evidence is needed, `web-research.md` | Testing whether a proposal appears in prior research, overlaps with an existing concept, or deserves a research network. |
| **Source-centered reading** | `reading-guide.md` | Reading one work directly and answering questions about its passages. |
| **PDF import** | `pdf-import.md` | Bringing local PDF files into the Library and making them readable and citable. |
| **Web verification** | `web-research.md` | Checking a supplied URL, a current claim, or a source the Catalog does not represent well. |

**Your role in a session.** You are a signal converter, and you work best as the medium
between sources of information. In a braincrawl session your job is a few general behaviors.
First, you build up the Library and Catalog — something you do faster than a human would in a
reference manager. Second, you act as the translator between the user and the library itself:
only the best works are read directly by a human, and as the work builds toward those moments
when a work must be encountered directly, the user instead asks a question as a prompt. You
carry that question to the artifacts in the library and return a response drawn from the
library.

Three layers:

| Layer | What it is | Where it lives |
|---|---|---|
| **L1 — Library** | works keyed by UUID + payloads (abstract now, fulltext on demand) | the **active store** |
| **L2 — Catalog** | nodes + citation edges, gathered lazily from OpenAlex | the **active store** |
| **L3 — Research Collection** | *your* questions, selections, annotations, domain edges | the **active store** (one Research Document per item; file repo behind the machine-local store, R2 behind the worker) |

Every store carries all three layers, and `store sync <src> <dst>` moves any of them
between stores — there is no privileged "local" or "remote" side, only a source and a
destination.

The Library and Catalog are general and shared across every project. The Research Collection is yours. **The Research Collection is
annotation-over-reference: it stores work *references* + your notes, never copies of the
metadata.** A work's canonical identity is its store-minted **UUID** (provider-neutral); on the
page you name a work by an `openalex:`/`doi:` reference that resolves to that UUID. Look facts
back up from the store at read time so a doc never drifts from the graph.

> Status note: the `braincrawl collection` commands edit the Research Documents behind the
> machine-local store directly — one markdown file per doc under the config-driven `l3_repo`
> root, which the local server also serves over `/api/l3/docs`. Editing files is store-local
> work; carrying docs to another store is `store sync`.

## 0. Choose the first evidence source

Do not use one fixed read order for every question. Before taking research actions, state the
route in one line and name the first evidence source, for example:

> Workflow: hypothesis check. I will search prior art and source pages first, then compare the
> result with the existing collection.

Use these first moves:

- **Existing collection** — refresh and read the relevant Research Documents, then use the
  Catalog or Library only for an open question.
- **Field mapping** — search the Catalog/OpenAlex and follow citation edges first; use L3 to
  record the campaign context, selections, and gaps.
- **Hypothesis or rough model** — search prior art and read abstracts or source pages first;
  consult L3 after the proposal has been compared with the literature.
- **Source-centered reading** — read the work's Library artifact or supplied PDF first; follow
  `reading-guide.md`.
- **PDF import** — inspect the local PDF first; follow `pdf-import.md` to resolve identity and
  derive artifacts.
- **Web verification** — use web search or WebFetch first; follow `web-research.md`.

The existing-collection route remains the right route for "what do we already know?", "continue
this research document", and similar requests. It is a route choice, not a prerequisite for
every provider or Library action.

At the end of a research turn, report what the selected evidence established, what remains open,
and whether you wrote to the Research Collection.

When a route consults the Research Collection, use its local index and document paths:

```bash
braincrawl collection index                # refresh before reading INDEX.md or cross-doc links
braincrawl collection list --text          # scan slugs + titles for on-topic docs
braincrawl collection path <doc>           # then Read the matching file(s) directly
```

Run `collection index` before reading `INDEX.md` when the route uses cross-document retrieval.
The store is written directly (the agent edits `.l3.md` files, so the manifest goes stale
between sessions); regenerating then makes the Cross-references and Work index reflect what is
actually on disk. A source-centered, PDF-import, or web-first route can skip this step until it
hands off to the Research Collection.

For cross-doc retrieval, read `INDEX.md` at the store root (regenerated by `collection index`).
Beyond the Documents table it carries two harvested indexes: **Cross-references** (the
doc→doc link graph, derived from the research nodes' link lines, *with computed back-links*
— "what else points at this doc", and `?`-marked targets are docs worth writing but not yet
written) and a **Work index** (each work reference — `openalex:`/`doi:` — → the docs that
reference it). To find every doc that already discusses a given work, look it up there rather
than grepping. A row naming two docs is a real cross-doc overlap. The reference is an alias
handle for the work's canonical UUID, not the UUID itself, so a work named by two different
aliases can still split across rows — keep one reference form per work.

A single L3 doc can carry the whole answer — its Questions frontier, Findings, and domain edges
are the distilled result of an earlier gathering pass. On the existing-collection route, state
the gap explicitly before pulling: name the docs you read and the part of the question they left
open. On the other routes, the first evidence may be a Catalog query, a Library artifact, a PDF,
or a web source. Do not repeat a read when the selected evidence already answers the question.

## 1. Use the one shared server (don't start your own)

There is **one** braincrawl server per machine. It owns the accumulating Library and Catalog
(a stable SQLite file + blob dir under `~/.local/share/braincrawl/`). Every consumer
repo points at it so gathered catalog entries compound — a second project launching its own binary would
fork the shared store into a per-repo DB and defeat the whole accumulation thesis.

When a route needs the CLI, run `braincrawl doctor` as an operational preflight. It reports
whether the server is up, which build it is running, and whether that build matches the CLI;
it does not choose the research workflow or evidence source:

```bash
braincrawl doctor
```

- **`server-reachable` OK and `build-match` OK** → use it (go to §2). Don't start anything.
- **`build-match` WARN, or `server-build` absent** → the server is running older code than
  the CLI. **Its answers cannot be trusted** — in particular a 404 from `library list` or
  `catalog get` may mean "this build has an old route", not "the store does not hold this".
  Do not conclude a work is missing, and do not re-ingest anything, until the builds match.
  The remedy `doctor` prints is `just upgrade`, which rebuilds and bounces the service;
  `just systemd-restart` does **not** rebuild.
- **`server-reachable` FAIL** → the server is down. **Do not silently launch a binary from
  another repo.** Tell the user to bring it up from the braincrawl repo, where `just -l`
  lists the current recipes (`systemd-install` first time, then `systemd-start`).

  The server runs as a systemd **user** service (`braincrawl-server.service`), enabled with
  lingering so it starts at boot and stays up — lifecycle is **not** manual day-to-day. The
  `just` recipes drive it via `systemctl --user`; `scripts/braincrawl-server.sh` is a thin
  shim over the same. It binds `127.0.0.1:8787` with auth disabled (localhost dev) and uses a
  stable DB path under `~/.local/share/braincrawl`. Config lives in the unit
  (`scripts/braincrawl-server.service` is the tracked template).

Underlying server env vars (if you bypass the script): `BRAINCRAWL_DB` (default
`braincrawl.db`), `BRAINCRAWL_BLOB_ROOT` (default `blobs`), `BRAINCRAWL_BIND` (default
`0.0.0.0:8787`), and auth — `BRAINCRAWL_AUTH_TOKEN=<secret>` **or** `BRAINCRAWL_AUTH_DISABLED=1`.

## 2. Point the CLI at the server

The `braincrawl` CLI talks to one store at a time and forks directly to OpenAlex for
provider queries. Stores are named in `~/.config/braincrawl/config.toml` under
`[stores.<name>]`; `active_store` picks the one every command targets. Config
precedence is `BRAINCRAWL_SERVER_URL` env > active named store > flat `server_url` >
default `http://127.0.0.1:8787`.

```toml
# ~/.config/braincrawl/config.toml
active_store = "local"

[stores.local]
url = "http://127.0.0.1:8787"

[stores.worker]
url = "https://example-worker-host"
# auth_token = "..."                    # the token belongs to the store it unlocks
# openalex_api_key = "..."              # top-level; optional — OpenAlex needs no key
```

Switch stores with `braincrawl store use <name>` (prints both sides' counts so you see
what you are walking away from). `braincrawl store list` shows the roster;
`braincrawl store diff <a> [b]` compares two stores; `braincrawl store sync <source>
<dest>` copies everything the destination lacks (idempotent replay — safe to re-run).
Sync carries all three layers: catalog works/edges/artifacts by replay, and Research
Documents node-by-node (anchors are the unit; rules in `crates/l3-sync`). Node conflicts
are reported, never clobbered; run the sync in both directions to converge both stores.

Global output flags (work on every command): `--text` (one result/line, tab-separated),
`--json` (default, pretty), `--limit N`, `--all`, `--fields a,b,c`, `--full`,
`--abstract`, `--skip-push`, `--emission`.

**Push-to-store is on by default.** Every `openalex` query writes the works (and, for
`cited-by`/`refs`, the citation edges) to the Library and Catalog. That is how the catalog grows — one
project's reading is the next project's cache. Use `--skip-push` only for throwaway peeks.

**Lookup and store-write decompose.** `--emission` prints the store-ready frame instead of
the display envelope; `braincrawl catalog put` reads that frame from stdin or a file and
writes it. So:

```bash
braincrawl openalex search works "…" --skip-push --emission | braincrawl catalog put
```

is equivalent to plain `braincrawl openalex search works "…"`. Use the decomposed form to
hold a lookup result (in a file, a variable, another tool) before deciding to land it.

## 3. Get more catalog entries (the funnel = graph traversal)

This is the inventory phase for the **field-mapping** workflow. Do not verify or kill anything
here — just gather. An existing-collection route may arrive here after stating its gap; a
hypothesis or web-first route may arrive here after an initial source search. If the selected
route's evidence already answers the question, stop gathering.

```bash
# First search — find heavily cited works (you don't know the starting work entering a new field)
braincrawl --text --limit 5 openalex search works "salt silt ancient mesopotamian agriculture"
# → W2031938753 "Salt and Silt in Ancient Mesopotamian Agriculture" (a heavily cited work)

# Follow citations FORWARD (cited-by) — old/humanities works have almost no backward refs,
# but forward citations are rich. This pushes nodes AND edges to the Catalog.
braincrawl --text --all openalex cited-by W2031938753

# Backward refs when they exist (modern works)
braincrawl --text openalex refs W2031938753

# Read the graph back from the store (no provider call) — ranked by in-degree
braincrawl --text catalog neighborhood openalex:W2031938753 --dir backward --depth 1 --max-nodes 50
```

**Watch for citation scatter** (GOALS §"Graph-building strategy"). A heavily-cited source is
referenced across many unrelated domains, so its most-cited forward citations are often off
in another field — the descendants of an ancient-history paper are mostly modern
plant-biology/agronomy works, and following them by raw influence marches out of the field.
This is *scatter*, not **drift** (a copy diverging from its source) — see the glossary,
`doc01.01`. The control is **topic-gating**: filter forward expansion by concept/topic, e.g.

```bash
# stay in-domain: works citing that work, filtered to an OpenAlex concept/topic
braincrawl --text openalex find works "cites:W2031938753" "concepts.id:C<your-field-concept>"
```

Find concept ids with `openalex autocomplete topics "<term>"` or
`openalex search topics "<term>"`. Rank canon by in-degree *within the topic-filtered
subgraph*, not by global citation count.

Other useful verbs: `openalex get <id>` (single entity; id can be `W…/A…/S…` or
`doi:…/orcid:…/issn:…`), `openalex find <entity> <key:value>…`, `openalex search <entity>
<query>`, `openalex autocomplete <entity> <prefix>`. Entities: works, authors, sources,
institutions, topics, keywords, publishers, funders.

## 4. Read by progressive disclosure

The reads below differ in how much they cost and how much they tell you.
**Answer from the evidence source selected for the workflow, and before each handoff say what
you consulted and what it left open.** Don't fetch fulltext for a paper whose title already
disqualifies it; don't fetch 40 abstracts when the in-degree ranking already names the three
that matter. Go straight to the read the question needs: these are options, and any one of them
can be the first and only one a question requires.

- **Your L3 Research Collection** — the docs you already wrote (`collection list` → Read the
  match). Cheapest by far and usually enough for the existing-collection workflow: a doc's
  Findings + domain edges are a prior gathering phase already distilled.
- **Catalog** — titles, authors, in-degree, citation edges. `catalog neighborhood`,
  `openalex search/find`, `cited-by`/`refs`. Often a title + who-cites-whom is enough to
  place a work or rule it out. Store-local (no provider call) once gathered.
- **Abstracts** — `--abstract` (pair with `--fields title,publication_year,abstract` to
  skip the JSON dump). The workhorse read: usually answers "what does this argue, and does
  it bear on my question?" **Coverage is uneven** — old/closed works may have *no* abstract
  (e.g. Jacobsen & Adams 1958), and some publishers (De Gruyter) return a boilerplate
  placeholder, not real content. When OpenAlex is blank, bridge it by fetching the source
  yourself (WebFetch / the open-access PDF) and reading the abstract in-context.
- **AI-condensed summary** — get a work's fulltext into the store and distill it to the
  claim you need. Acquire from the open web with `library fetch`, or ingest a local file
  with `library put` (e.g. a user-provided PDF), then `library extract-text` to store the
  extracted text at role `text` and `library get --role text` to read it back and condense
  it in-context. When the source isn't reachable that way, bridge with WebFetch. Run
  `library list <id>` first if you are not sure which roles a work already holds — it
  returns role, version, size, mime, and provenance for every stored artifact, and now shows
  which artifacts derive from which, so a session can see a work holds two editions before it
  picks one, and is not guessing a role name.
- **Addressing a place in the book directly** — once `library paginate` and `library outline`
  have derived a per-page artifact and a table-of-contents section map, `library read` reaches
  a printed page, an outline section, or a text search directly, with page markers on the
  output. Cheaper than condensing the whole work, and citable without a second lookup.
- **Full text** — read the whole work. The dearest read; reserve for the load-bearing few
  a finding actually hangs on. The Library holds fulltext "on demand" — same `library fetch`/
  `library put` + `library extract-text` + `library get --role text` path as the condensed
  summary; just read more of the extracted text.
- **Web sources** — use search or WebFetch first for a web-verification route, for a supplied
  URL, or when provider metadata is absent or stale. Treat the page as source evidence; use
  `library fetch` when the goal is to acquire an open artifact, and use `catalog` when the goal
  is to land a durable work identity.

Worked loop: a temple-formation question found *nothing* in the held docs or the catalog →
ran `openalex search "origins of the temple economy…"` → one `--abstract` read of that work
carried the full Gelb/Diakonoff vs Deimel answer. The question was answered there, so no
fulltext was acquired and nothing was condensed. Saying what the catalog had missed is what
made the search worth running; saying what the abstract had settled is what made the rest
unnecessary.

**Verify the CLI surface before assuming a capability is missing.** This doc names verbs by
way of example, not as an exhaustive or current inventory — the binary evolves. Run
`braincrawl --help` to see today's top-level commands, and `braincrawl <command> --help`
for a subcommand's flags, rather than trusting a hard-coded "X is not implemented" claim
here. If a step genuinely has no verb after you've checked, *say so explicitly* and name the
capability to add rather than silently stopping short.

## 5. Maintain your Research Collection (the consolidated store you own)

Research Documents are **not** scattered in per-project repos anymore. They live in **one
consolidated store** — a single directory, one `<doc>.l3.md` markdown file per item —
and you create, locate, and list them through the `braincrawl collection` CLI. Each document is a
collection of research nodes (`##` headings) recording *your* selections, questions,
findings, and domain edges, each pointing at a work in the shared store by an `openalex:`/`doi:`
reference that resolves to its canonical UUID.
It never copies titles, abstracts, or metadata except as a human-readable convenience — the
reference (and the canonical UUID it resolves to) is the source of truth; look it back up from
the store when you need the facts.

### The store location (config-driven)

The store root resolves from `BRAINCRAWL_L3_REPO` (env) > `l3_repo` in
`~/.config/braincrawl/config.toml` > default `~/.local/share/braincrawl/l3`. Set it once:

```toml
# ~/.config/braincrawl/config.toml
l3_repo = "~/code/github/saxonthune/braincrawl-l3"   # a dedicated git repo is recommended
```

### The contract lives in `doc02.01.04`

The full contract — required frontmatter, the node grammar, and the reference-never-copy
invariant — is spec'd in `.rhidoc/02-architecture/01-core/04-l3-conventions.md`
(`doc02.01.04`), with a worked example at `.agents/skills/braincrawl/l3-example.l3.md`. Read
one before writing or editing an L3 doc. The cheat-sheet below is enough for routine authoring.

**Frontmatter** — two required fields: `doc:` (kebab-case slug, filename stem, primary key)
and `updated:` (date, stamped by the CLI).

**Node grammar** — everything below the frontmatter is a sequence of nodes, no loose prose,
no H1:

- A node opens with `## title` — tooling appends the `^r-…` anchor; never write one yourself.
- A property line is `- key: value`; a value may hold one flow map `{k: v}`.
  `- tags: #a #b` turns each `#`-token into a label.
- A link line is `- kind [[target]]` (implicit source) or `- [[src]] kind [[dst]]`, with
  optional trailing `{props}`. A target is a node anchor `^r-…`, a bare `slug` (that doc's
  intro node), or any `scheme:value` catalog reference (`openalex:`/`doi:`/`isbn:`/
  `pmid:`/…, or `uuid:` for a work — e.g. a book — that has no external id, referenced by
  its canonical id directly). `catalog` names a work-link; `contradicts` is the blessed word
  for a claim-link (never `refutes`); `supports`/`builds-on`/`relates-to`/`bridges`/
  `complicates` are free domain vocabulary.
- A catalog reference only resolves once some node in the store carries that alias. For a
  work no provider has a record for — a book, most often — mint it by hand with
  `braincrawl catalog add --alias isbn:… --title … [--author … --year …]` before linking to it.
- Anchors are **store-global**: `assign-ids` keeps every `^r-…` unique across the whole store,
  so a bare `^r-…` resolves to its node from any doc — no `slug#` prefix, and the reference
  survives if the node moves docs.
- To cross-link two nodes written in the same batch, before `assign-ids` has run and a real
  anchor exists — write a **temporary anchor** by hand instead of guessing a `^r-…`: give the
  heading a `^t-<slug>` and reference it as `[[^t-<slug>]]` from any edge in any doc of the
  batch. One `assign-ids` run resolves every `^t-<slug>` to a fresh store-global `^r-…`,
  rewriting the heading and every reference to it. Worked example:

  ```
  ## The three textures ^t-three-textures
  - tags: #meta

  - builds-on [[^t-three-textures]]
  ```

  A `^r-…` is still never hand-written.
- A `[[wikilink]]` inside a property *value* is text, not an edge — only a `- kind [[…]]`
  bullet line becomes a link.
- Invariant: reference, never copy — store ids, look facts up from the server at read time.
- `- reading: {role: start-here, why: '…'}` plus a `catalog` link marks a recommended
  reading; `role` is `start-here`/`core`/`rigor`/`reference` or any other string.

### CLI surface

```bash
braincrawl collection new <doc> [--title "…"]  # create; prints absolute path
braincrawl collection path <doc>          # print absolute path of an existing doc
braincrawl collection list                # all docs (doc · updated · path)
braincrawl collection check <doc> | --all # advisory lint against the required frontmatter
braincrawl collection index               # regenerate INDEX.md
braincrawl collection import <file> [--doc <slug>] [--mv]   # adopt an existing md, normalize its frontmatter
braincrawl collection rm <doc>            # delete + reindex
braincrawl collection assign-ids [--dry-run]   # assign ^r-… anchors for every heading that lacks one, and resolve every ^t-<slug> temporary anchor to a fresh ^r-…
braincrawl collection reading-list [--json]    # every node with a reading property, grouped by role
```

`new` and `path` print **only the absolute path** to stdout, so you can capture it and
write the file yourself: get the path, then use your editor/Write tool on it. braincrawl
owns *where* the doc lives and the index; you own the bytes.

Maintenance loop, when a route produces a Research Collection result:

1. **Choose the route first.** Follow the route's first evidence source and record the open gap.
2. **Find the doc when you need to record.** `braincrawl collection path <doc>` (or
   `collection new <doc>` the first time) → get its absolute path, then read/edit that file.
3. **Use the store to continue the selected route.** For example, read a held work from the
   Catalog/Library, gather citations, or hand off from a web source to a stored artifact.
4. **Reference the selected works in your Research Document.** Add the works that matter — by their
   `openalex:`/`doi:` reference — to your selection set with tags + a one-line note; add domain edges (`supports`/`contradicts`/`builds-on`)
   linked to your questions. Keep notes terse — they annotate, they don't restate.
5. **Read and judge at query time, not now.** Verification is a *lens you choose later*, never an
   up-front kill-gate. Gathering stays inclusive; reading and judging happens when you query.
6. **Reindex** (`braincrawl collection index`) after edits when you want `INDEX.md` refreshed. The store is plain
   markdown under git — commit it like any repo.

### Rules that keep the Research Collection honest

The invariants — reference-never-copy, one canonical UUID per work (named by an `openalex:`/`doi:`
reference that resolves to it), one doc per item, and your edges
are yours to keep — are the settled contract in `doc02.01.04`
(`.rhidoc/02-architecture/01-core/04-l3-conventions.md`); read them there rather than from a
copy here.

## Quick reference

The CLI is self-documenting, so there is **no hand-maintained command table here** — it would
only drift from the binary. Generate the current reference on demand from the source of truth:

- `braincrawl --help` — current top-level commands (`openalex`, `catalog`, `collection`,
  `library`, …).
- `braincrawl <command> --help` — a subcommand's flags and arguments (e.g.
  `braincrawl openalex --help`, `braincrawl collection --help`, `braincrawl library --help`).
- `braincrawl doctor` — server reachability, build match, store identity, config sources (§1).
- Server lifecycle is **not** part of this CLI — it lives in the braincrawl repo's `just`
  recipes. Run `just -l` for the current list rather than working from a remembered one.

For OpenAlex filter/field details, see the `openalex-reference` skill.

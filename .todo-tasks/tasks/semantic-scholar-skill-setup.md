# Document the Semantic Scholar provider: config section, setup help, SETUP.md, rhidoc doc

## Motivation

Phase 1 (`semantic-scholar-provider`) added the `braincrawl semanticscholar` CLI namespace
and a `semanticscholar_api_key` config field, but left **all docs untouched on purpose**.
This phase makes S2 usable by a consumer: a config section + a "walk the user through setup"
flow in the consumer-facing **braincrawl** skill, a separate **SETUP.md** for troubleshooting,
and the rhidoc provider doc so the spec workspace reflects the second provider.

This is **Phase 2 of a 2-phase chain** and is **docs/skill only — no Rust changes.** It
triages against Phase 1's declared Surface (below), not live code, since Phase 1 has not
merged yet.

The braincrawl skill (`.claude/skills/braincrawl/SKILL.md`) is the **consumer-facing**
surface (currently consumed via symlink). The `openalex-reference` skill is a separate
dev-only API reference — **do not** put S2 setup help there; it belongs in the braincrawl
skill so consumers see it.

## Phase 1 Surface this phase depends on

> Phase 1 has not merged. Triage against this contract; treat anything not listed as
> nonexistent.

- CLI namespace `braincrawl semanticscholar` with verbs `get <id>`,
  `search <entity> <query>` (entity ∈ {papers, authors}), `cited-by <id>`, `refs <id>`.
  All honor existing global flags (`--text/--json/--limit/--all/--fields/--full/--abstract/--skip-push`).
- Config field `semanticscholar_api_key`, precedence env `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY`
  > `~/.config/braincrawl/config.toml` key `semanticscholar_api_key` > none. Sent to S2 as
  the `x-api-key` header.
- Push provenance `source = "semanticscholar"`; papers → `Work`, authors → `Author`.
- Alias namespaces: `s2`, `corpusid`, `doi`, `arxiv`, `mag`, `pmid`, `pmcid` (papers);
  `s2author`, `orcid` (authors). DOI overlap merges S2 ↔ OpenAlex onto one canonical node.

## Do NOT

- **Do NOT change any Rust** (`apps/cli/**`). This phase is `.md` / rhidoc only.
- **Do NOT put S2 setup help in the `openalex-reference` skill.** That skill is dev-only.
  Consumer setup lives in the braincrawl skill + its SETUP.md.
- **Do NOT hand-create or hand-renumber rhidoc docs.** Structural changes (new doc) go
  through the `rhidoc` CLI; prose edits use Write/Edit; then `rhidoc regenerate`. A direct
  file write into `.rhidoc/` corrupts numbering and the MANIFEST.
- **Do NOT claim verbs/flags S2 does not have.** Document exactly the Phase 1 Surface above
  (no `find`/`autocomplete` for S2).
- **Do NOT overstate the merge.** Describe it as "DOI overlap merges onto one canonical
  node via the server"; do not imply field-level conflict resolution.

## Plan

### 1. braincrawl SKILL.md — config section + setup-help flow

Edit `.claude/skills/braincrawl/SKILL.md`:

- **§2 ("Point the CLI at the server")** — extend the `config.toml` block and prose to cover
  S2. Add `# semanticscholar_api_key = "..."` to the example TOML with a one-line note that
  it is optional but **strongly recommended** (the keyless shared pool is ~1 req/s and 429s
  hard during a snowball). Note precedence is the same env > config.toml > default, and the
  env var is `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY`. Add a short **"Set up Semantic Scholar"**
  paragraph instructing the agent to, *if the user wants S2 and no key is configured*, walk
  them through it (point at SETUP.md, §3 below) rather than silently running keyless.
- **§3 ("Build coverage")** — add S2 as the second provider alongside OpenAlex: a couple of
  example commands (`braincrawl --text semanticscholar get DOI:<doi> --abstract`,
  `braincrawl --text --all semanticscholar cited-by <paperId>`,
  `braincrawl --text semanticscholar refs <paperId>`) and one sentence on *when* to reach
  for S2 — patchy OpenAlex coverage for older/humanities works (missing abstracts, empty
  backward `refs`). Stress that S2 pushes into the **same** L1/L2 store and merges on DOI,
  so coverage compounds across both providers.
- **Quick reference table** — add S2 rows mirroring the OpenAlex ones (get / cited-by / refs).
- Keep edits tight and in the existing voice. Do not restructure the doc.

### 2. New `.claude/skills/braincrawl/SETUP.md` — troubleshooting

Create `.claude/skills/braincrawl/SETUP.md` as a focused troubleshooting doc the SKILL.md
links to (kept separate to avoid context pollution in the main skill). Cover:

- **Getting a Semantic Scholar API key** — request form at
  `https://www.semanticscholar.org/product/api#api-key`; it arrives by email; free.
- **Configuring it** — the two ways, with precedence: export
  `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY=…` (session/shell) or add
  `semanticscholar_api_key = "…"` to `~/.config/braincrawl/config.toml` (also note the
  `BRAINCRAWL_CONFIG` override path). Show the exact TOML.
- **Verifying it works** — a command to confirm (`braincrawl --text semanticscholar get
  DOI:10.1126/science.aaf2654 --skip-push` returns a paper) and how to tell a 429
  (rate-limited → key missing/throttled) from a real failure.
- **Common issues** — keyless 429 storms during `--all` snowballs (get a key / slow down);
  id forms S2 accepts (`DOI:`, `ARXIV:`, `CorpusId:`, bare 40-hex paperId) and the loud
  "cannot infer" error on ambiguous bare numerics (prefix it); server-down vs provider-down
  (check `curl /health` first, per §1).
- A short OpenAlex note for parity (no key required; `BRAINCRAWL_OPENALEX_API_KEY` optional).

Keep it scannable — headers + short bullets, not prose walls.

### 3. rhidoc provider doc

Read `.rhidoc/MANIFEST.md`, then `.rhidoc/02-architecture/06-cli/01-providers/00-index.md`
and `01-openalex.md` for the pattern. Run `rhidoc ai-skill` first (per the rhidoc-cli
skill) and do not guess flags.

- Create a new provider doc under `01-providers/` for Semantic Scholar via the `rhidoc`
  CLI (a `create`/`new` in that directory — let the CLI assign the number; it becomes
  `doc02.06.01.02`). Then Write its body, mirroring `01-openalex.md`'s structure: base
  vocabulary table (`get`/`search`/`cited-by`/`refs` only), output shape (shared envelope),
  upstream client (x-api-key header, offset/next paging, 429 backoff), and persistence
  (node kinds Work/Author, alias namespaces, `source:"semanticscholar"`, DOI-merge). Set
  frontmatter `title`, `summary`, `tags: [cli, providers, semanticscholar, fork, verbs, citations]`,
  `deps: [doc02.06.01, doc02.01.03]`.
- Edit the providers **index** (`00-index.md`, doc02.06.01.00): change "One provider today:
  OpenAlex" → two providers, add a Contents bullet for the S2 doc. Update its `summary`
  frontmatter likewise.
- Update the CLI index (`doc02.06.00`) and the OpenAlex doc's "Refs" only as the MANIFEST's
  reverse-dep computation requires — i.e. just run `rhidoc regenerate` and let it recompute;
  do not hand-edit Refs columns.
- Run `rhidoc regenerate` (or `python3 .rhidoc/rhidoc.py regenerate` if `rhidoc` is not on
  PATH) and confirm the MANIFEST lists the new doc with no orphan warnings.

## Files to Modify

- `.claude/skills/braincrawl/SKILL.md` — §2 config + setup-help, §3 S2 commands, quick-ref rows
- `.claude/skills/braincrawl/SETUP.md` — new troubleshooting doc
- `.rhidoc/02-architecture/06-cli/01-providers/02-*.md` — new S2 provider doc (via rhidoc CLI)
- `.rhidoc/02-architecture/06-cli/01-providers/00-index.md` — "two providers" + Contents bullet
- `.rhidoc/MANIFEST.md` — regenerated (do not hand-edit; `rhidoc regenerate` rewrites it)

## Verification

```bash
# braincrawl skill documents S2 config + the env var, and SETUP.md exists
grep -q "semanticscholar_api_key" .claude/skills/braincrawl/SKILL.md
grep -q "BRAINCRAWL_SEMANTICSCHOLAR_API_KEY" .claude/skills/braincrawl/SETUP.md
test -f .claude/skills/braincrawl/SETUP.md
# rhidoc workspace regenerates cleanly and the new provider doc is registered
(rhidoc regenerate || python3 .rhidoc/rhidoc.py regenerate)
grep -qi "semanticscholar\|semantic scholar" .rhidoc/MANIFEST.md
# providers index no longer claims a single provider
grep -L "One provider today" .rhidoc/02-architecture/06-cli/01-providers/00-index.md
```

All grep assertions must succeed and `rhidoc regenerate` must exit 0 with no orphan warnings.

## Out of Scope

- Any Rust change (Phase 1 owns the implementation).
- Editing the `openalex-reference` dev skill.
- Documenting Crossref/OpenCitations (separate task) or provider auto-fallback policy.

## Notes

- This phase has no compile gate — its correctness is "does a consumer reading the braincrawl
  skill know how to configure and use S2, and does the rhidoc workspace reflect the second
  provider." Keep claims aligned to the Phase 1 Surface exactly.
- The setup-help flow is the user's explicit ask: the skill should *offer to walk the user
  through* getting+configuring a key when S2 is wanted and none is set, not silently run
  keyless and hit 429s.

## Surface after this phase

- braincrawl SKILL.md §2 documents `semanticscholar_api_key` (env + config.toml, precedence)
  and a setup-help flow pointing at SETUP.md; §3 + quick-ref show S2 `get/search/cited-by/refs`.
- `.claude/skills/braincrawl/SETUP.md` exists: key acquisition, configuration, verification,
  and common-issue troubleshooting for S2 (and an OpenAlex parity note).
- rhidoc doc `doc02.06.01.02` documents the Semantic Scholar provider fork; the providers
  index and MANIFEST reflect two providers.

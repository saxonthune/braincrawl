# L3 conventions

The single home for **what belongs in an L3 Research Document and what doesn't**. Two parts:
a small **settled contract** (the laws — the frozen envelope + the invariants every doc
obeys), and an **experimental ledger** (candidate body conventions, still churning, held as
leans not laws). The braincrawl `SKILL.md` points here instead of carrying any of it, so
there is one source of truth.

Spec cross-reference: `doc02.01.04` in the `.rhidoc/` workspace points at this file.

---

## The settled contract

The Research Collection stays consolidated *and* keeps evolving its document design because
the contract is split — a frozen envelope the tooling reads, a free body you experiment in.

**Envelope (frozen, tooling reads it without parsing the body).** Three frontmatter keys,
the *required* set (`braincrawl l3 check` warns on any missing):

- `doc:` — kebab-case slug, the **primary key** and filename stem.
- `schema:` — a label naming the body convention in use (free string).
- `updated:` — date, stamped by the CLI.

The set grows over time as conventions firm up — that is the one place the contract tightens.

**Body (free — the experimentation zone).** Everything below the frontmatter is yours. Many
document designs coexist because each doc *declares* its `schema:`; the linter only checks
what that schema requires. `schema: freeform` = no body lint at all (the escape hatch).
`l3 new --schema spine` / `--schema dialectical` stamp a starter skeleton (Questions /
Selection set / Domain edges / Findings); any other schema just stamps the envelope + an H1.

**Invariants that keep the Research Collection honest** (these hold across every schema):

- **Reference, never copy.** Store ids, not metadata. Look the facts back up from the server
  when you need them. This is the one hard body rule.
- **UUIDs are the join key.** Prefer `openalex:W…`; the store resolves DOIs/ISBNs to the same
  UUID, so any id form is safe — but be consistent.
- **One doc per item, in the consolidated store.** Don't hand-create `.l3.md` files in random
  repos — go through `l3 new`/`l3 import` so nothing scatters.
- **Edges and questions are yours.** Catalog edges are neutral citations; Research Collection
  edges (`supports`/`refutes`/domain relations) carry your interpretation.
- **`doc`/`schema`/`updated` are the envelope; the body is free.** Experiment with layout
  under a new `schema:` label; never break the three envelope keys.

---

## The experimental ledger

**Nothing below is binding.** This is the scratch space where candidate *body* conventions
are collected, churned, and discarded until one earns its way up into the settled contract
above (or into a `braincrawl l3 check` per-schema lint).

Pipeline: an `l3-feedback-*` report lands in the braincrawl todo-tasks inbox (filed by the
`braincrawl-l3-feedback` skill) → a braincrawl session reads it → adds or updates a candidate
below with a **lean**, not a law. A candidate graduates only when the same lean has survived a
few real sessions; on graduation, promote it into the settled contract / a lint and delete it
from here.

### How to use the ledger

- One candidate per `###` heading. Phrase it as the behavior in question.
- Each carries a **Status**, the source report slug, the behavior, the current lean, and why.
  Statuses: `experimental` (just collected), `leaning-allow`, `leaning-discourage`, `landed`
  (graduated — record where to, then delete), `discarded` (record why).
- Reverse freely. A lean can be wrong and cost nothing to drop. Don't formalize early.

### Catalog/artifact-state markers in an L3 selection entry

- **Status:** leaning-discourage
- **Source:** `l3-feedback-catalog-state-in-l3`
- **Behavior:** annotating selection entries with facts about L1/L2 — "✅ fulltext stored",
  "node pinned this session", "⚠️ NO open fulltext found", "(pinned + FULLTEXT stored
  <date>)".
- **Lean:** discourage. These are look-up-able store facts; writing them into L3 duplicates
  catalog state and goes stale silently when an artifact is evicted or re-fetched — the exact
  drift reference-never-copy exists to prevent. L3 entry stays `id + note + edges`.
- **The real need underneath:** "which of my selected works can I read deeply right now?" is
  legitimate — but its home is a **store query** (a `has-fulltext` filter / `store have`) at
  read time, not a written-in note. If pursued, that's a separate `feedback-*` CLI task, not
  an L3 convention.
- **Not yet landed because:** wants confirmation across more than one session before it goes
  into the settled contract, and the store-query alternative isn't built yet.

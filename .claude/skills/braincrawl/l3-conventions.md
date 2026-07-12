# L3 conventions

The single home for **what belongs in an L3 Research Document and what doesn't**. Two parts:
a small **settled contract** (the laws — the frozen envelope + the node grammar every doc's
body is written in), and an **experimental ledger** (candidate conventions on top of that
grammar, still churning, held as leans not laws). The braincrawl `SKILL.md` points here
instead of carrying any of it, so there is one source of truth.

Spec cross-reference: `doc02.01.04` in the `.rhidoc/` workspace points at this file.

---

## The settled contract

The Research Collection stays consolidated *and* keeps evolving its document design because
the contract is split — a frozen envelope the tooling reads, and a body written in one node
grammar that crate `l3` parses into the research graph.

**Envelope (frozen, tooling reads it without parsing the body).** Three frontmatter keys,
the *required* set (`braincrawl l3 check` warns on any missing):

- `doc:` — kebab-case slug, the **primary key** and filename stem.
- `schema:` — a label naming the body convention in use (free string).
- `updated:` — date, stamped by the CLI.

The set grows over time as conventions firm up — that is the one place the contract tightens.

**Body — the node grammar.** Everything below the frontmatter is a sequence of research
nodes. `l3 new` stamps every doc with the same node-grammar skeleton regardless of `schema:`
(the label still names your convention, but the grammar underneath is universal); `l3
list`/`check`/`index` all read the body through this grammar.

- **Node.** A `##` heading opens a node; its title is opaque prose (write anything). The
  tooling appends a trailing `^r-…` anchor to the heading — **never write one yourself**;
  `braincrawl l3 assign-ids` mints anchors for every heading that lacks one. A node without
  an anchor still parses (title, properties, links), it just has no stable id yet.
- **Property line** — `- key: value`, one per bullet. A value may hold one flow map,
  `{k: v, k2: 'v, with a comma'}`; quote any value containing a comma. `- tags: #a #b` is the
  one special key — each `#`-prefixed token becomes a label on the node, not a property.
- **Link line** — connects two endpoints, one of which may be implicit:
  - `- kind [[target]] {props}` — implicit source (the enclosing node).
  - `- [[src]] kind [[dst]] {props}` — explicit source and target.
  - A target is `^r-…` (a node in this doc), `slug#^r-…` (a node in another doc), a bare
    `slug` (a doc-level forward reference, no anchor needed), or `openalex:…`/`doi:…` (a
    catalog entry). `kind` is free lowercase-kebab vocabulary — see below.
  - `catalog` is the convention for a work-link (node → catalog id).
  - `contradicts`/`supports`/`builds-on` are free domain-edge vocabulary; **`contradicts` is
    the blessed word for a claim-link** — never write `refutes`.
  - `reading: {role: …, why: …}` marks a recommended reading (a property, not a link).
- **Anchors and `INDEX.md` are tooling-owned.** Agents never write a `^r-…` anchor or hand-edit
  `INDEX.md` — both are generated (`l3 assign-ids`, `l3 index`).

**Invariants that keep the Research Collection honest** (these hold across every schema):

- **Reference, never copy.** Store ids, not metadata. Look the facts back up from the server
  when you need them. This is the one hard body rule.
- **UUIDs are the join key.** Prefer `openalex:W…`; the store resolves DOIs/ISBNs to the same
  UUID, so any id form is safe — but be consistent.
- **One doc per item, in the consolidated store.** Don't hand-create `.l3.md` files in random
  repos — go through `l3 new`/`l3 import` so nothing scatters.
- **Edges and questions are yours.** Catalog links are neutral citations; Research Collection
  links (`supports`/`contradicts`/`builds-on`/domain relations) carry your interpretation.
- **`doc`/`schema`/`updated` are the envelope; the body is the node grammar.** Experiment
  with what you record inside a node under a new `schema:` label; never break the three
  envelope keys or the grammar the parser reads.

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
  "node created this session", "⚠️ NO open fulltext found", "(node created + FULLTEXT stored
  <date>)".
- **Lean:** discourage. These are look-up-able store facts; recording them in an L3 doc duplicates
  catalog state and goes stale silently when an artifact is evicted or re-fetched — the exact
  drift reference-never-copy exists to prevent. L3 entry stays `id + note + edges`.
- **The real need underneath:** "which of my selected works can I read deeply right now?" is
  legitimate — but its home is a **store query** (a `has-fulltext` filter / `store have`) at
  read time, not a written-in note. If pursued, that's a separate `feedback-*` CLI task, not
  an L3 convention.
- **Not yet landed because:** wants confirmation across more than one session before it goes
  into the settled contract, and the store-query alternative isn't built yet.

### Attributed voice for a source's theoretical claims

- **Status:** leaning-allow
- **Source:** `feedback-attributed-voice-for-source-claims`
- **Behavior:** whether a source's contested theoretical construct may be written in the doc's
  own assertive voice ("value is a real abstraction") or must be attributed to the source who
  claims it ("Sohn-Rethel's *real abstraction* holds that…").
- **Lean:** attribute. Split the doc into two voices by what is being stated:
  - **Assertive voice** — the doc may state as settled only *graph facts* (ids, edges,
    citation structure it looks up from the store) and *the user's own positions*.
  - **Attributed voice** — *any source's theoretical claim* is reported, never asserted: name
    the claimant with the claim. This is reference-never-copy carried from a source's metadata
    to its ontology — don't adopt a source's contested construct as the doc's own truth.
- **Point-of-use marker.** A convention fires only while *authoring*; a later session *reading*
  the doc needs the skepticism to travel with the term. At a contested construct's first
  appearance, attach an inline marker carrying the concept, the claim, the user's stance, and
  the attribute-don't-assert instruction:

  > **⟨attributed⟩ `real abstraction` — Sohn-Rethel.** Claims value is a real abstraction. User
  > stance: skeptical — grants that other idealisms can be shown to fall out of it, denies that
  > it is itself real. Attribute to the source; never assert in the doc's own voice.

- **Trigger case:** `materialist-signal-theory.l3.md` — the Sohn-Rethel "real abstraction"
  entries and the Q3 findings that leaned on it as "the answer."
- **Not yet landed because:** the register split and the marker shape both want a few real
  sessions before graduating into the settled contract.

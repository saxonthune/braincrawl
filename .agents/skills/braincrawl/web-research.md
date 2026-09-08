# Web research — source pages and current evidence

Read `SKILL.md` first. This guide covers the **web-verification** and **hypothesis or rough
model** workflows, where a web source or an open question should be investigated before the
Research Collection is consulted.

## Route

Choose this workflow when the user supplies a URL, asks about a current or changing claim, needs
a source the Catalog does not represent well, or proposes a model and asks whether prior research
supports it. State the route before searching:

> Workflow: web verification. I will inspect the supplied source first, then use the Catalog or
> Library if the claim needs a durable work identity or a citable artifact.

Do not run a Research Collection survey as a prerequisite. Hand off to the **existing
collection** workflow when the question becomes "what have we already recorded?" Hand off to
**source-centered reading** when a page or PDF becomes the work being read in detail.

## Evidence order

1. **Clarify the claim.** Separate the user's question from the source's assertions. Note the
   date or version when the claim can change.
2. **Search the web or open the supplied source.** Prefer the primary page, paper, dataset,
   official record, or author copy. Use WebFetch for a known URL and web search for discovery.
   Record the URL and the relevant passage while it is in context.
3. **Use the Catalog when the source is a scholarly work.** Search OpenAlex or another provider
   for a durable work record and citation context. Use `--skip-push` for a temporary lookup;
   allow the normal push when the work belongs in the shared Catalog.

   ```bash
   braincrawl --text --skip-push openalex search works "<query>"
   braincrawl --text openalex get <id> --abstract
   ```

4. **Use the Library when the question needs the artifact.** If an open-access artifact is
   available for a work already in the Catalog, acquire it with `library fetch`. For a local PDF,
   hand off to `pdf-import.md`; for passage-level questions, hand off to `reading-guide.md`.

   ```bash
   braincrawl library fetch <id> --require-pdf
   braincrawl library extract-text <id>
   braincrawl library paginate <id>
   braincrawl library read <id> --find "<phrase>"
   ```

5. **Compare the evidence.** For a hypothesis, distinguish overlap, support, contradiction, and
   an unresolved distinction. A search result or abstract is a lead; cite the source passage that
   supports the conclusion.
6. **Record only the durable result.** If the user wants a Research Collection update, create or
   locate the relevant document after the source work. Keep the source URL and access date in the
   remarks. Add a Catalog link when the work has been landed in the store; do not copy provider
   metadata into the document.

## Web sources and Library artifacts are different

Web search and WebFetch answer a source-specific question in the current session. `library fetch`
acquires an open artifact for a work already identified in the Catalog, so later sessions can read
the bytes through the Library. Use both when the question needs immediate web evidence and a
reusable artifact; do not assume that a page discovered on the web has been stored.

## Reporting the route

Say what you searched or opened, what it established, and what remains uncertain. If no durable
Research Collection write is warranted, leave the collection unchanged and return the evidence
with its source link. If the work has become a collection finding, state the handoff and reindex
the Research Collection after editing it.

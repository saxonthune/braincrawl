## Documentation
This repo uses a `.rhidoc/` spec workspace. Read `.rhidoc/AGENTS.md` for how to
navigate and edit it, and `.rhidoc/MANIFEST.md` for the doc index.

**Read the glossary (`doc01.01`, `.rhidoc/01-product/01-glossary.md`) at the start
of every session, before touching anything.** It is the controlled vocabulary and
the authority on the domain model — in particular the identity model: a work's
**canonical id** is a braincrawl-minted UUID, and every external scheme (an OpenAlex
`W…` id, a DOI, a PMID) is only an **alias** that resolves to it. Never re-derive
these facts from code; the code layer can contradict the model (and in the CLI has),
so the glossary is the source of truth. A `SessionStart` hook prints it into context;
if that hook is absent, read the file yourself.

## Naming
The glossary (`doc01.01`) is the controlled vocabulary, and new domain names are
the user's call: propose candidates with collisions and tradeoffs, and let the user
commit the term to the glossary before it fans out. Prefer combining existing
terms over minting new ones.

## Shell commands: avoid approval prompts
Shell hygiene rules (avoiding command forms that trigger approval prompts) live in
the user-level agent guidance shared across all repos.

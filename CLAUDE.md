## Documentation
This repo uses a `.rhidoc/` spec workspace. Read `.rhidoc/AGENTS.md` for how to
navigate and edit it, and `.rhidoc/MANIFEST.md` for the doc index.

## Naming
The glossary (`doc01.01`) is the controlled vocabulary, and new domain names are
the user's call: propose candidates with collisions and tradeoffs, and let the user
commit the term to the glossary before it fans out. Prefer combining existing
terms over minting new ones.

## Shell commands: avoid approval prompts
Shell hygiene rules (avoiding command forms that trigger approval prompts) live in
the user-level `~/.claude/CLAUDE.md`, shared across all repos.

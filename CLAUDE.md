## Documentation
This repo uses a `.rhidoc/` spec workspace. Read `.rhidoc/AGENTS.md` for how to
navigate and edit it, and `.rhidoc/MANIFEST.md` for the doc index.

## Shell commands: avoid approval prompts
Prefer Bash commands that can be auto-approved. Avoid constructs that the permission
system flags for explicit approval. We build this list as we hit them:

- **Shell parameter/command expansion** (`simple_expansion`) — e.g. `$?`, `$VAR`,
  `$(...)`, backticks. Don't use them to inspect or branch on state. Instead chain
  with `&&`/`||`, let a command's own non-zero exit surface failure, or split into
  separate calls.

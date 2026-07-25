# Agent Result: doctor-and-upgrade

date: 2026-07-25T10:34:47-04:00
session: completed
verification: passed
commits: 5
branch: feat/milestone01_claude_doctor-and-upgrade
surface deviations: none
turns: 83/200
cost: $3.920642999999999/$5.00
uncommitted: 1 files, 1 lines
session id: 49d55664-9ba2-4ff6-8371-d7d10abe0e29


## Summary

None. All Surface-declared symbols (`Check`, the registry entries, `applied_migrations()`, the `/health` fields, the `Doctor` namespace variant, the collapsed `upgrade` recipe) match the plan exactly.

## Commits

```
da1bf6e readme: point Setup at just upgrade and braincrawl doctor
e04d41a justfile: collapse repair recipes into one converging upgrade
89a6199 cli: add braincrawl doctor command
91b2fa0 server+cli: build stamp on CLI, migrations/l3_root on /health
16f688e store: add applied_migrations() to MetadataStore
```

## Build & Test Output (last 30 lines)

```
  }
]
braincrawl da1bf6e-dirty
Available recipes:
    build                 # Build the server + CLI binaries (debug)
    build-release         # Build optimized release binaries (server script + consumers point here)
    default               # List available recipes
    deploy-worker         # if you haven't already).
    generate-worker-token # KV key (no TTL is intentional — CLI tokens are meant to persist).
    install               # `just watch` rebuilds on save instead; `just install` is also how you undo it.)
    luminous-atlas        # Regenerate the Atlas Data File from the CLI grammar signal (refreshes the signal first)
    luminous-cli          # Regenerate the Luminous CLI-grammar canvas in .luminous/ from the clap definition
    skill-install         # Install the braincrawl skill for Claude Code into ~/.claude/skills
    systemd-install       # Idempotent — re-run after editing the template. Undo: `just systemd-uninstall`.
    systemd-install-cli   # Install + start the service without the Web UI (for CLI/Claude Code users)
    systemd-logs          # Tail the server log (journald)
    systemd-restart       # Bounce the service (does NOT rebuild — use `just upgrade` to pick up code changes)
    systemd-start         # Start the shared server (systemd user service)
    systemd-status        # Show whether the server is up + /health
    systemd-stop          # Stop the shared server
    systemd-uninstall     # Remove the systemd user service (stops it, disables boot start, drops the unit)
    test                  # Run the workspace test suite
    upgrade               # sees the result. Idempotent — always safe to rerun.
    watch                 # CLI. Undo the symlink (restore a standalone binary) with `just install`.
    web-build             # Production build of the Web UI (web/dist)
    web-check             # Format + lint + type-check the Web UI in one pass
    web-dev               # Dev server for the Web UI
    web-eval              # Run only the golden-turn harness tests (the fixed offline eval set)
    web-test              # Web UI tests (Vitest)
    worker-test           # --local (emulated D1/R2/KV) and runs the shared conformance suite against it.
```

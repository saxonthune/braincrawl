#!/usr/bin/env python3
"""Project the clap-derived CLI grammar into an Atlas Data File.

Reads `.luminous/cli-grammar.signal.json` — the faithful clap dump written by
`just luminous-cli` — and writes one entry per command into a
`<name>.atlasdata.json` sidecar. An Atlas Node whose Content names one of these
keys draws the generated text instead of its authored fallback.

The signal is the input, not `cli.rs`, so this script parses no Rust: the
grammar it projects is the grammar clap built from the real `Cli` type. Run
`just luminous-cli` first whenever the CLI has changed.

Usage:
    python3 .luminous/atlas/gen-atlasdata.py [--out PATH] [--signal PATH]
"""

from __future__ import annotations

import argparse
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_SIGNAL = REPO / ".luminous" / "cli-grammar.signal.json"
DEFAULT_OUT = REPO / ".luminous" / "braincrawl.atlasdata.json"
# Where the grammar comes from. clap introspection gives no line spans, so the
# entry records the file and revision without a line range.
CLI_SOURCE = "apps/cli/src/cli.rs"


def git_rev() -> str | None:
    try:
        out = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        )
        return out.stdout.strip() or None
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def segments(node: dict) -> list[str]:
    """The signal's `path` is slash-joined (`braincrawl/openalex/get`)."""
    return node["path"].split("/")


def key_for(path: list[str]) -> str:
    """`["braincrawl","openalex","get"]` -> `cli.braincrawl.openalex.get`.

    Keys mirror the Atlas node ids the CLI section already uses, so binding a
    Node to its key is a matter of copying the id.
    """
    return "cli." + ".".join(path)


def usage(path: list[str], args: list[dict]) -> str:
    parts = list(path)
    for arg in args:
        if arg["kind"] != "positional":
            continue
        name = arg["name"]
        parts.append(f"<{name}>" if arg["required"] else f"[{name}]")
    if any(a["kind"] != "positional" for a in args):
        parts.append("[options]")
    return " ".join(parts)


def arg_line(arg: dict) -> str:
    """One line per arg: what the user types, then what it means.

    Shaped like YAML for reading, not for parsing — clap help text carries
    colons and dashes that no parser would survive, and this block is display
    text inside a fence.
    """
    if arg["kind"] == "positional":
        label = arg["name"]
    else:
        flags = []
        if arg["short"]:
            flags.append(f"-{arg['short']}")
        if arg["long"]:
            flags.append(f"--{arg['long']}")
        label = ", ".join(flags) or arg["name"]
        if arg["kind"] == "option":
            label += f" <{arg['name'].upper()}>"

    notes = []
    if arg["required"]:
        notes.append("required")
    if arg.get("global"):
        notes.append("global")
    for default in arg.get("defaults") or []:
        notes.append(f"default {default}")
    values = arg.get("possibleValues") or []
    # clap reports a bare flag's values as true/false; that is noise, not a choice.
    if values and set(values) != {"true", "false"}:
        notes.append("one of " + "|".join(values))

    help_text = (arg.get("help") or "").strip().replace("\n", " ")
    described = " — ".join([t for t in [", ".join(notes), help_text] if t])
    return f"  {label}:{' ' + described if described else ''}"


def entry_text(node: dict) -> str:
    """The markdown a command Node draws: usage, description, then an args block."""
    path = segments(node)
    args = node.get("args") or []
    lines = [f"`{usage(path, args)}`"]

    about = (node.get("about") or "").strip()
    if about:
        lines += ["", about]

    positionals = [a for a in args if a["kind"] == "positional"]
    options = [a for a in args if a["kind"] != "positional"]
    if positionals or options:
        lines += ["", "```yaml"]
        if positionals:
            lines.append("positionals:")
            lines += [arg_line(a) for a in positionals]
        if options:
            lines.append("options:")
            lines += [arg_line(a) for a in options]
        lines.append("```")

    subs = [s["name"] for s in node.get("subcommands") or []]
    if subs:
        lines += ["", "subcommands: " + ", ".join(sorted(subs))]

    if node.get("hidden"):
        lines += ["", "_hidden from `--help`._"]

    return "\n".join(lines)


def walk(node: dict, out: dict[str, dict], source: dict) -> None:
    out[key_for(segments(node))] = {"text": entry_text(node), "source": source}
    for sub in node.get("subcommands") or []:
        walk(sub, out, source)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--signal", type=Path, default=DEFAULT_SIGNAL)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--of", default="braincrawl.atlas.json")
    args = parser.parse_args()

    if not args.signal.exists():
        print(f"missing {args.signal} — run `just luminous-cli` first")
        return 1

    signal = json.loads(args.signal.read_text())

    source = {"path": CLI_SOURCE}
    rev = git_rev()
    if rev:
        source["rev"] = rev

    entries: dict[str, dict] = {}
    walk(signal, entries, source)

    data = {
        "v": 1,
        "of": args.of,
        "generatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "entries": {k: entries[k] for k in sorted(entries)},
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(data, indent=2) + "\n")
    print(f"wrote {len(entries)} entries to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

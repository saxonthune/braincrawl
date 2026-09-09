//! Luminous CLI-grammar pipeline.
//!
//! Walks the real clap `Cli` definition (via `CommandFactory`) and emits, into
//! the workspace `.luminous/` directory:
//!
//!   - `cli-grammar.signal.json` — the raw clap-derived grammar tree. This is the
//!     "source signal": a faithful, human-readable enumeration of every command,
//!     subcommand, flag and positional, with its metadata. Generated, never
//!     hand-maintained, so it cannot drift from `cli.rs`.
//!   - `cli-grammar.graph.json`  — a Luminous v3 graph (nodes + edges) derived
//!     from the signal.
//!   - `cli-grammar.pack.json`   — the Luminous vocabulary (node/edge kinds +
//!     one view) the graph renders against.
//!
//! Regenerate after any change to the CLI grammar:
//!   cargo run -p braincrawl-cli --example luminous_cli_grammar
//!
//! The pipeline follows the canonical shape: parse source (clap) → collect nodes
//! → collect edges → sort deterministically → write files. Node/edge IDs are
//! derived from the command path, so re-running produces a diffable update.

use braincrawl_cli::cli::Cli;
use clap::{Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. clap → signal tree (the source-signal documentation)
    let cmd = Cli::command();
    let signal = command_signal(&cmd, &[cmd.get_name().to_string()]);

    // 2/3. signal → Luminous nodes + edges
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    build_graph(&signal, None, &mut nodes, &mut edges);

    // 4. deterministic order → diffable output
    nodes.sort_by(|a, b| a["id"].as_str().unwrap_or("").cmp(b["id"].as_str().unwrap_or("")));
    edges.sort_by(|a, b| a["id"].as_str().unwrap_or("").cmp(b["id"].as_str().unwrap_or("")));

    let graph = json!({
        "version": 3,
        "pack": "cli-grammar",
        "nodes": nodes,
        "edges": edges,
        "defaultView": "cli-grammar",
    });

    // 5. write artifacts
    let dir = luminous_dir();
    std::fs::create_dir_all(&dir)?;
    write_pretty(&dir.join("cli-grammar.signal.json"), &signal)?;
    write_pretty(&dir.join("cli-grammar.graph.json"), &graph)?;
    write_pretty(&dir.join("cli-grammar.pack.json"), &pack())?;

    eprintln!(
        "wrote {} command node(s) + flags to {}",
        count_kind(&graph, "cli.command"),
        dir.display()
    );
    Ok(())
}

// ── clap introspection → signal ────────────────────────────────────────────────

/// Recursively describe a clap `Command` as a plain JSON signal node.
/// `path` is the chain of names from the root (e.g. `["braincrawl","openalex","get"]`).
fn command_signal(cmd: &Command, path: &[String]) -> Value {
    let is_root = path.len() == 1;

    let mut args: Vec<Value> = Vec::new();
    for arg in cmd.get_arguments() {
        let id = arg.get_id().as_str();
        // Auto-generated help/version args are clap noise, not authored boundaries.
        if id == "help" || id == "version" {
            continue;
        }
        // Global flags are declared once on the root; don't duplicate them onto
        // every subcommand.
        if !is_root && arg.is_global_set() {
            continue;
        }
        args.push(arg_signal(arg));
    }

    let mut subs: Vec<Value> = Vec::new();
    for sub in cmd.get_subcommands() {
        let mut p = path.to_vec();
        p.push(sub.get_name().to_string());
        subs.push(command_signal(sub, &p));
    }

    json!({
        "name": cmd.get_name(),
        "path": path.join("/"),
        "about": cmd.get_about().map(|s| s.to_string()),
        "hidden": cmd.is_hide_set(),
        "args": args,
        "subcommands": subs,
    })
}

/// Describe a single clap `Arg` as a signal node.
fn arg_signal(arg: &Arg) -> Value {
    let kind = if arg.is_positional() {
        "positional"
    } else if matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    ) {
        "flag"
    } else {
        "option"
    };

    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

    let possible: Vec<String> = arg
        .get_possible_values()
        .iter()
        .map(|pv| pv.get_name().to_string())
        .collect();

    json!({
        "name": arg.get_id().as_str(),
        "long": arg.get_long(),
        "short": arg.get_short().map(|c| c.to_string()),
        "kind": kind,
        "required": arg.is_required_set(),
        "global": arg.is_global_set(),
        "defaults": defaults,
        "possibleValues": possible,
        "help": arg.get_help().map(|s| s.to_string()),
    })
}

// ── signal → Luminous graph ────────────────────────────────────────────────────

/// Walk the signal tree, emitting a `cli.command` node per command, a `cli.flag`
/// node per arg, and `cli.contains` edges for both subcommand and flag ownership.
fn build_graph(signal: &Value, parent_id: Option<&str>, nodes: &mut Vec<Value>, edges: &mut Vec<Value>) {
    let path = signal["path"].as_str().unwrap_or("");
    let dotted = path.replace('/', ".");
    let cmd_id = format!("command.{dotted}");
    let depth = dotted.matches('.').count();

    let name = signal["name"].as_str().unwrap_or("");
    nodes.push(json!({
        "id": cmd_id,
        "kind": "cli.command",
        "props": {
            "name": name,
            "path": path,
            "about": signal["about"].as_str().unwrap_or(""),
            "hidden": signal["hidden"].as_bool().unwrap_or(false),
            "depth": depth,
        },
        // Tag by terminal name so the same verb across branches (the three `get`s,
        // the four `refs`) is groupable as a cross-tree similarity.
        "tags": [name],
    }));

    if let Some(pid) = parent_id {
        edges.push(contains_edge(pid, &cmd_id));
    }

    if let Some(args) = signal["args"].as_array() {
        for arg in args {
            let flag = flag_node(&dotted, arg);
            let flag_id = flag["id"].as_str().unwrap().to_string();
            edges.push(contains_edge(&cmd_id, &flag_id));
            nodes.push(flag);
        }
    }

    if let Some(subs) = signal["subcommands"].as_array() {
        for sub in subs {
            build_graph(sub, Some(&cmd_id), nodes, edges);
        }
    }
}

/// Build a `cli.flag` node, omitting absent optional props so the pack's string
/// schemas never see a null.
fn flag_node(dotted: &str, arg: &Value) -> Value {
    let name = arg["name"].as_str().unwrap_or("");
    let id = format!("flag.{dotted}.{name}");

    let mut props = Map::new();
    props.insert("name".into(), json!(name));
    props.insert("kind".into(), arg["kind"].clone());
    props.insert("required".into(), json!(arg["required"].as_bool().unwrap_or(false)));
    props.insert("global".into(), json!(arg["global"].as_bool().unwrap_or(false)));
    if let Some(l) = arg["long"].as_str() {
        props.insert("long".into(), json!(format!("--{l}")));
    }
    if let Some(s) = arg["short"].as_str() {
        props.insert("short".into(), json!(format!("-{s}")));
    }
    if let Some(h) = arg["help"].as_str() {
        props.insert("help".into(), json!(h));
    }
    if let Some(defaults) = arg["defaults"].as_array() {
        let joined: Vec<&str> = defaults.iter().filter_map(|v| v.as_str()).collect();
        if !joined.is_empty() {
            props.insert("default".into(), json!(joined.join(",")));
        }
    }
    if let Some(pv) = arg["possibleValues"].as_array() {
        let joined: Vec<&str> = pv.iter().filter_map(|v| v.as_str()).collect();
        if !joined.is_empty() {
            props.insert("values".into(), json!(joined.join("|")));
        }
    }

    // Tag by bare arg name and kind so the same arg recurring across the tree
    // (every `id` positional, every `--from` option) is groupable. This is the
    // signal that surfaces similarities the nested-per-command structure hides.
    let kind = arg["kind"].as_str().unwrap_or("");
    json!({
        "id": id,
        "kind": "cli.flag",
        "props": Value::Object(props),
        "tags": [name, format!("kind:{kind}")],
    })
}

fn contains_edge(from: &str, to: &str) -> Value {
    json!({
        "id": format!("edge.cli.contains.{from}.{to}"),
        "kind": "cli.contains",
        "from": from,
        "to": to,
        "props": {},
        "tags": [],
    })
}

// ── pack (vocabulary + view) ───────────────────────────────────────────────────

fn pack() -> Value {
    json!({
        "id": "cli-grammar",
        "version": "0.1.0",
        "description": "The braincrawl CLI grammar — every command, subcommand and flag, derived from the clap definition.",
        "nodeKinds": [
            {
                "id": "cli.command",
                "label": "Command",
                "props": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "path": { "type": "string" },
                        "about": { "type": "string" },
                        "hidden": { "type": "boolean" },
                        "depth": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["name", "path"],
                    "additionalProperties": false
                },
                "render": {
                    "peek": { "type": "text", "value": "{content.name}", "style": "heading" },
                    "card": {
                        "type": "card", "shape": "rectangle", "padding": 12,
                        "children": [
                            {
                                "type": "hstack", "gap": 6, "justify": "space-between",
                                "children": [
                                    { "type": "text", "value": "{content.name}", "style": "heading" },
                                    { "type": "badge", "value": "command", "tone": "muted" }
                                ]
                            },
                            { "type": "text", "value": "{content.about}", "style": "caption", "tone": "muted" }
                        ]
                    }
                }
            },
            {
                "id": "cli.flag",
                "label": "Flag / Arg",
                "props": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "kind": { "type": "string" },
                        "long": { "type": "string" },
                        "short": { "type": "string" },
                        "required": { "type": "boolean" },
                        "global": { "type": "boolean" },
                        "default": { "type": "string" },
                        "values": { "type": "string" },
                        "help": { "type": "string" }
                    },
                    "required": ["name", "kind"],
                    "additionalProperties": false
                },
                "render": {
                    "peek": { "type": "text", "value": "{content.name}", "style": "mono" },
                    "card": {
                        "type": "card", "shape": "pill", "padding": 8,
                        "children": [
                            {
                                "type": "hstack", "gap": 6,
                                "children": [
                                    { "type": "text", "value": "{content.name}", "style": "mono" },
                                    { "type": "badge", "value": "{content.kind}", "tone": "accent" }
                                ]
                            },
                            { "type": "text", "value": "{content.help}", "style": "caption", "tone": "muted" }
                        ]
                    }
                }
            }
        ],
        "edgeKinds": [
            {
                "id": "cli.contains",
                "label": "contains",
                "directed": true,
                "props": { "type": "object", "properties": {}, "additionalProperties": false },
                "acceptsSource": ["cli.command"],
                "acceptsTarget": ["cli.command", "cli.flag"]
            }
        ],
        "views": [
            {
                "id": "cli-grammar",
                "name": "CLI Grammar",
                "description": "The full command tree with flags nested under their command.",
                "zoomToLevel": [
                    { "minZoom": 0,   "level": "peek" },
                    { "minZoom": 0.4, "level": "card" },
                    { "minZoom": 1.2, "level": "open" },
                    { "minZoom": 3.0, "level": "deep" }
                ],
                "nodeRoles": { "cli.command": "spatial", "cli.flag": "spatial" },
                "edgeRoles": { "cli.contains": "contain" },
                "layers": {},
                "layout": { "algorithm": "elk" }
            }
        ],
        "layers": [],
        "disclosure": [
            {
                "kind": "cli.command",
                "peek": ["name"],
                "card": ["name", "about"],
                "open": ["name", "path", "about", "hidden"],
                "deep": ["name", "path", "about", "hidden", "depth"]
            },
            {
                "kind": "cli.flag",
                "peek": ["name"],
                "card": ["name", "kind", "help"],
                "open": ["name", "kind", "long", "short", "required", "help"],
                "deep": ["name", "kind", "long", "short", "required", "global", "default", "values", "help"]
            }
        ]
    })
}

// ── io helpers ─────────────────────────────────────────────────────────────────

/// Workspace `.luminous/` directory, resolved relative to this crate (apps/cli).
fn luminous_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root is two levels above apps/cli")
        .join(".luminous")
}

fn write_pretty(path: &std::path::Path, value: &Value) -> std::io::Result<()> {
    let mut s = serde_json::to_string_pretty(value)?;
    s.push('\n');
    std::fs::write(path, s)
}

fn count_kind(graph: &Value, kind: &str) -> usize {
    graph["nodes"]
        .as_array()
        .map(|ns| ns.iter().filter(|n| n["kind"].as_str() == Some(kind)).count())
        .unwrap_or(0)
}

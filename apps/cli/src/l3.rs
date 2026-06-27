//! The consolidated L3 document store.
//!
//! L3 is the per-consumer projection layer: questions, selections, annotations,
//! and domain edges that point at canonical ids in the shared L1/L2 store. Unlike
//! L1/L2 (which live in the server's DB), L3 docs are plain markdown files — one
//! `<doc>.l3.md` per item — consolidated under a single, config-driven root so
//! knowledge stops scattering into per-project repos.
//!
//! The contract is deliberately split in two:
//!   * **Envelope** (frozen): a small set of frontmatter keys the tooling reads to
//!     file, find, and index a doc *without parsing its body* — `doc`, `schema`,
//!     `updated`. `REQUIRED_FRONTMATTER` is the single place that grows over time.
//!   * **Body** (free): everything below the frontmatter. The `schema` key labels
//!     the convention in use so many document designs can coexist and be linted
//!     (or not) per their declared schema. `freeform` is the no-lint escape hatch.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::L3Cmd;
use crate::config::Config;
use crate::cli::OutputOpts;
use crate::output::{render, Envelope, QueryMeta};

/// Frontmatter keys the tooling requires. Extend this as the contract firms up;
/// `check` warns (never fails) on any missing key.
pub const REQUIRED_FRONTMATTER: &[&str] = &["doc", "schema", "updated"];

const FILE_SUFFIX: &str = ".l3.md";

type DynErr = Box<dyn std::error::Error>;

/// One L3 doc as seen by the index/list/check surfaces.
struct DocMeta {
    doc: String,
    schema: String,
    /// Declared date from frontmatter `updated:` (set at create/import; can drift).
    updated: String,
    /// Real filesystem last-modified date (always reflects the latest edit locally).
    modified: String,
    title: String,
    path: PathBuf,
    /// Doc slugs this doc points at, harvested from `[[wikilinks]]` + `siblings:`.
    links: Vec<String>,
    /// Canonical ids referenced in the body (`openalex:…`/`doi:…`) — the split-proof
    /// join key. A doc can split without these moving, so an index keyed on them survives.
    ids: Vec<String>,
}

pub fn dispatch(cmd: L3Cmd, config: &Config, opts: &OutputOpts) -> Result<(), DynErr> {
    let root = config.l3_root();
    match cmd {
        L3Cmd::New { doc, schema, title, force } => cmd_new(&root, &doc, &schema, title, force),
        L3Cmd::Path { doc } => cmd_path(&root, &doc),
        L3Cmd::List => cmd_list(&root, opts),
        L3Cmd::Check { doc, all } => cmd_check(&root, doc, all),
        L3Cmd::Index => cmd_index(&root),
        L3Cmd::Import { file, doc, schema, mv } => cmd_import(&root, &file, doc, schema, mv),
        L3Cmd::Rm { doc } => cmd_rm(&root, &doc),
    }
}

// ── commands ────────────────────────────────────────────────────────────────

fn cmd_new(root: &Path, doc: &str, schema: &str, title: Option<String>, force: bool) -> Result<(), DynErr> {
    validate_slug(doc)?;
    ensure_root(root)?;
    let path = doc_path(root, doc);
    if path.exists() && !force {
        return Err(format!(
            "doc already exists: {} (use `l3 path {doc}` to locate, or --force to overwrite)",
            path.display()
        )
        .into());
    }
    let title = title.unwrap_or_else(|| doc.to_string());
    let content = scaffold(doc, schema, &title);
    fs::write(&path, content)?;
    reindex(root)?;
    // stdout = the absolute path only, so `p=$(braincrawl l3 new foo)` works.
    println!("{}", path.display());
    eprintln!("created: doc={doc} schema={schema}");
    Ok(())
}

fn cmd_path(root: &Path, doc: &str) -> Result<(), DynErr> {
    let path = doc_path(root, doc);
    if !path.exists() {
        return Err(format!("no such doc: {doc} (looked for {})", path.display()).into());
    }
    println!("{}", path.display());
    Ok(())
}

fn cmd_list(root: &Path, opts: &OutputOpts) -> Result<(), DynErr> {
    let mut docs = scan(root)?;
    // Most-recently-edited first, so "keywords + rough time of last edit" is a scan
    // down the list. `modified` is `YYYY-MM-DD`, so a lexical sort is chronological.
    docs.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.doc.cmp(&b.doc)));
    if opts.text && !opts.json {
        for d in &docs {
            println!("{}\t{}\t{}\t{}\t{}", d.doc, d.schema, d.modified, d.updated, d.path.display());
        }
        eprintln!("{} doc(s) in {}", docs.len(), root.display());
        return Ok(());
    }
    let results: Vec<serde_json::Value> = docs
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.doc,
                "doc": d.doc,
                "schema": d.schema,
                "modified": d.modified,
                "updated": d.updated,
                "title": d.title,
                "path": d.path.display().to_string(),
            })
        })
        .collect();
    let count = results.len() as u64;
    let envelope = Envelope {
        query: QueryMeta { entity: Some("l3:list".to_string()), resolved_filter: None, url: None },
        count,
        returned: results.len(),
        truncated: false,
        next_cursor: None,
        results,
    };
    render(&envelope, opts);
    Ok(())
}

fn cmd_check(root: &Path, doc: Option<String>, all: bool) -> Result<(), DynErr> {
    let targets: Vec<DocMeta> = if all {
        scan(root)?
    } else {
        let doc = doc.expect("clap guarantees doc unless --all");
        let path = doc_path(root, &doc);
        if !path.exists() {
            return Err(format!("no such doc: {doc} (looked for {})", path.display()).into());
        }
        vec![read_meta(&path)?]
    };

    let mut total_warns = 0usize;
    for d in &targets {
        let content = fs::read_to_string(&d.path)?;
        let warns = lint(&d.path, &content);
        if warns.is_empty() {
            eprintln!("ok: {}", d.doc);
        } else {
            for w in &warns {
                eprintln!("warn: {} — {w}", d.doc);
            }
            total_warns += warns.len();
        }
    }
    eprintln!("{} doc(s) checked, {} warning(s)", targets.len(), total_warns);
    Ok(())
}

fn cmd_index(root: &Path) -> Result<(), DynErr> {
    let path = reindex(root)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn cmd_import(
    root: &Path,
    file: &str,
    doc_override: Option<String>,
    schema_override: Option<String>,
    mv: bool,
) -> Result<(), DynErr> {
    ensure_root(root)?;
    let src = PathBuf::from(file);
    let content = fs::read_to_string(&src)
        .map_err(|e| format!("cannot read {}: {e}", src.display()))?;

    let (mut fm, body) = split_frontmatter(&content);

    // Determine the doc slug: --doc > existing `doc` > existing `domain` > filename stem.
    let doc = doc_override
        .or_else(|| fm_get(&fm, "doc"))
        .or_else(|| fm_get(&fm, "domain"))
        .unwrap_or_else(|| stem_slug(&src));
    validate_slug(&doc)?;

    // Normalize the envelope in place, preserving every other line (incl. multi-line
    // YAML values and unknown keys) verbatim.
    fm = rename_key(fm, "domain", "doc");
    fm = upsert_key(fm, "doc", &doc);
    let schema = schema_override
        .or_else(|| fm_get(&fm, "schema"))
        .unwrap_or_else(|| "freeform".to_string());
    fm = upsert_key(fm, "schema", &schema);
    let updated = fm_get(&fm, "updated").unwrap_or_else(today);
    fm = upsert_key(fm, "updated", &updated);

    let dst = doc_path(root, &doc);
    if dst.exists() {
        return Err(format!(
            "doc already exists: {} (remove it first or pick a different --doc)",
            dst.display()
        )
        .into());
    }
    let rebuilt = format!("---\n{}\n---\n{body}", fm.join("\n"));
    fs::write(&dst, rebuilt)?;
    if mv {
        fs::remove_file(&src).map_err(|e| format!("imported, but failed to remove source: {e}"))?;
    }
    reindex(root)?;
    println!("{}", dst.display());
    eprintln!("imported: doc={doc} schema={schema} (from {})", src.display());
    Ok(())
}

fn cmd_rm(root: &Path, doc: &str) -> Result<(), DynErr> {
    let path = doc_path(root, doc);
    if !path.exists() {
        return Err(format!("no such doc: {doc} (looked for {})", path.display()).into());
    }
    fs::remove_file(&path)?;
    reindex(root)?;
    eprintln!("removed: {doc}");
    Ok(())
}

// ── store helpers ─────────────────────────────────────────────────────────────

fn ensure_root(root: &Path) -> Result<(), DynErr> {
    fs::create_dir_all(root)
        .map_err(|e| format!("cannot create L3 root {}: {e}", root.display()))?;
    Ok(())
}

fn doc_path(root: &Path, doc: &str) -> PathBuf {
    root.join(format!("{doc}{FILE_SUFFIX}"))
}

/// Read every `*.l3.md` (skipping `_`-prefixed helpers) as a `DocMeta`, sorted by slug.
fn scan(root: &Path) -> Result<Vec<DocMeta>, DynErr> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('_') || !name.ends_with(FILE_SUFFIX) {
            continue;
        }
        out.push(read_meta(&entry.path())?);
    }
    out.sort_by(|a, b| a.doc.cmp(&b.doc));
    Ok(out)
}

fn read_meta(path: &Path) -> Result<DocMeta, DynErr> {
    let content = fs::read_to_string(path)?;
    let (fm, body) = split_frontmatter(&content);
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(FILE_SUFFIX).to_string())
        .unwrap_or_default();
    Ok(DocMeta {
        doc: fm_get(&fm, "doc").unwrap_or(stem),
        schema: fm_get(&fm, "schema").unwrap_or_else(|| "—".to_string()),
        updated: fm_get(&fm, "updated").unwrap_or_else(|| "—".to_string()),
        modified: modified_date(path),
        title: first_h1(&body).unwrap_or_default(),
        links: harvest_links(&fm, &body),
        ids: harvest_ids(&body),
        path: path.to_path_buf(),
    })
}

/// The file's last-modified date (`YYYY-MM-DD`) from filesystem mtime, or `—` if
/// unavailable. Reflects the real latest edit, unlike the declared `updated:` field.
fn modified_date(path: &Path) -> String {
    let secs = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    match secs {
        Some(s) => {
            let (y, m, d) = civil_from_days((s / 86_400) as i64);
            format!("{y:04}-{m:02}-{d:02}")
        }
        None => "—".to_string(),
    }
}

/// Regenerate `INDEX.md` — a deterministic manifest harvested from the docs themselves.
///
/// Three sections, each a pure function of the on-disk bytes (no RAG, no embeddings):
///   1. **Documents** — the envelope table (doc · schema · modified · updated · title).
///   2. **Cross-references** — the doc→doc link graph from `[[wikilinks]]` + `siblings:`,
///      with back-references computed so "what refers to this doc" is a lookup, not a grep.
///   3. **Work index** — the inverted index `canonical id → docs that reference it`. Keyed on
///      the work, not the doc, so it survives a doc splitting into pieces (the ids travel).
///
/// Returns the index path.
fn reindex(root: &Path) -> Result<PathBuf, DynErr> {
    ensure_root(root)?;
    let docs = scan(root)?;
    let known: BTreeSet<&str> = docs.iter().map(|d| d.doc.as_str()).collect();

    // doc → docs that link to it (known targets only; dangling links still render as out-edges)
    let mut backrefs: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for d in &docs {
        for l in &d.links {
            if l.as_str() != d.doc.as_str() && known.contains(l.as_str()) {
                backrefs.entry(l.as_str()).or_default().insert(d.doc.as_str());
            }
        }
    }

    // canonical id → docs referencing it
    let mut work_index: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for d in &docs {
        for id in &d.ids {
            work_index.entry(id.as_str()).or_default().insert(d.doc.as_str());
        }
    }

    let mut out = String::from("# L3 store index\n\n");
    out.push_str(&format!(
        "{} doc(s). Generated by `braincrawl l3 index` — do not edit by hand.\n\n",
        docs.len()
    ));

    out.push_str("## Documents\n\n");
    out.push_str("| doc | schema | modified | updated | title |\n|---|---|---|---|---|\n");
    for d in &docs {
        out.push_str(&format!(
            "| [{}]({}{}) | {} | {} | {} | {} |\n",
            d.doc, d.doc, FILE_SUFFIX, d.schema, d.modified, d.updated, d.title.replace('|', "\\|")
        ));
    }

    out.push_str("\n## Cross-references\n\n");
    out.push_str("Doc-to-doc links from `[[wikilinks]]` and `siblings:`. A dangling target (no such doc) is marked `?`.\n\n");
    out.push_str("| doc | → links out | ← linked from |\n|---|---|---|\n");
    for d in &docs {
        let out_links: Vec<String> = d
            .links
            .iter()
            .filter(|l| l.as_str() != d.doc.as_str())
            .map(|l| if known.contains(l.as_str()) { format!("[{l}]({l}{FILE_SUFFIX})") } else { format!("{l}?") })
            .collect();
        let in_links: Vec<String> = backrefs
            .get(d.doc.as_str())
            .map(|s| s.iter().map(|l| format!("[{l}]({l}{FILE_SUFFIX})")).collect())
            .unwrap_or_default();
        if out_links.is_empty() && in_links.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            d.doc,
            if out_links.is_empty() { "—".to_string() } else { out_links.join(", ") },
            if in_links.is_empty() { "—".to_string() } else { in_links.join(", ") },
        ));
    }

    out.push_str("\n## Work index\n\n");
    out.push_str("Canonical ids (`openalex:`/`doi:`) → docs that reference them — the split-proof join key.\n\n");
    out.push_str("| id | docs |\n|---|---|\n");
    for (id, slugs) in &work_index {
        let cells: Vec<String> = slugs.iter().map(|s| format!("[{s}]({s}{FILE_SUFFIX})")).collect();
        out.push_str(&format!("| `{id}` | {} |\n", cells.join(", ")));
    }

    let path = root.join("INDEX.md");
    fs::write(&path, out)?;
    Ok(path)
}

/// Advisory lint: envelope completeness + the one body invariant (reference, not copy).
fn lint(path: &Path, content: &str) -> Vec<String> {
    let mut warns = Vec::new();
    let (fm, body) = split_frontmatter(content);
    if fm.is_empty() {
        warns.push("no YAML frontmatter block".to_string());
    }
    for key in REQUIRED_FRONTMATTER {
        if fm_get(&fm, key).is_none() {
            warns.push(format!("missing required frontmatter key `{key}`"));
        }
    }
    // doc slug must match the filename stem so the primary key stays the join key.
    if let Some(doc) = fm_get(&fm, "doc") {
        let stem = path
            .file_name()
            .map(|n| n.to_string_lossy().trim_end_matches(FILE_SUFFIX).to_string())
            .unwrap_or_default();
        if doc != stem {
            warns.push(format!("frontmatter doc `{doc}` ≠ filename stem `{stem}`"));
        }
    }
    if fm_get(&fm, "domain").is_some() {
        warns.push("legacy `domain` key present — should be `doc`".to_string());
    }
    // Body invariant: an L3 references canonical ids, it does not copy metadata.
    if !body.contains("openalex:") && !body.contains("doi:") {
        warns.push("no canonical ids (openalex:/doi:) referenced — L3 is annotation-over-reference".to_string());
    }
    warns
}

// ── scaffolding ───────────────────────────────────────────────────────────────

fn scaffold(doc: &str, schema: &str, title: &str) -> String {
    let header = format!("---\ndoc: {doc}\nschema: {schema}\nupdated: {}\n---\n\n# L3 — {title}\n\n", today());
    let body = match schema {
        "spine" => SPINE_BODY,
        "dialectical" => DIALECTICAL_BODY,
        _ => "",
    };
    format!("{header}{body}")
}

const SPINE_BODY: &str = "\
## Questions (aim-directed frontier)
- Q1 …

## Selection set  (canonical id → tags · note)
- openalex:W…  #landmark #Q1  one-line note (id is source of truth; this note is convenience only)

## Domain edges  (src --type--> dst)
- openalex:W… --supports--> Q1

## Findings / annotations
- [Q1] …
";

const DIALECTICAL_BODY: &str = "\
## Question taxonomy (dialectical — questions beget sub-questions)
Status: ○ open · ◐ partial · ● answered · ⊘ blocked. Leaf ids are stable; sub-questions hang off with decimal ids.
- Q1 ○ …
  - Q1.1 ○ …

## Selection set  (canonical id → tags · note)
- openalex:W…  #landmark #Q1  one-line note (id is source of truth; this note is convenience only)

## Domain edges  (src --type--> dst)
- openalex:W… --supports--> Q1

## Findings / annotations
- [Q1] …
";

// ── frontmatter (line-preserving, dependency-free) ────────────────────────────

/// Split content into (frontmatter lines, body). Frontmatter is the block between a
/// leading `---` and the next `---`. Returns empty fm when there is no block.
fn split_frontmatter(content: &str) -> (Vec<String>, String) {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return (Vec::new(), content.to_string());
    }
    let mut fm = Vec::new();
    let mut rest = Vec::new();
    let mut in_fm = true;
    for line in lines {
        if in_fm && line == "---" {
            in_fm = false;
            continue;
        }
        if in_fm {
            fm.push(line.to_string());
        } else {
            rest.push(line);
        }
    }
    if in_fm {
        // Never closed — treat whole thing as body to avoid eating content.
        return (Vec::new(), content.to_string());
    }
    // Reattach body with a trailing newline so round-trips stay clean.
    let mut body = rest.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    (fm, body)
}

/// Read a top-level scalar `key: value` from frontmatter lines. Block scalars
/// (`key: >`) yield an empty/placeholder value, which is fine for envelope keys.
fn fm_get(fm: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in fm {
        if let Some(rest) = line.strip_prefix(&prefix) {
            // Only treat as a match if the char after the key is whitespace or EOL,
            // i.e. avoid `documented:` matching `doc:`.
            if line.starts_with(&format!("{key}: ")) || line == &format!("{key}:") {
                let v = rest.trim();
                if v.is_empty() || v == ">" || v == "|" {
                    return Some(String::new());
                }
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Set `key` to `value`, replacing the first existing top-level occurrence or
/// appending if absent. Other lines are preserved verbatim.
fn upsert_key(mut fm: Vec<String>, key: &str, value: &str) -> Vec<String> {
    let needle = format!("{key}: ");
    let bare = format!("{key}:");
    for line in fm.iter_mut() {
        if line.starts_with(&needle) || line == &bare {
            *line = format!("{key}: {value}");
            return fm;
        }
    }
    fm.push(format!("{key}: {value}"));
    fm
}

/// Rename the first top-level `from:` key to `to:`, keeping its value.
fn rename_key(mut fm: Vec<String>, from: &str, to: &str) -> Vec<String> {
    let needle = format!("{from}: ");
    let bare = format!("{from}:");
    for line in fm.iter_mut() {
        if let Some(rest) = line.strip_prefix(&needle) {
            *line = format!("{to}: {rest}");
            return fm;
        }
        if line == &bare {
            *line = format!("{to}:");
            return fm;
        }
    }
    fm
}

/// Read a top-level inline-flow YAML list (`key: [a, b, c]`) — the form L3 envelopes use
/// for `siblings:`. Block-sequence form is not parsed (envelopes don't use it).
fn fm_get_list(fm: &[String], key: &str) -> Vec<String> {
    let prefix = format!("{key}: ");
    for line in fm {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let rest = rest.trim();
            let inner = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(rest);
            return inner
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

/// Doc slugs this doc points at: `[[wikilink]]` targets in the body plus `siblings:` in
/// the envelope, each normalized to a bare slug (trailing `.l3.md`/`.md` stripped), deduped.
fn harvest_links(fm: &[String], body: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else { break };
        let inner = &rest[..end];
        // `[[slug|alias]]` → take the target before the pipe. Strip all whitespace so a
        // hard-wrapped link (`[[llm-context-\n  optimization]]`) rejoins to one slug.
        let target: String = inner.split('|').next().unwrap_or(inner).split_whitespace().collect();
        let slug = target.trim_end_matches(FILE_SUFFIX).trim_end_matches(".md");
        if !slug.is_empty() {
            set.insert(slug.to_string());
        }
        rest = &rest[end + 2..];
    }
    for s in fm_get_list(fm, "siblings") {
        let slug = s.trim_end_matches(FILE_SUFFIX).trim_end_matches(".md");
        if !slug.is_empty() {
            set.insert(slug.to_string());
        }
    }
    set.into_iter().collect()
}

/// Canonical ids referenced in the body (`openalex:…`, `doi:…`), deduped. Tokenizes on
/// whitespace and strips surrounding punctuation; the id-part must be ≥4 chars so the
/// `openalex:W…` scaffold placeholder is not mistaken for a real id.
fn harvest_ids(body: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    for raw in body.split_whitespace() {
        let tok = raw.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == ':'));
        let Some((prefix, rest)) = tok.split_once(':') else { continue };
        if (prefix == "openalex" || prefix == "doi") && rest.len() >= 4 {
            set.insert(tok.to_string());
        }
    }
    set.into_iter().collect()
}

fn first_h1(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
}

// ── slug + date ───────────────────────────────────────────────────────────────

fn validate_slug(doc: &str) -> Result<(), DynErr> {
    if doc.is_empty()
        || !doc.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || doc.starts_with('-')
        || doc.ends_with('-')
    {
        return Err(format!(
            "invalid doc slug `{doc}` — use kebab-case [a-z0-9-], no leading/trailing dash"
        )
        .into());
    }
    Ok(())
}

fn stem_slug(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = name.trim_end_matches(FILE_SUFFIX).trim_end_matches(".md");
    stem.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Today's UTC date as `YYYY-MM-DD`, dependency-free.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert days-since-Unix-epoch to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_validation() {
        assert!(validate_slug("mesopotamia-loop").is_ok());
        assert!(validate_slug("Process_Math").is_err());
        assert!(validate_slug("-bad").is_err());
        assert!(validate_slug("bad-").is_err());
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn frontmatter_roundtrip_preserves_unknown_and_multiline() {
        let src = "---\ndomain: foo-bar\nnote: >\n  multi\n  line\nupdated: 2026-06-17\n---\n\n# L3 — Foo\n\nbody openalex:W1\n";
        let (fm, body) = split_frontmatter(src);
        assert_eq!(fm_get(&fm, "domain").as_deref(), Some("foo-bar"));
        assert_eq!(fm_get(&fm, "updated").as_deref(), Some("2026-06-17"));
        let fm = rename_key(fm, "domain", "doc");
        let fm = upsert_key(fm, "schema", "freeform");
        assert_eq!(fm_get(&fm, "doc").as_deref(), Some("foo-bar"));
        assert_eq!(fm_get(&fm, "schema").as_deref(), Some("freeform"));
        // multi-line `note:` block survived
        assert!(fm.iter().any(|l| l == "  multi"));
        assert!(body.contains("openalex:W1"));
    }

    #[test]
    fn fm_get_does_not_prefix_match() {
        let fm = vec!["documented: yes".to_string()];
        assert_eq!(fm_get(&fm, "doc"), None);
    }

    #[test]
    fn date_epoch_zero_is_1970() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(31 + 28), (1970, 3, 1));
    }

    #[test]
    fn harvest_links_from_wikilinks_and_siblings() {
        let fm = vec!["siblings: [process-theology.l3.md, process-math]".to_string()];
        let body = "see [[process-software-design]], [[llm-context-\n  optimization]] and [[process-theology|the theology one]].";
        let links = harvest_links(&fm, body);
        assert_eq!(
            links,
            vec!["llm-context-optimization", "process-math", "process-software-design", "process-theology"]
        );
    }

    #[test]
    fn harvest_ids_dedupes_and_skips_placeholder() {
        let body = "- openalex:W2088189323 #key  note\n- (openalex:W2088189323) again\n- doi:10.1126/science.aaf2654\n- openalex:W…  scaffold placeholder";
        let ids = harvest_ids(body);
        assert_eq!(ids, vec!["doi:10.1126/science.aaf2654", "openalex:W2088189323"]);
    }

    #[test]
    fn lint_flags_missing_envelope_and_copy() {
        let warns = lint(Path::new("/x/foo.l3.md"), "# just a body\n");
        assert!(warns.iter().any(|w| w.contains("frontmatter")));
    }
}

//! The consolidated L3 document store.
//!
//! L3 is the per-consumer projection layer: questions, selections, annotations,
//! and domain edges that point at canonical ids in the shared L1/L2 store. Unlike
//! L1/L2 (which live in the server's DB), L3 docs are plain markdown files — one
//! `<doc>.l3.md` per item — consolidated under a single, config-driven root so
//! knowledge stops scattering into per-project repos.
//!
//! The contract is deliberately split in two:
//!   * **Frontmatter** (required): a small set of keys the tooling reads to file,
//!     find, and index a doc *without parsing its body* — `doc`.
//!     `REQUIRED_FRONTMATTER` is the single place that grows over time.
//!   * **Body** (free): everything below the frontmatter, written in one shared
//!     node grammar (see `doc02.01.04` in `.rhidoc/`) that every doc follows.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::cli::L3Cmd;
use crate::config::Config;
use crate::cli::OutputOpts;
use crate::output::{render, Envelope, QueryMeta};
use crate::store_client::{L3PutError, StoreClient};

/// Frontmatter keys the tooling requires. Extend this as the contract firms up;
/// `check` warns (never fails) on any missing key.
pub const REQUIRED_FRONTMATTER: &[&str] = &["doc"];

const FILE_SUFFIX: &str = ".l3.md";

type DynErr = Box<dyn std::error::Error>;

/// One L3 doc's frontmatter, as seen by the index/list/check surfaces. Body content
/// (title, links, ids) comes from the parsed graph, not from this struct.
struct DocumentMetadata {
    doc: String,
    /// Real filesystem last-modified date (always reflects the latest edit locally).
    modified: String,
    path: PathBuf,
}

pub fn dispatch(cmd: L3Cmd, config: &Config, opts: &OutputOpts) -> Result<(), DynErr> {
    let root = config.l3_root();
    match cmd {
        L3Cmd::New { doc, title, force } => cmd_new(&root, &doc, title, force),
        L3Cmd::Path { doc } => cmd_path(&root, &doc),
        L3Cmd::List => cmd_list(&root, opts),
        L3Cmd::Check { doc, all } => cmd_check(&root, doc, all),
        L3Cmd::Index => cmd_index(&root),
        L3Cmd::Import { file, doc, mv } => cmd_import(&root, &file, doc, mv),
        L3Cmd::Rm { doc } => cmd_rm(&root, &doc),
        L3Cmd::AssignIds { dry_run } => cmd_assign_ids(&root, dry_run),
        L3Cmd::ReadingList => cmd_reading_list(&root, opts),
        L3Cmd::Push { doc, dry_run, force } => {
            let store = StoreClient::new(&config.server_url).with_token(config.auth_token.clone());
            cmd_push(&root, &store, doc, dry_run, force)
        }
        L3Cmd::Pull { doc, dry_run } => {
            let store = StoreClient::new(&config.server_url).with_token(config.auth_token.clone());
            cmd_pull(&root, &store, doc, dry_run)
        }
    }
}

// ── commands ────────────────────────────────────────────────────────────────

fn cmd_new(root: &Path, doc: &str, title: Option<String>, force: bool) -> Result<(), DynErr> {
    validate_slug(doc)?;
    ensure_root(root)?;
    let path = doc_path(root, doc);
    if path.exists() && !force {
        return Err(format!(
            "doc already exists: {} (use `collection path {doc}` to locate, or --force to overwrite)",
            path.display()
        )
        .into());
    }
    let title = title.unwrap_or_else(|| doc.to_string());
    let content = scaffold(doc, &title);
    fs::write(&path, content)?;
    reindex(root)?;
    // stdout = the absolute path only, so `p=$(braincrawl collection new foo)` works.
    println!("{}", path.display());
    eprintln!("created: doc={doc}");
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
    let (graph, _warnings) = l3::parse(root);
    if opts.text && !opts.json {
        for d in &docs {
            println!("{}\t{}\t{}", d.doc, d.modified, d.path.display());
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
                "modified": d.modified,
                "title": doc_title(&graph, d),
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
    let targets: Vec<DocumentMetadata> = if all {
        scan(root)?
    } else {
        let doc = doc.expect("clap guarantees doc unless --all");
        let path = doc_path(root, &doc);
        if !path.exists() {
            return Err(format!("no such doc: {doc} (looked for {})", path.display()).into());
        }
        vec![read_meta(&path)?]
    };

    let (graph, parse_warnings) = l3::parse(root);
    let docs_with_catalog_ref: BTreeSet<&str> = graph
        .links
        .iter()
        .filter_map(|link| {
            let recorded_in = link.recorded_in.as_ref()?;
            let source_doc = graph.node_by_id(&recorded_in.0)?.provenance.doc.as_str();
            let has_catalog = [&link.source, &link.target]
                .iter()
                .any(|e| matches!(e, l3::Endpoint::Catalog(_)));
            has_catalog.then_some(source_doc)
        })
        .collect();

    let mut total_warns = 0usize;
    for d in &targets {
        let content = fs::read_to_string(&d.path)?;
        let mut warns = lint(&d.path, &content, docs_with_catalog_ref.contains(d.doc.as_str()));
        warns.extend(
            parse_warnings
                .iter()
                .filter(|w| w.doc == d.doc)
                .map(|w| format!("line {}: {}", w.line, w.message)),
        );
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
    mv: bool,
) -> Result<(), DynErr> {
    ensure_root(root)?;
    let src = PathBuf::from(file);
    let content = fs::read_to_string(&src)
        .map_err(|e| format!("cannot read {}: {e}", src.display()))?;

    let (fm, body) = split_frontmatter(&content);

    // Determine the doc slug: --doc > existing `doc` > existing `domain` > filename stem.
    let doc = doc_override
        .or_else(|| fm_get(&fm, "doc"))
        .or_else(|| fm_get(&fm, "domain"))
        .unwrap_or_else(|| stem_slug(&src));
    validate_slug(&doc)?;

    // Normalize the required frontmatter in place, preserving every other line
    // (incl. multi-line YAML values and unknown keys) verbatim.
    let renamed_fm = rename_key(fm, "domain", "doc");
    // An empty `renamed_fm` means there was no frontmatter block at all — don't
    // manufacture one with a blank line; let `upsert_frontmatter_key` create it.
    let renamed = if renamed_fm.is_empty() {
        content.clone()
    } else {
        format!("---\n{}\n---\n{body}", renamed_fm.join("\n"))
    };
    let rebuilt = l3::upsert_frontmatter_key(&renamed, "doc", &doc);

    let dst = doc_path(root, &doc);
    if dst.exists() {
        return Err(format!(
            "doc already exists: {} (remove it first or pick a different --doc)",
            dst.display()
        )
        .into());
    }
    fs::write(&dst, rebuilt)?;
    if mv {
        fs::remove_file(&src).map_err(|e| format!("imported, but failed to remove source: {e}"))?;
    }
    reindex(root)?;
    println!("{}", dst.display());
    eprintln!("imported: doc={doc} (from {})", src.display());
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

fn cmd_assign_ids(root: &Path, dry_run: bool) -> Result<(), DynErr> {
    let assigned = l3::assign_ids(root, dry_run)?;
    for a in &assigned {
        eprintln!("{}\t{}\t{}\t{}", a.doc, a.heading_line, a.id, a.title);
    }
    if dry_run {
        eprintln!("{} anchor(s) would be assigned (dry run)", assigned.len());
    } else {
        eprintln!("{} anchor(s) assigned", assigned.len());
    }
    Ok(())
}

/// The blessed `reading.role` order (start-here → core → rigor → reference); an
/// unknown role sorts after, grouped under its own literal value.
const BLESSED_ROLES: &[&str] = &["start-here", "core", "rigor", "reference"];

struct ReadingRow {
    doc: String,
    role: String,
    why: String,
    work_id: Option<String>,
}

fn cmd_reading_list(root: &Path, opts: &OutputOpts) -> Result<(), DynErr> {
    let (graph, _warnings) = l3::parse(root);

    let mut rows: Vec<ReadingRow> = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let reading = node.properties.get("reading")?;
            let role = reading.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let why = reading.get("why").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let work_id = node.id.as_ref().and_then(|id| {
                graph.links_touching(&id.0).iter().find_map(|l| {
                    if l.kind != "catalog" {
                        return None;
                    }
                    match (&l.source, &l.target) {
                        (l3::Endpoint::Node(src), l3::Endpoint::Catalog(dst)) if src.0 == id.0 => {
                            Some(format!("{}:{}", dst.namespace, dst.value))
                        }
                        (l3::Endpoint::Catalog(src), l3::Endpoint::Node(dst)) if dst.0 == id.0 => {
                            Some(format!("{}:{}", src.namespace, src.value))
                        }
                        _ => None,
                    }
                })
            });
            Some(ReadingRow { doc: node.provenance.doc.clone(), role, why, work_id })
        })
        .collect();

    let role_rank = |role: &str| BLESSED_ROLES.iter().position(|r| *r == role).unwrap_or(BLESSED_ROLES.len());
    rows.sort_by(|a, b| {
        role_rank(&a.role).cmp(&role_rank(&b.role)).then_with(|| a.role.cmp(&b.role)).then_with(|| a.doc.cmp(&b.doc))
    });

    if opts.text && !opts.json {
        let mut last_role: Option<&str> = None;
        for r in &rows {
            let role_label = if r.role.is_empty() { "(no role)" } else { r.role.as_str() };
            if last_role != Some(role_label) {
                println!("== {role_label} ==");
                last_role = Some(role_label);
            }
            println!("{}\t{}\t{}", r.doc, r.work_id.as_deref().unwrap_or("—"), r.why);
        }
        eprintln!("{} reading(s)", rows.len());
        return Ok(());
    }

    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "doc": r.doc,
                "role": r.role,
                "why": r.why,
                "work_id": r.work_id,
            })
        })
        .collect();
    let count = results.len() as u64;
    let envelope = Envelope {
        query: QueryMeta { entity: Some("l3:reading-list".to_string()), resolved_filter: None, url: None },
        count,
        returned: results.len(),
        truncated: false,
        next_cursor: None,
        results,
    };
    render(&envelope, opts);
    Ok(())
}

// ── sync (push / pull) ──────────────────────────────────────────────────────────

const SYNC_STATE_FILE: &str = "_sync-state.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SyncEntry {
    hash: String,
}

/// What a doc's push/pull relationship is, given local hash `L`, remote hash
/// `R`, and the hash recorded at the last successful sync `S`. Pure over
/// hashes so it is unit-testable without touching the filesystem or network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    InSync,
    Push,
    Pull,
    Conflict,
}

fn classify(local: Option<&str>, remote: Option<&str>, recorded: Option<&str>) -> SyncAction {
    match (local, remote) {
        (None, None) => SyncAction::InSync,
        (Some(_), None) => SyncAction::Push,
        (None, Some(_)) => SyncAction::Pull,
        (Some(l), Some(r)) if l == r => SyncAction::InSync,
        (Some(l), Some(r)) => match (recorded == Some(l), recorded == Some(r)) {
            (false, true) => SyncAction::Push,
            (true, false) => SyncAction::Pull,
            _ => SyncAction::Conflict,
        },
    }
}

fn sync_state_path(root: &Path) -> PathBuf {
    root.join(SYNC_STATE_FILE)
}

fn read_sync_state(root: &Path) -> BTreeMap<String, SyncEntry> {
    fs::read_to_string(sync_state_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_sync_state(root: &Path, state: &BTreeMap<String, SyncEntry>) -> Result<(), DynErr> {
    let json = serde_json::to_string_pretty(state)?;
    fs::write(sync_state_path(root), json)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// One doc's content on each side, gathered up front so classification and the
/// actual transfer both read the same snapshot.
struct DocSides {
    slug: String,
    local: Option<String>,
    remote: Option<String>,
}

/// Gather local/remote content for either the one named doc, or every doc known
/// on either side (the union of local `*.l3.md` stems and remote slugs).
fn collect_sides(root: &Path, store: &StoreClient, doc: Option<&str>) -> Result<Vec<DocSides>, DynErr> {
    if let Some(slug) = doc {
        let path = doc_path(root, slug);
        let local = if path.exists() { Some(fs::read_to_string(&path)?) } else { None };
        let remote = store.l3_get(slug)?;
        if local.is_none() && remote.is_none() {
            return Err(format!("no such doc: {slug} (not found locally or on the remote)").into());
        }
        return Ok(vec![DocSides { slug: slug.to_string(), local, remote }]);
    }

    let local_docs = scan(root)?;
    let remote_docs = store.l3_list()?;
    let mut slugs: BTreeSet<String> = BTreeSet::new();
    slugs.extend(local_docs.iter().map(|d| d.doc.clone()));
    slugs.extend(remote_docs.iter().map(|d| d.doc.clone()));

    let mut out = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let path = doc_path(root, &slug);
        let local = if path.exists() { Some(fs::read_to_string(&path)?) } else { None };
        let remote = store.l3_get(&slug)?;
        out.push(DocSides { slug, local, remote });
    }
    Ok(out)
}

fn format_put_error(slug: &str, err: &L3PutError) -> String {
    match err {
        L3PutError::Warnings(warnings) => {
            let lines: Vec<String> =
                warnings.iter().map(|w| format!("line {}: {}", w.line, w.message)).collect();
            format!("{slug} rejected (use --force to bypass):\n  {}", lines.join("\n  "))
        }
        L3PutError::Conflicts(ids) => {
            format!("{slug} rejected: anchor(s) already used elsewhere: {}", ids.join(", "))
        }
        L3PutError::Client(e) => format!("{slug}: {e}"),
    }
}

/// One agent context file's content on each side, and the sync-state key it's
/// tracked under (`agent:<name>`, distinct from a doc slug's own state key).
struct AgentSides {
    name: String,
    local: Option<String>,
    remote: Option<String>,
}

fn agent_state_key(name: &str) -> String {
    format!("agent:{name}")
}

fn agent_path(root: &Path, name: &str) -> PathBuf {
    root.join("_agent").join(format!("{name}.md"))
}

/// Gather local/remote content for every agent context file known on either
/// side (the union of local `_agent/*.md` stems and remote names). Only
/// called for whole-store push/pull — the agent-file namespace has no
/// single-name selector on the CLI surface.
fn collect_agent_sides(root: &Path, store: &StoreClient) -> Result<Vec<AgentSides>, DynErr> {
    let agent_dir = root.join("_agent");
    let mut names: BTreeSet<String> = BTreeSet::new();
    if agent_dir.exists() {
        for entry in fs::read_dir(&agent_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if let Some(stem) = file_name.strip_suffix(".md") {
                names.insert(stem.to_string());
            }
        }
    }
    let remote_files = store.l3_agent_list()?;
    names.extend(remote_files.into_iter().map(|f| f.name));

    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let path = agent_path(root, &name);
        let local = if path.exists() { Some(fs::read_to_string(&path)?) } else { None };
        let remote = store.l3_agent_get(&name)?;
        out.push(AgentSides { name, local, remote });
    }
    Ok(out)
}

fn cmd_push(
    root: &Path,
    store: &StoreClient,
    doc: Option<String>,
    dry_run: bool,
    force: bool,
) -> Result<(), DynErr> {
    let named = doc.is_some();
    let sides = collect_sides(root, store, doc.as_deref())?;
    let mut state = read_sync_state(root);

    let mut pushed = 0usize;
    let mut skipped = 0usize;
    let mut conflicts = 0usize;
    let mut errors = 0usize;

    for side in &sides {
        let recorded = state.get(&side.slug).map(|e| e.hash.as_str());
        let local_hash = side.local.as_deref().map(|s| sha256_hex(s.as_bytes()));
        let remote_hash = side.remote.as_deref().map(|s| sha256_hex(s.as_bytes()));
        let action = classify(local_hash.as_deref(), remote_hash.as_deref(), recorded);

        if action == SyncAction::Conflict {
            eprintln!("CONFLICT {} (changed on both sides)", side.slug);
            conflicts += 1;
            continue;
        }

        let should_push = named || action == SyncAction::Push;
        if !should_push {
            eprintln!("skip {} (in sync)", side.slug);
            skipped += 1;
            continue;
        }

        let Some(local_content) = &side.local else {
            eprintln!("skip {} (no local copy to push)", side.slug);
            skipped += 1;
            continue;
        };

        eprintln!("push {}", side.slug);
        if dry_run {
            pushed += 1;
            continue;
        }

        match store.l3_put(&side.slug, local_content, force) {
            Ok(normalized) => {
                fs::write(doc_path(root, &side.slug), &normalized)?;
                let hash = sha256_hex(normalized.as_bytes());
                state.insert(side.slug.clone(), SyncEntry { hash });
                pushed += 1;
            }
            Err(e) => {
                eprintln!("error: {}", format_put_error(&side.slug, &e));
                errors += 1;
            }
        }
    }

    if !named {
        for side in collect_agent_sides(root, store)? {
            let key = agent_state_key(&side.name);
            let recorded = state.get(&key).map(|e| e.hash.as_str());
            let local_hash = side.local.as_deref().map(|s| sha256_hex(s.as_bytes()));
            let remote_hash = side.remote.as_deref().map(|s| sha256_hex(s.as_bytes()));
            let action = classify(local_hash.as_deref(), remote_hash.as_deref(), recorded);

            if action == SyncAction::Conflict {
                eprintln!("CONFLICT agent:{} (changed on both sides)", side.name);
                conflicts += 1;
                continue;
            }
            if action != SyncAction::Push {
                eprintln!("skip agent:{} (in sync)", side.name);
                skipped += 1;
                continue;
            }
            let Some(local_content) = &side.local else {
                eprintln!("skip agent:{} (no local copy to push)", side.name);
                skipped += 1;
                continue;
            };

            eprintln!("push agent:{}", side.name);
            if dry_run {
                pushed += 1;
                continue;
            }

            match store.l3_agent_put(&side.name, local_content) {
                Ok(()) => {
                    let hash = sha256_hex(local_content.as_bytes());
                    state.insert(key, SyncEntry { hash });
                    pushed += 1;
                }
                Err(e) => {
                    eprintln!("error: agent:{}: {e}", side.name);
                    errors += 1;
                }
            }
        }
    }

    if !dry_run {
        write_sync_state(root, &state)?;
    }

    eprintln!("{pushed} pushed, {skipped} skipped, {conflicts} conflict(s), {errors} error(s)");
    if conflicts > 0 || errors > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_pull(root: &Path, store: &StoreClient, doc: Option<String>, dry_run: bool) -> Result<(), DynErr> {
    let named = doc.is_some();
    let sides = collect_sides(root, store, doc.as_deref())?;
    let mut state = read_sync_state(root);

    let mut pulled = 0usize;
    let mut skipped = 0usize;
    let mut conflicts = 0usize;
    let mut wrote_any = false;

    for side in &sides {
        let recorded = state.get(&side.slug).map(|e| e.hash.as_str());
        let local_hash = side.local.as_deref().map(|s| sha256_hex(s.as_bytes()));
        let remote_hash = side.remote.as_deref().map(|s| sha256_hex(s.as_bytes()));
        let action = classify(local_hash.as_deref(), remote_hash.as_deref(), recorded);

        if action == SyncAction::Conflict {
            eprintln!("CONFLICT {} (changed on both sides)", side.slug);
            conflicts += 1;
            continue;
        }

        let should_pull = named || action == SyncAction::Pull;
        if !should_pull {
            eprintln!("skip {} (in sync)", side.slug);
            skipped += 1;
            continue;
        }

        let Some(remote_content) = &side.remote else {
            eprintln!("skip {} (no remote copy to pull)", side.slug);
            skipped += 1;
            continue;
        };

        eprintln!("pull {}", side.slug);
        if dry_run {
            pulled += 1;
            continue;
        }

        fs::write(doc_path(root, &side.slug), remote_content)?;
        let hash = sha256_hex(remote_content.as_bytes());
        state.insert(side.slug.clone(), SyncEntry { hash });
        pulled += 1;
        wrote_any = true;
    }

    if !named {
        for side in collect_agent_sides(root, store)? {
            let key = agent_state_key(&side.name);
            let recorded = state.get(&key).map(|e| e.hash.as_str());
            let local_hash = side.local.as_deref().map(|s| sha256_hex(s.as_bytes()));
            let remote_hash = side.remote.as_deref().map(|s| sha256_hex(s.as_bytes()));
            let action = classify(local_hash.as_deref(), remote_hash.as_deref(), recorded);

            if action == SyncAction::Conflict {
                eprintln!("CONFLICT agent:{} (changed on both sides)", side.name);
                conflicts += 1;
                continue;
            }
            if action != SyncAction::Pull {
                eprintln!("skip agent:{} (in sync)", side.name);
                skipped += 1;
                continue;
            }
            let Some(remote_content) = &side.remote else {
                eprintln!("skip agent:{} (no remote copy to pull)", side.name);
                skipped += 1;
                continue;
            };

            eprintln!("pull agent:{}", side.name);
            if dry_run {
                pulled += 1;
                continue;
            }

            let path = agent_path(root, &side.name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, remote_content)?;
            let hash = sha256_hex(remote_content.as_bytes());
            state.insert(key, SyncEntry { hash });
            pulled += 1;
            wrote_any = true;
        }
    }

    if !dry_run {
        write_sync_state(root, &state)?;
        if wrote_any {
            reindex(root)?;
        }
    }

    eprintln!("{pulled} pulled, {skipped} skipped, {conflicts} conflict(s)");
    if conflicts > 0 {
        std::process::exit(1);
    }
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

/// Read every `*.l3.md` (skipping `_`-prefixed helpers) as a `DocumentMetadata`, sorted by slug.
fn scan(root: &Path) -> Result<Vec<DocumentMetadata>, DynErr> {
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

fn read_meta(path: &Path) -> Result<DocumentMetadata, DynErr> {
    let content = fs::read_to_string(path)?;
    let (fm, _body) = split_frontmatter(&content);
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(FILE_SUFFIX).to_string())
        .unwrap_or_default();
    Ok(DocumentMetadata {
        doc: fm_get(&fm, "doc").unwrap_or(stem),
        modified: modified_date(path),
        path: path.to_path_buf(),
    })
}

/// A doc's title: the first parsed node's title property, falling back to the
/// body's first H1 for docs the parser lifted zero nodes from.
fn doc_title(graph: &l3::Graph, doc: &DocumentMetadata) -> String {
    if let Some(node) = graph.nodes.iter().find(|n| n.provenance.doc == doc.doc) {
        return node.properties.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    }
    fs::read_to_string(&doc.path)
        .ok()
        .and_then(|content| first_h1(&split_frontmatter(&content).1))
        .unwrap_or_default()
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
/// Three sections, each derived from the parsed research graph (no RAG, no embeddings):
///   1. **Documents** — the frontmatter table (doc · modified · title).
///   2. **Cross-references** — the doc→doc link graph: any node link whose endpoints span
///      two docs, plus doc-level forward references, with back-references computed so
///      "what refers to this doc" is a lookup, not a grep.
///   3. **Work index** — the inverted index `canonical id → docs that reference it`. Keyed on
///      the work, not the doc, so it survives a doc splitting into pieces (the ids travel).
///
/// Returns the index path.
fn reindex(root: &Path) -> Result<PathBuf, DynErr> {
    ensure_root(root)?;
    let docs = scan(root)?;
    let known: BTreeSet<&str> = docs.iter().map(|d| d.doc.as_str()).collect();
    let (graph, _warnings) = l3::parse(root);

    // doc → docs it links out to, and the reverse (known targets only; a dangling
    // target still renders as an out-edge, just without a matching backref).
    let mut outrefs: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut backrefs: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    // work-reference alias string (ns:value) → docs referencing it
    let mut work_index: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();

    for link in &graph.links {
        let Some(recorded_in) = &link.recorded_in else { continue };
        let Some(source_doc) = graph.node_by_id(&recorded_in.0).map(|n| n.provenance.doc.as_str()) else {
            continue;
        };

        for endpoint in [&link.source, &link.target] {
            match endpoint {
                l3::Endpoint::Catalog(id) => {
                    work_index.entry(format!("{}:{}", id.namespace, id.value)).or_default().insert(source_doc);
                }
                l3::Endpoint::Node(id) => {
                    let target_doc = if let Some(slug) = id.0.strip_prefix("doc:") {
                        Some(slug.to_string())
                    } else {
                        // A bare anchor is store-global; resolve it to the doc that holds it.
                        graph.node_by_id(&id.0).map(|n| n.provenance.doc.clone())
                    };
                    if let Some(target_doc) = target_doc {
                        if target_doc != source_doc {
                            outrefs.entry(source_doc).or_default().insert(target_doc.clone());
                            if known.contains(target_doc.as_str()) {
                                backrefs.entry(target_doc).or_default().insert(source_doc);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut out = String::from("# L3 store index\n\n");
    out.push_str(&format!(
        "{} doc(s). Generated by `braincrawl collection index` — do not edit by hand.\n\n",
        docs.len()
    ));

    out.push_str("## Documents\n\n");
    out.push_str("| doc | modified | title |\n|---|---|---|\n");
    for d in &docs {
        out.push_str(&format!(
            "| [{}]({}{}) | {} | {} |\n",
            d.doc, d.doc, FILE_SUFFIX, d.modified, doc_title(&graph, d).replace('|', "\\|")
        ));
    }

    out.push_str("\n## Cross-references\n\n");
    out.push_str("Doc-to-doc links between research nodes, plus doc-level forward references. A dangling target (no such doc) is marked `?`.\n\n");
    out.push_str("| doc | → links out | ← linked from |\n|---|---|---|\n");
    for d in &docs {
        let out_links: Vec<String> = outrefs
            .get(d.doc.as_str())
            .map(|s| {
                s.iter()
                    .map(|l| if known.contains(l.as_str()) { format!("[{l}]({l}{FILE_SUFFIX})") } else { format!("{l}?") })
                    .collect()
            })
            .unwrap_or_default();
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
    out.push_str("Work references (`openalex:`/`doi:`/`isbn:`/…/`uuid:`) → docs that reference them.\n\n");
    out.push_str("| id | docs |\n|---|---|\n");
    for (id, slugs) in &work_index {
        let cells: Vec<String> = slugs.iter().map(|s| format!("[{s}]({s}{FILE_SUFFIX})")).collect();
        out.push_str(&format!("| `{id}` | {} |\n", cells.join(", ")));
    }

    let path = root.join("INDEX.md");
    fs::write(&path, out)?;
    Ok(path)
}

/// Advisory lint: required frontmatter present + the one body invariant (reference, not copy).
fn lint(path: &Path, content: &str, has_catalog_ref: bool) -> Vec<String> {
    let mut warns = Vec::new();
    let (fm, _body) = split_frontmatter(content);
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
    if !has_catalog_ref {
        warns.push("no catalog reference (any ns:value, e.g. openalex:/doi:/isbn:) — L3 is annotation-over-reference".to_string());
    }
    warns
}

// ── scaffolding ───────────────────────────────────────────────────────────────

fn scaffold(doc: &str, title: &str) -> String {
    let header = format!("---\ndoc: {doc}\n---\n\n# L3 — {title}\n\n");
    format!("{header}{NODE_BODY}")
}

/// Every doc gets the same starter — the node grammar is the one body format.
/// Headings carry no `^r-…` anchor — `collection assign-ids` assigns one on first run,
/// so node identity stays tooling-owned.
const NODE_BODY: &str = "\
<!-- Headings below carry no ^r-… anchor — `collection assign-ids` assigns one on first run. -->

## About this document
- tags: #meta
- remarks: One line on what this collection is about and what question set it collects around.

## Example question
- tags: #Q1
- remarks: Replace with the actual question; add findings, links, and catalog references as you go.
<!-- - contradicts [[^r-anchor]] {why: 'briefly why'}  (a ^r-… anchor names a node in any doc) -->
<!-- - catalog [[openalex:W…]] {why: 'briefly why'} -->
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
/// (`key: >`) yield an empty/placeholder value, which is fine for frontmatter keys.
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
        let renamed = format!("---\n{}\n---\n{body}", fm.join("\n"));
        let content = l3::upsert_frontmatter_key(&renamed, "kind", "research");
        let (fm, body) = split_frontmatter(&content);
        assert_eq!(fm_get(&fm, "doc").as_deref(), Some("foo-bar"));
        assert_eq!(fm_get(&fm, "kind").as_deref(), Some("research"));
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
    fn lint_flags_missing_metadata_and_copy() {
        let warns = lint(Path::new("/x/foo.l3.md"), "# just a body\n", false);
        assert!(warns.iter().any(|w| w.contains("frontmatter")));
    }

    #[test]
    fn lint_flags_missing_catalog_ref() {
        let warns = lint(Path::new("/x/foo.l3.md"), "---\ndoc: foo\n---\n\nbody\n", false);
        assert!(warns.iter().any(|w| w.contains("no catalog reference")));
    }

    #[test]
    fn lint_accepts_catalog_ref_when_present() {
        let warns = lint(Path::new("/x/foo.l3.md"), "---\ndoc: foo\n---\n\nbody\n", true);
        assert!(!warns.iter().any(|w| w.contains("no catalog reference")));
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("l3-cli-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reindex_derives_documents_and_title_from_the_graph() {
        let root = temp_root("documents");
        fs::write(
            root.join("a.l3.md"),
            "---\ndoc: a\nupdated: 2026-07-01\n---\n\n## First node title ^r-aaa1\n- tags: #x\n",
        )
        .unwrap();
        reindex(&root).unwrap();
        let index = fs::read_to_string(root.join("INDEX.md")).unwrap();
        assert!(index.contains("| [a](a.l3.md) | "));
        assert!(index.contains("First node title"));
    }

    #[test]
    fn reindex_cross_references_span_two_docs() {
        let root = temp_root("cross-refs");
        fs::write(
            root.join("a.l3.md"),
            "---\ndoc: a\n---\n\n## Node in A ^r-aaa1\n- contradicts [[^r-bbb1]] {why: scope}\n",
        )
        .unwrap();
        fs::write(root.join("b.l3.md"), "---\ndoc: b\n---\n\n## Node in B ^r-bbb1\n- tags: #y\n").unwrap();
        reindex(&root).unwrap();
        let index = fs::read_to_string(root.join("INDEX.md")).unwrap();
        assert!(index.contains("| a | [b](b.l3.md) | — |\n"));
        assert!(index.contains("| b | — | [a](a.l3.md) |\n"));
    }

    #[test]
    fn reindex_work_index_from_catalog_links() {
        let root = temp_root("work-index");
        fs::write(
            root.join("a.l3.md"),
            "---\ndoc: a\n---\n\n## Node in A ^r-aaa1\n- catalog [[openalex:W1]] {why: 'cites'}\n",
        )
        .unwrap();
        reindex(&root).unwrap();
        let index = fs::read_to_string(root.join("INDEX.md")).unwrap();
        assert!(index.contains("| `openalex:W1` | [a](a.l3.md) |\n"));
    }

    #[test]
    fn classify_in_sync_when_local_equals_remote() {
        assert_eq!(classify(Some("h"), Some("h"), None), SyncAction::InSync);
        assert_eq!(classify(Some("h"), Some("h"), Some("stale")), SyncAction::InSync);
    }

    #[test]
    fn classify_push_when_only_local_changed() {
        assert_eq!(classify(Some("l"), Some("s"), Some("s")), SyncAction::Push);
    }

    #[test]
    fn classify_pull_when_only_remote_changed() {
        assert_eq!(classify(Some("s"), Some("r"), Some("s")), SyncAction::Pull);
    }

    #[test]
    fn classify_conflict_when_both_sides_changed() {
        assert_eq!(classify(Some("l"), Some("r"), Some("s")), SyncAction::Conflict);
    }

    #[test]
    fn classify_conflict_when_never_synced_and_sides_differ() {
        assert_eq!(classify(Some("l"), Some("r"), None), SyncAction::Conflict);
    }

    #[test]
    fn classify_push_when_missing_on_remote() {
        assert_eq!(classify(Some("l"), None, None), SyncAction::Push);
        assert_eq!(classify(Some("l"), None, Some("s")), SyncAction::Push);
    }

    #[test]
    fn classify_pull_when_missing_locally() {
        assert_eq!(classify(None, Some("r"), None), SyncAction::Pull);
        assert_eq!(classify(None, Some("r"), Some("s")), SyncAction::Pull);
    }

    #[test]
    fn classify_in_sync_when_missing_on_both_sides() {
        assert_eq!(classify(None, None, None), SyncAction::InSync);
    }

    #[test]
    fn scaffold_emits_anchor_less_node_grammar() {
        let content = scaffold("some-doc", "Some Doc");
        assert!(content.contains("## About this document"));
        assert!(content.contains("## Example question"));
        assert!(content.lines().filter(|l| l.starts_with("## ")).all(|l| !l.contains('^')));
        assert!(content.contains("assign-ids"));
    }
}

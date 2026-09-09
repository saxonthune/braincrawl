use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alias {
    pub scheme: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Work,
    Author,
    Venue,
    Concept,
    Topic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArtifactRole {
    Abstract,
    Fulltext,
    Other(String),
}

impl ArtifactRole {
    pub fn as_str(&self) -> &str {
        match self {
            ArtifactRole::Abstract => "abstract",
            ArtifactRole::Fulltext => "fulltext",
            ArtifactRole::Other(s) => s.as_str(),
        }
    }

    /// Parse a role string. Accepts `"abstract"`, `"fulltext"`, or any non-empty
    /// slug matching `[a-z0-9_-]+`. Returns `None` for empty, uppercase, or unsafe
    /// strings (slashes, dots, whitespace) — these would break blob-key paths.
    pub fn parse(s: &str) -> Option<ArtifactRole> {
        match s {
            "abstract" => Some(ArtifactRole::Abstract),
            "fulltext" => Some(ArtifactRole::Fulltext),
            other => {
                if !other.is_empty() && other.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-')) {
                    Some(ArtifactRole::Other(other.to_string()))
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct StoredBlob {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub content_hash: String,
}

#[derive(Clone)]
pub struct Artifact {
    pub canonical_id: CanonicalId,
    pub role: ArtifactRole,
    pub version: u32,
    pub r2_key: String,
    pub content_hash: String,
    pub byte_size: u64,
    pub mime: String,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub fetched_at: String,
    pub is_current: bool,
    /// The (role, version) this artifact was produced from; `None` for a root
    /// artifact (a user-supplied upload or an original fetch — nothing derived it).
    pub derived_from: Option<(ArtifactRole, u32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkRecord {
    pub source: String,
    pub kind: NodeKind,
    pub aliases: Vec<Alias>,
    pub attrs: serde_json::Value,
    /// Provenance timestamp for the assertion; `None` means "stamp with the
    /// store's own now". Sync replay passes the original timestamp so a copy
    /// between stores does not rewrite provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

/// Store-side work search. Every set field must match (AND). `author` and
/// `title` are case-insensitive substring matches against any assertion;
/// `year` matches `publication_year` exactly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkSearchFilter {
    pub author: Option<String>,
    pub title: Option<String>,
    pub year: Option<u32>,
}

impl WorkSearchFilter {
    pub fn is_empty(&self) -> bool {
        self.author.is_none() && self.title.is_none() && self.year.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkView {
    pub canonical_id: CanonicalId,
    pub kind: NodeKind,
    pub attrs: serde_json::Value,
    pub provenance: serde_json::Value,
    pub aliases: Vec<Alias>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeInput {
    pub src: Alias,
    pub dst: Alias,
    pub relation: String,
    pub source: String,
    pub attrs: Option<serde_json::Value>,
    pub fetched_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeView {
    pub src: CanonicalId,
    pub dst: CanonicalId,
    pub relation: String,
    pub assertions: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodNode {
    pub canonical_id: CanonicalId,
    pub kind: NodeKind,
    /// BFS distance from the nearest seed (seeds = 0).
    pub depth: u32,
    /// Count of discovered edges (both endpoints included) whose `dst == this node`.
    pub in_degree: u32,
    /// Merged node attrs (same merge as get_work); `{}` for stubs.
    pub attrs: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodEdge {
    pub src: CanonicalId,
    pub dst: CanonicalId,
    pub relation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Neighborhood {
    /// Sorted: in_degree desc, then canonical_id asc.
    pub nodes: Vec<NeighborhoodNode>,
    /// Only edges where both endpoints are included nodes.
    pub edges: Vec<NeighborhoodEdge>,
    /// True if max_nodes capped the traversal.
    pub truncated: bool,
}

/// One provider assertion carried by an [`ExportNode`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportAssertion {
    pub source: String,
    pub attrs: serde_json::Value,
    pub fetched_at: String,
}

/// One live node with everything needed to replay it into another store:
/// aliases (identity) and per-provider assertions (metadata + provenance).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportNode {
    pub canonical_id: CanonicalId,
    pub kind: NodeKind,
    pub aliases: Vec<Alias>,
    pub assertions: Vec<ExportAssertion>,
}

/// One edge assertion, endpoints as canonical ids. A sync client maps the ids
/// to aliases using the node export's alias sets before replaying.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportEdgeAssertion {
    pub src_id: CanonicalId,
    pub dst_id: CanonicalId,
    pub relation: String,
    pub source: String,
    pub attrs: Option<serde_json::Value>,
    pub fetched_at: String,
}

/// A single named count, e.g. `{ key: "work", count: 42 }`.
/// Used for the grouped breakdowns in [`GraphStats`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub key: String,
    pub count: u64,
}

/// Aggregate counts over the whole metadata network.
///
/// "Live" everywhere means `merged_into IS NULL` — merge redirects are excluded
/// from the headline counts and reported separately.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphStats {
    /// Live work nodes — the headline "how many works" number.
    pub works: u64,
    /// Live works with at least one assertion (fetched metadata).
    pub works_described: u64,
    /// Live works with zero assertions — cited-only frontier stubs.
    pub works_stub: u64,
    /// All live nodes regardless of kind.
    pub nodes_total: u64,
    /// Live node counts by kind, descending by count.
    pub nodes_by_kind: Vec<Tally>,
    /// Merge redirects (nodes merged into a survivor).
    pub merge_redirects: u64,
    /// Total deduped edges.
    pub edges_total: u64,
    /// Edge counts by relation, descending by count.
    pub edges_by_relation: Vec<Tally>,
    /// Node-assertion counts by provider source, descending by count.
    pub assertions_by_source: Vec<Tally>,
    /// Stored artifact content bytes — the blob store / R2 footprint (`SUM(byte_size)`).
    pub library_bytes: u64,
    /// On-disk size of the metadata database. 0 for backends with no on-disk file.
    pub catalog_bytes: u64,
    /// `library_bytes + catalog_bytes` — the corpus's total storage footprint.
    pub total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EdgeDir {
    Forward,
    Backward,
}

/// The redirect network a work delete will remove, computed without mutating.
/// `network` is the live node plus every merge-redirect node forwarding into it;
/// the counts and `r2_keys` cover everything those nodes own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletePlan {
    pub network: Vec<CanonicalId>,
    pub alias_count: u64,
    pub artifact_count: u64,
    pub edge_count: u64,
    /// Blob keys the deleted artifacts point at; the use-case removes them from
    /// the blob store after the metadata transaction commits.
    pub r2_keys: Vec<String>,
}

/// What a work delete did (or, when `dry_run`, would do). `redirect_nodes` is the
/// merge-redirect count folded into the delete — the network size minus the live
/// node. `blob_orphans` are blob keys whose bytes failed to delete after the SQL
/// commit; the rows are gone, so these are swept later, not a failed delete.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeleteReport {
    pub work_id: String,
    pub redirect_nodes: u64,
    pub aliases: u64,
    pub artifacts: u64,
    pub edges: u64,
    pub blobs_deleted: u64,
    pub blob_orphans: Vec<String>,
    pub dry_run: bool,
}

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("backend error: {0}")]
    Backend(String),
    #[error("serde error: {0}")]
    Serde(String),
}

// ── Job types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JobKind {
    Fulltext,
    Refs,
}

impl JobKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobKind::Fulltext => "fulltext",
            JobKind::Refs => "refs",
        }
    }
}

impl FromStr for JobKind {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fulltext" => Ok(JobKind::Fulltext),
            "refs" => Ok(JobKind::Refs),
            other => Err(DomainError::Backend(format!("unknown JobKind: {other}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
}

/// What a producer hands to enqueue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobSpec {
    pub kind: JobKind,
    pub target_id: String,
    pub params: serde_json::Value,
}

/// What the worker receives on claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub target_id: String,
    pub params: serde_json::Value,
    pub attempts: u32,
}

/// Outcome of `get_content`, mirroring HTTP status semantics.
///
/// - `Bytes`   → 200 OK (bytes returned)
/// - `Pending` → 202 Accepted (descriptor exists; blob not yet stored)
/// - `Absent`  → 404 Not Found
pub enum ContentOutcome {
    Bytes {
        bytes: Vec<u8>,
        mime: String,
        content_hash: String,
    },
    Pending,
    Absent,
}

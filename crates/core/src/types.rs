use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alias {
    pub namespace: String,
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
pub enum PayloadKind {
    Abstract,
    Fulltext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Rights {
    Open,
    LinkOnly,
    Restricted,
}

#[derive(Clone)]
pub struct StoredBlob {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub content_hash: String,
}

#[derive(Clone)]
pub struct PayloadDescriptor {
    pub canonical_id: CanonicalId,
    pub kind: PayloadKind,
    pub version: u32,
    pub r2_key: String,
    pub content_hash: String,
    pub byte_size: u64,
    pub mime: String,
    pub rights: Rights,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub fetched_at: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkRecord {
    pub source: String,
    pub kind: NodeKind,
    pub aliases: Vec<Alias>,
    pub attrs: serde_json::Value,
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

/// A single named count, e.g. `{ key: "work", count: 42 }`.
/// Used for the grouped breakdowns in [`GraphStats`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub key: String,
    pub count: u64,
}

/// Aggregate counts over the whole metadata network.
///
/// "Live" everywhere means `merged_into IS NULL` — tombstones are excluded
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
    /// Tombstones (nodes merged into a survivor).
    pub tombstones: u64,
    /// Total deduped edges.
    pub edges_total: u64,
    /// Edge counts by relation, descending by count.
    pub edges_by_relation: Vec<Tally>,
    /// Node-assertion counts by provider source, descending by count.
    pub assertions_by_source: Vec<Tally>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EdgeDir {
    Forward,
    Backward,
}

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("rights violation: {0:?}")]
    RightsViolation(Rights),
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
/// - `Bytes`       → 200 OK (open content, bytes returned)
/// - `RedirectUrl` → 302 Found (link_only; redirect to source URL)
/// - `Pending`     → 202 Accepted (descriptor exists; blob not yet stored)
/// - `Restricted`  → 451 Unavailable For Legal Reasons
/// - `Absent`      → 404 Not Found
pub enum ContentOutcome {
    Bytes {
        bytes: Vec<u8>,
        mime: String,
        content_hash: String,
    },
    RedirectUrl(String),
    Pending,
    Restricted,
    Absent,
}

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

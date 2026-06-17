pub mod client;
pub mod entity;
pub mod mapping;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::semanticscholar::entity::Entity;

#[derive(Debug, Error)]
pub enum SemanticScholarError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("cannot infer entity from ID: {0}")]
    InferFailed(String),
    #[error("unknown entity: {0}")]
    UnknownEntity(String),
}

pub type Result<T> = std::result::Result<T, SemanticScholarError>;

/// Raw records and citation pairs returned by each verb.
pub struct PushBatch {
    /// (entity_type, raw_record) pairs fetched from Semantic Scholar
    pub records: Vec<(Entity, serde_json::Value)>,
    /// Citation edges as (citing_alias, cited_alias) in ns:value form
    pub edges: Vec<(String, String)>,
}

impl PushBatch {
    pub fn empty() -> Self {
        PushBatch { records: Vec::new(), edges: Vec::new() }
    }
}

/// Counts from a push operation; reported to stderr.
pub struct PushSummary {
    pub nodes_pushed: usize,
    pub edges_pushed: u64,
    pub skipped_unmappable: usize,
    pub errors: Vec<String>,
}

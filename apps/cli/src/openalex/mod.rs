pub mod client;
pub mod entity;
pub mod filters;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::openalex::entity::Entity;

#[derive(Debug, Error)]
pub enum OpenAlexError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("unknown entity: {0}")]
    UnknownEntity(String),
    #[error("unknown filter key(s) for {entity}: {keys}")]
    BadFilter { entity: String, keys: String },
    #[error("cannot infer entity from ID: {0}")]
    InferFailed(String),
    #[error("invalid filter expression (expected key:value): {0}")]
    BadFilterExpr(String),
}

pub type Result<T> = std::result::Result<T, OpenAlexError>;

/// Raw records and citation pairs returned by each verb.
/// This phase discards it after rendering; the push phase consumes it.
pub struct PushBatch {
    /// (entity_type, raw_record) pairs fetched from OpenAlex
    pub records: Vec<(Entity, serde_json::Value)>,
    /// Citation edges as (citing_openalex_id, cited_openalex_id) pairs
    pub edges: Vec<(String, String)>,
}

impl PushBatch {
    pub fn empty() -> Self {
        PushBatch { records: Vec::new(), edges: Vec::new() }
    }
}

pub mod client;
pub mod entity;
pub mod filters;
pub mod mapping;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::{Emission, Provider, ProviderCmd, ProviderError};

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

impl From<OpenAlexError> for ProviderError {
    fn from(e: OpenAlexError) -> Self {
        ProviderError::Other(Box::new(e))
    }
}

pub struct OpenAlexProvider {
    pub client: client::OpenAlexClient,
}

impl OpenAlexProvider {
    pub fn new(client: client::OpenAlexClient) -> Self {
        Self { client }
    }
}

impl Provider for OpenAlexProvider {
    fn name(&self) -> &'static str {
        "openalex"
    }

    fn dispatch(
        &self,
        cmd: ProviderCmd,
        opts: &OutputOpts,
    ) -> std::result::Result<(Envelope, Emission), ProviderError> {
        match cmd {
            ProviderCmd::Get { id } => {
                verbs::get(&self.client, &id, opts).map_err(Into::into)
            }
            ProviderCmd::Search { entity, query } => {
                let ent = entity.unwrap_or_else(|| "works".to_string());
                verbs::search(&self.client, &ent, &query, opts).map_err(Into::into)
            }
            ProviderCmd::Find { entity, filters } => {
                verbs::find(&self.client, &entity, &filters, opts).map_err(Into::into)
            }
            ProviderCmd::Autocomplete { entity, q } => {
                verbs::autocomplete(&self.client, &entity, &q, opts).map_err(Into::into)
            }
            ProviderCmd::CitedBy { id } => {
                verbs::cited_by(&self.client, &id, opts).map_err(Into::into)
            }
            ProviderCmd::Refs { id } => {
                verbs::refs(&self.client, &id, opts).map_err(Into::into)
            }
        }
    }
}

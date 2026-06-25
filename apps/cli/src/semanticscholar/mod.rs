pub mod client;
pub mod entity;
pub mod mapping;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::{Emission, Provider, ProviderCmd, ProviderError};

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

impl From<SemanticScholarError> for ProviderError {
    fn from(e: SemanticScholarError) -> Self {
        ProviderError::Other(Box::new(e))
    }
}

pub struct SemanticScholarProvider {
    pub client: client::SemanticScholarClient,
}

impl SemanticScholarProvider {
    pub fn new(client: client::SemanticScholarClient) -> Self {
        Self { client }
    }
}

impl Provider for SemanticScholarProvider {
    fn name(&self) -> &'static str {
        "semanticscholar"
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
                let ent = entity.unwrap_or_else(|| "papers".to_string());
                verbs::search(&self.client, &ent, &query, opts).map_err(Into::into)
            }
            ProviderCmd::CitedBy { id } => {
                verbs::cited_by(&self.client, &id, opts).map_err(Into::into)
            }
            ProviderCmd::Refs { id } => {
                verbs::refs(&self.client, &id, opts).map_err(Into::into)
            }
            ProviderCmd::Find { .. } => Err(ProviderError::UnsupportedVerb {
                provider: self.name(),
                verb: "find",
            }),
            ProviderCmd::Autocomplete { .. } => Err(ProviderError::UnsupportedVerb {
                provider: self.name(),
                verb: "autocomplete",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputOpts;
    use crate::provider::ProviderCmd;

    fn dummy_opts() -> OutputOpts {
        OutputOpts {
            json: false,
            text: false,
            limit: None,
            all: false,
            fields: vec![],
            full: false,
            skip_push: true,
            include_abstract: false,
        }
    }

    #[test]
    fn find_returns_unsupported_verb() {
        let provider = SemanticScholarProvider {
            client: client::SemanticScholarClient::new(None),
        };
        let result = provider.dispatch(
            ProviderCmd::Find {
                entity: "papers".to_string(),
                filters: vec!["year:2020".to_string()],
            },
            &dummy_opts(),
        );
        assert!(matches!(
            result,
            Err(ProviderError::UnsupportedVerb { verb: "find", .. })
        ));
    }

    #[test]
    fn autocomplete_returns_unsupported_verb() {
        let provider = SemanticScholarProvider {
            client: client::SemanticScholarClient::new(None),
        };
        let result = provider.dispatch(
            ProviderCmd::Autocomplete {
                entity: "papers".to_string(),
                q: "sal".to_string(),
            },
            &dummy_opts(),
        );
        assert!(matches!(
            result,
            Err(ProviderError::UnsupportedVerb { verb: "autocomplete", .. })
        ));
    }
}

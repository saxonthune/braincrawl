pub mod client;
pub mod entity;
pub mod mapping;
pub mod parse;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::{Emission, Provider, ProviderCmd, ProviderError};

#[derive(Debug, Error)]
pub enum ArxivError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("cannot infer arXiv id: {0}")]
    InferFailed(String),
    #[error("XML parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, ArxivError>;

impl From<ArxivError> for ProviderError {
    fn from(e: ArxivError) -> Self {
        ProviderError::Other(Box::new(e))
    }
}

pub struct ArxivProvider {
    pub client: client::ArxivClient,
}

impl Default for ArxivProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ArxivProvider {
    pub fn new() -> Self {
        Self { client: client::ArxivClient::new() }
    }
}

impl Provider for ArxivProvider {
    fn name(&self) -> &'static str {
        "arxiv"
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
            ProviderCmd::Search { query, .. } => {
                verbs::search(&self.client, &query, opts).map_err(Into::into)
            }
            ProviderCmd::Find { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "arxiv",
                verb: "find",
            }),
            ProviderCmd::Autocomplete { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "arxiv",
                verb: "autocomplete",
            }),
            ProviderCmd::CitedBy { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "arxiv",
                verb: "cited_by",
            }),
            ProviderCmd::Refs { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "arxiv",
                verb: "refs",
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
    fn cited_by_returns_unsupported() {
        let p = ArxivProvider::new();
        let r = p.dispatch(ProviderCmd::CitedBy { id: "x".into() }, &dummy_opts());
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "cited_by", .. })));
    }

    #[test]
    fn refs_returns_unsupported() {
        let p = ArxivProvider::new();
        let r = p.dispatch(ProviderCmd::Refs { id: "x".into() }, &dummy_opts());
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "refs", .. })));
    }

    #[test]
    fn find_returns_unsupported() {
        let p = ArxivProvider::new();
        let r = p.dispatch(
            ProviderCmd::Find { entity: "works".into(), filters: vec![] },
            &dummy_opts(),
        );
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "find", .. })));
    }

    #[test]
    fn autocomplete_returns_unsupported() {
        let p = ArxivProvider::new();
        let r = p.dispatch(
            ProviderCmd::Autocomplete { entity: "works".into(), q: "cs".into() },
            &dummy_opts(),
        );
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "autocomplete", .. })));
    }
}

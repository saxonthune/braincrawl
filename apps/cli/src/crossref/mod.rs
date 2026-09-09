pub mod client;
pub mod mapping;
pub mod shape;
pub mod verbs;

use thiserror::Error;

use crate::cli::OutputOpts;
use crate::output::Envelope;
use crate::provider::{Emission, Provider, ProviderCmd, ProviderError};

#[derive(Debug, Error)]
pub enum CrossrefError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, CrossrefError>;

impl From<CrossrefError> for ProviderError {
    fn from(e: CrossrefError) -> Self {
        ProviderError::Other(Box::new(e))
    }
}

pub struct CrossrefProvider {
    pub client: client::CrossrefClient,
}

impl CrossrefProvider {
    pub fn new(mailto: Option<String>) -> Self {
        Self { client: client::CrossrefClient::new(mailto) }
    }
}

impl Provider for CrossrefProvider {
    fn name(&self) -> &'static str {
        "crossref"
    }

    fn dispatch(
        &self,
        cmd: ProviderCmd,
        opts: &OutputOpts,
    ) -> std::result::Result<(Envelope, Emission), ProviderError> {
        match cmd {
            ProviderCmd::Refs { id } => {
                verbs::refs(&self.client, &id, opts).map_err(Into::into)
            }
            ProviderCmd::Get { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "crossref",
                verb: "get",
            }),
            ProviderCmd::Search { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "crossref",
                verb: "search",
            }),
            ProviderCmd::Find { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "crossref",
                verb: "find",
            }),
            ProviderCmd::Autocomplete { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "crossref",
                verb: "autocomplete",
            }),
            ProviderCmd::CitedBy { .. } => Err(ProviderError::UnsupportedVerb {
                provider: "crossref",
                verb: "cited_by",
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
            emission: false,
        }
    }

    #[test]
    fn get_returns_unsupported() {
        let p = CrossrefProvider::new(None);
        let r = p.dispatch(ProviderCmd::Get { id: "x".into() }, &dummy_opts());
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "get", .. })));
    }

    #[test]
    fn search_returns_unsupported() {
        let p = CrossrefProvider::new(None);
        let r = p.dispatch(
            ProviderCmd::Search { entity: None, query: "x".into() },
            &dummy_opts(),
        );
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "search", .. })));
    }

    #[test]
    fn find_returns_unsupported() {
        let p = CrossrefProvider::new(None);
        let r = p.dispatch(
            ProviderCmd::Find { entity: "works".into(), filters: vec![] },
            &dummy_opts(),
        );
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "find", .. })));
    }

    #[test]
    fn autocomplete_returns_unsupported() {
        let p = CrossrefProvider::new(None);
        let r = p.dispatch(
            ProviderCmd::Autocomplete { entity: "works".into(), q: "cs".into() },
            &dummy_opts(),
        );
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "autocomplete", .. })));
    }

    #[test]
    fn cited_by_returns_unsupported() {
        let p = CrossrefProvider::new(None);
        let r = p.dispatch(ProviderCmd::CitedBy { id: "x".into() }, &dummy_opts());
        assert!(matches!(r, Err(ProviderError::UnsupportedVerb { verb: "cited_by", .. })));
    }
}

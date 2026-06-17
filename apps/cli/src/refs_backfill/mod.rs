pub mod crossref;
pub mod mapping;
pub mod opencitations;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RefsBackfillError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, RefsBackfillError>;

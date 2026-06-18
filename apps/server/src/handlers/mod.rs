//! Concrete `FetchHandler` implementations for the local server.
//!
//! Each handler is thin: resolve → fetch → write. Network/upstream errors map to
//! `DomainError::Backend` so the worker's retry path re-engages.

pub mod fulltext;
pub mod refs;

pub use fulltext::FulltextHandler;
pub use refs::RefsHandler;

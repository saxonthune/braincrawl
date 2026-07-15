//! Concrete `FetchHandler` implementations for the local server.
//!
//! Each handler is thin: resolve → fetch → write. Network/upstream errors map to
//! `DomainError::Backend` so the worker's retry path re-engages.

pub mod events;
pub mod fulltext;
pub mod l3;
pub mod refs;

pub use events::handler_events;
pub use fulltext::FulltextHandler;
pub use l3::{
    handler_l3_agent_get, handler_l3_agent_list, handler_l3_agent_put, handler_l3_doc_get,
    handler_l3_doc_put, handler_l3_docs_list, handler_l3_graph,
};
pub use refs::RefsHandler;

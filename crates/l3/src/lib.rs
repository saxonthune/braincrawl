//! Lifts the research graph out of a Research Collection's `*.l3.md` files.
//! See glossary §"The research graph" (`.rhidoc/01-product/01-glossary.md`).

mod assign;
mod model;
mod parse;
pub mod ids;

pub use assign::{assign_ids, Assigned};
pub use model::{Endpoint, Graph, Link, Node, NodeId, Provenance, Warning};
pub use parse::parse;

//! Lifts the research graph out of a Research Collection's `*.l3.md` files.
//! See glossary §"The research graph" (`.rhidoc/01-product/01-glossary.md`).

mod assign;
mod frontmatter;
mod model;
mod normalize;
mod parse;
pub mod ids;

pub use assign::{assign_ids, assign_ids_source, collect_anchors, Assigned};
pub use frontmatter::upsert_frontmatter_key;
pub use model::{Endpoint, Graph, Link, Node, NodeId, Provenance, Warning};
pub use normalize::{normalize_doc, NormalizeOutcome};
pub use parse::{parse, parse_sources};

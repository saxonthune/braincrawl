//! The research graph's types — see glossary §"The research graph".

use std::collections::BTreeMap;
use std::path::PathBuf;

use braincrawl_core::types::CanonicalId;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A research node's anchor, without the leading `^` (e.g. `"r-x7k2m"`).
/// A cross-doc reference is `"slug#r-x7k2m"`; a doc-level forward reference
/// (bare `[[slug]]`, no anchor) is `"doc:slug"`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

/// One endpoint of a link: either a research node or a catalog entry.
#[derive(Clone, Debug, PartialEq)]
pub enum Endpoint {
    Node(NodeId),
    Catalog(CanonicalId),
}

impl Endpoint {
    /// `openalex:…`/`doi:…` → catalog; `^r-…` → this doc; `slug#^r-…` → another
    /// doc; bare `slug` → a doc-level forward reference. Never fails — an
    /// unrecognized string still resolves to a forward-reference node id.
    pub fn resolve(target: &str) -> Endpoint {
        if target.starts_with("openalex:") || target.starts_with("doi:") {
            return Endpoint::Catalog(CanonicalId(target.to_string()));
        }
        if let Some(anchor) = target.strip_prefix('^') {
            return Endpoint::Node(NodeId(anchor.to_string()));
        }
        if let Some((slug, anchor)) = target.split_once("#^") {
            return Endpoint::Node(NodeId(format!("{slug}#{anchor}")));
        }
        Endpoint::Node(NodeId(format!("doc:{target}")))
    }
}

impl Serialize for Endpoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = match self {
            Endpoint::Node(id) => format!("node:{}", id.0),
            Endpoint::Catalog(id) => id.0.clone(),
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Endpoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.strip_prefix("node:") {
            Some(rest) => Endpoint::Node(NodeId(rest.to_string())),
            None => Endpoint::Catalog(CanonicalId(s)),
        })
    }
}

/// Where a node came from — never serialized (the wire format is `id`/`labels`/`properties`).
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    pub doc: String,
    pub path: PathBuf,
    /// Position of this node among the nodes of its file (0-based).
    pub order: usize,
    /// 1-based line number of the `##` heading.
    pub heading_line: usize,
}

/// A research node — an id-bearing block lifted from a `##` heading.
/// `id` is `None` until assign-ids has run (a later phase mints anchors).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: Option<NodeId>,
    pub labels: Vec<String>,
    pub properties: Map<String, Value>,
    pub provenance: Provenance,
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Node", 3)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("labels", &self.labels)?;
        s.serialize_field("properties", &self.properties)?;
        s.end()
    }
}

/// A typed connection in the research graph. `recorded_in` (the node whose body
/// held the link line) is bookkeeping, not part of the wire format.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub source: Endpoint,
    pub kind: String,
    pub target: Endpoint,
    pub properties: Map<String, Value>,
    pub recorded_in: Option<NodeId>,
}

impl Serialize for Link {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Link", 4)?;
        s.serialize_field("source", &self.source)?;
        s.serialize_field("target", &self.target)?;
        s.serialize_field("type", &self.kind)?;
        s.serialize_field("properties", &self.properties)?;
        s.end()
    }
}

/// The research graph: nodes and links, plus private indexes rebuilt in `build()`.
pub struct Graph {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    by_id: BTreeMap<String, usize>,
    by_label: BTreeMap<String, Vec<usize>>,
    adjacency: BTreeMap<String, Vec<usize>>,
}

impl Graph {
    /// Sorts nodes by (doc, order) and rebuilds the id/label/adjacency indexes.
    pub fn build(mut nodes: Vec<Node>, links: Vec<Link>) -> Graph {
        nodes.sort_by(|a, b| {
            a.provenance
                .doc
                .cmp(&b.provenance.doc)
                .then(a.provenance.order.cmp(&b.provenance.order))
        });

        let mut by_id = BTreeMap::new();
        let mut by_label: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, node) in nodes.iter().enumerate() {
            if let Some(id) = &node.id {
                by_id.insert(id.0.clone(), i);
            }
            for label in &node.labels {
                by_label.entry(label.clone()).or_default().push(i);
            }
        }

        let mut adjacency: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, link) in links.iter().enumerate() {
            if let Endpoint::Node(id) = &link.source {
                adjacency.entry(id.0.clone()).or_default().push(i);
            }
            if let Endpoint::Node(id) = &link.target {
                adjacency.entry(id.0.clone()).or_default().push(i);
            }
        }

        Graph { nodes, links, by_id, by_label, adjacency }
    }

    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    pub fn nodes_by_label(&self, label: &str) -> Vec<&Node> {
        self.by_label
            .get(label)
            .map(|idxs| idxs.iter().map(|&i| &self.nodes[i]).collect())
            .unwrap_or_default()
    }

    pub fn links_touching(&self, id: &str) -> Vec<&Link> {
        self.adjacency
            .get(id)
            .map(|idxs| idxs.iter().map(|&i| &self.links[i]).collect())
            .unwrap_or_default()
    }
}

impl Serialize for Graph {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Graph", 2)?;
        s.serialize_field("nodes", &self.nodes)?;
        s.serialize_field("links", &self.links)?;
        s.end()
    }
}

/// A non-fatal issue found while parsing — malformed input still produces a graph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Warning {
    pub doc: String,
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

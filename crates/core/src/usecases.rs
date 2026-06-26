//! Platform-agnostic use-case layer (doc02.01.03).
//!
//! [`Store`] is generic over all Phase-1 trait bounds:
//! ```text
//! Store<M, B, P, R, C, Clk, Id>
//!   M   : MetadataStore
//!   B   : BlobStore
//!   P   : PayloadsRepo
//!   R   : IdResolver   (wired to Cloudflare KV in Phase 5; unused here)
//!   C   : Coordinator
//!   Clk : Clock
//!   Id  : IdGen
//! ```
//!
//! ## Alias namespace priority for coordinator lock key
//!
//! When multiple aliases are present, the one with the highest-priority namespace is
//! used as the lock key.  Priority (lowest number = highest priority):
//!
//! | Priority | Namespace        |
//! |----------|------------------|
//! | 0        | doi              |
//! | 1        | pmid             |
//! | 2        | pmcid            |
//! | 3        | openalex         |
//! | 4        | semantic_scholar |
//! | 5        | arxiv            |
//! | 255      | (anything else)  |
//!
//! ## Merge tie-break in `get_work`
//!
//! For each attribute field, the value from the assertion with the **newest `fetched_at`**
//! wins.  Tie-break: **lexicographically smallest source name** (deterministic alphabetical
//! order).  The `provenance` map records which source won each field.

use crate::{
    traits::{BlobStore, Clock, Coordinator, IdGen, IdResolver, MetadataStore, PayloadsRepo},
    types::{
        Alias, CanonicalId, ContentOutcome, DomainError, EdgeDir, EdgeInput, EdgeView, GraphStats,
        Neighborhood, NeighborhoodEdge, NeighborhoodNode, NodeKind, PayloadDescriptor, PayloadKind,
        WorkRecord, WorkView,
    },
};

const NEIGHBORHOOD_EDGE_PAGE: u32 = 200;

// ---------------------------------------------------------------------------
// Store struct
// ---------------------------------------------------------------------------

pub struct Store<M, B, P, R, C, Clk, Id>
where
    M: MetadataStore,
    B: BlobStore,
    P: PayloadsRepo,
    R: IdResolver,
    C: Coordinator,
    Clk: Clock,
    Id: IdGen,
{
    pub meta: M,
    pub blob: B,
    pub payloads: P,
    /// Id resolution cache (KV in Phase 5; unused in Phase 2).
    pub resolver: R,
    pub coord: C,
    pub clock: Clk,
    pub id_gen: Id,
}

// ---------------------------------------------------------------------------
// Use-case methods
// ---------------------------------------------------------------------------

impl<M, B, P, R, C, Clk, Id> Store<M, B, P, R, C, Clk, Id>
where
    M: MetadataStore,
    B: BlobStore,
    P: PayloadsRepo,
    R: IdResolver,
    C: Coordinator,
    Clk: Clock,
    Id: IdGen,
{
    /// Resolve-and-upsert a work record (doc02.01.02 §resolve(record)→guid).
    ///
    /// Algorithm:
    /// 1. Take `record.aliases` as the id bundle.
    /// 2. Look each alias up; collect distinct live GUIDs G.
    /// 3. |G|=0 → mint new node; insert all aliases.
    /// 4. |G|=1 → use it; insert any missing aliases.
    /// 5. |G|≥2 → pick lex-smallest GUID as survivor; merge losers; attach aliases.
    ///
    /// The entire resolve+merge is wrapped in `Coordinator::with_lock` keyed by the
    /// strongest alias in the bundle.
    pub async fn put_work(&self, record: WorkRecord) -> Result<CanonicalId, DomainError> {
        let lock_key = strongest_alias_key(&record.aliases);
        let _guard = self.coord.with_lock(&lock_key).await?;

        // Collect distinct live GUIDs for all known aliases.
        let mut guids: Vec<CanonicalId> = Vec::new();
        for alias in &record.aliases {
            if let Some(raw) = self.meta.get_alias(alias).await? {
                let live = self.meta.resolve_live(&raw).await?;
                if !guids.iter().any(|g| g.0 == live.0) {
                    guids.push(live);
                }
            }
        }

        let canonical = match guids.len() {
            0 => {
                // No existing node — mint a fresh one.
                let new_id = self.id_gen.new_guid();
                self.meta
                    .mint_node(&new_id, record.kind.clone(), &self.clock.now_rfc3339())
                    .await?;
                for alias in &record.aliases {
                    self.meta.get_or_create_alias(alias, &new_id).await?;
                }
                new_id
            }
            1 => {
                // Exactly one existing node — attach any missing aliases.
                let id = guids.remove(0);
                for alias in &record.aliases {
                    self.meta.get_or_create_alias(alias, &id).await?;
                }
                id
            }
            _ => {
                // Multiple distinct nodes — merge into lex-smallest survivor.
                guids.sort_by(|a, b| a.0.cmp(&b.0));
                let survivor = guids.remove(0);
                for loser in guids {
                    self.meta.merge(&survivor, &loser).await?;
                }
                for alias in &record.aliases {
                    self.meta.get_or_create_alias(alias, &survivor).await?;
                }
                survivor
            }
        };

        self.meta
            .upsert_node_assertion(
                &canonical,
                &record.source,
                &record.attrs,
                &self.clock.now_rfc3339(),
            )
            .await?;

        Ok(canonical)
    }

    /// Resolve alias → live node → merge assertions with per-field provenance.
    ///
    /// Returns `None` if the alias is unknown.
    pub async fn get_work(&self, id: Alias) -> Result<Option<WorkView>, DomainError> {
        let raw = match self.meta.get_alias(&id).await? {
            None => return Ok(None),
            Some(r) => r,
        };
        let canonical = self.meta.resolve_live(&raw).await?;

        let (kind, assertions, aliases) = match self.meta.read_node(&canonical).await? {
            None => return Ok(None),
            Some(n) => n,
        };

        let (attrs, provenance) = merge_assertions(&assertions);
        Ok(Some(WorkView {
            canonical_id: canonical,
            kind,
            attrs,
            provenance,
            aliases,
        }))
    }

    /// Store content bytes and record a payload descriptor.
    #[allow(clippy::too_many_arguments)]
    pub async fn put_content(
        &self,
        id: Alias,
        kind: PayloadKind,
        body: Vec<u8>,
        mime: String,
        source: Option<String>,
        source_url: Option<String>,
        fetched_at: String,
    ) -> Result<PayloadDescriptor, DomainError> {
        let canonical = self.resolve_alias_to_live(&id).await?;
        let version = self.payloads.next_version(&canonical, kind.clone()).await?;
        let kind_str = match kind {
            PayloadKind::Abstract => "abstract",
            PayloadKind::Fulltext => "fulltext",
        };
        let r2_key = format!("{}/{}/v{}", canonical.0, kind_str, version);
        let content_hash = fnv1a_hash(&body);
        let byte_size = body.len() as u64;

        self.blob.put(&r2_key, body, &mime).await?;

        let descriptor = PayloadDescriptor {
            canonical_id: canonical,
            kind,
            version,
            r2_key,
            content_hash,
            byte_size,
            mime,
            source,
            source_url,
            fetched_at,
            is_current: true,
        };
        self.payloads.record(&descriptor).await?;
        Ok(descriptor)
    }

    /// Retrieve content.
    pub async fn get_content(
        &self,
        id: Alias,
        kind: PayloadKind,
    ) -> Result<ContentOutcome, DomainError> {
        let raw = match self.meta.get_alias(&id).await? {
            None => return Ok(ContentOutcome::Absent),
            Some(r) => r,
        };
        let canonical = match self.meta.resolve_live(&raw).await {
            Ok(c) => c,
            Err(DomainError::NotFound) => return Ok(ContentOutcome::Absent),
            Err(e) => return Err(e),
        };

        let descriptor = match self.payloads.current_payload(&canonical, kind).await? {
            None => return Ok(ContentOutcome::Absent),
            Some(d) => d,
        };

        match self.blob.get(&descriptor.r2_key).await? {
            None => Ok(ContentOutcome::Pending),
            Some(b) => Ok(ContentOutcome::Bytes {
                bytes: b.bytes,
                mime: b.mime,
                content_hash: b.content_hash,
            }),
        }
    }

    /// Resolve src/dst aliases (minting stub nodes for unknowns), write edges.
    /// Returns count of edges written.
    ///
    /// A stub node is a node minted with no assertions; it becomes fully materialized
    /// when a `put_work` record arrives with one of its aliases.
    pub async fn put_edges(&self, edges: Vec<EdgeInput>) -> Result<usize, DomainError> {
        let mut count = 0;
        for edge in edges {
            let src = self.resolve_or_mint_stub(&edge.src).await?;
            let dst = self.resolve_or_mint_stub(&edge.dst).await?;
            self.meta
                .put_edge(
                    &src,
                    &dst,
                    &edge.relation,
                    &edge.source,
                    edge.attrs.as_ref(),
                    &edge.fetched_at,
                )
                .await?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve alias → live node → delegate to `read_edges`.
    pub async fn get_edges(
        &self,
        id: Alias,
        dir: EdgeDir,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<(Vec<EdgeView>, Option<String>), DomainError> {
        let raw = match self.meta.get_alias(&id).await? {
            None => return Ok((Vec::new(), None)),
            Some(r) => r,
        };
        let canonical = self.meta.resolve_live(&raw).await?;
        self.meta
            .read_edges(&canonical, dir, cursor.as_deref(), limit)
            .await
    }

    /// Return which of the given aliases are already present in the store.
    pub async fn have(&self, aliases: Vec<Alias>) -> Result<Vec<Alias>, DomainError> {
        self.meta.present_aliases(&aliases).await
    }

    /// Bounded BFS from seed aliases, returning a closed subgraph ranked by in-degree.
    ///
    /// In-degree is computed over edges discovered during traversal among the included
    /// nodes. Last-layer cross-edges that were never expanded are not counted — this is
    /// intentional and bounded by `max_nodes`.
    pub async fn neighborhood(
        &self,
        seeds: Vec<Alias>,
        dir: EdgeDir,
        depth: u32,
        max_nodes: u32,
    ) -> Result<Neighborhood, DomainError> {
        // 1. Resolve seeds.
        let mut visited: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for seed_alias in &seeds {
            let raw = match self.meta.get_alias(seed_alias).await? {
                None => continue,
                Some(r) => r,
            };
            let live = self.meta.resolve_live(&raw).await?;
            visited.entry(live.0).or_insert(0);
        }

        // If seed count exceeds max_nodes, keep only the first max_nodes (sorted for determinism).
        let mut truncated = false;
        if visited.len() as u32 > max_nodes {
            let mut keys: Vec<String> = visited.keys().cloned().collect();
            keys.sort();
            keys.truncate(max_nodes as usize);
            visited.retain(|k, _| keys.contains(k));
            truncated = true;
        }

        // Discovered edges: keyed by (src, dst, relation) for dedup.
        let mut discovered_edges: std::collections::HashSet<(String, String, String)> =
            std::collections::HashSet::new();
        // Also store the actual edge objects to avoid recomputing.
        let mut edge_list: Vec<NeighborhoodEdge> = Vec::new();

        // 2. BFS.
        let mut frontier: Vec<String> = visited.keys().cloned().collect();

        for current_depth in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let mut next_frontier: Vec<String> = Vec::new();

            for node_id in &frontier {
                // Drain all pages for this node.
                let mut cursor: Option<String> = None;
                loop {
                    let (edges, next_cursor) = self
                        .meta
                        .read_edges(
                            &CanonicalId(node_id.clone()),
                            dir.clone(),
                            cursor.as_deref(),
                            NEIGHBORHOOD_EDGE_PAGE,
                        )
                        .await?;

                    for ev in edges {
                        let edge_key = (ev.src.0.clone(), ev.dst.0.clone(), ev.relation.clone());
                        if discovered_edges.insert(edge_key) {
                            edge_list.push(NeighborhoodEdge {
                                src: ev.src.clone(),
                                dst: ev.dst.clone(),
                                relation: ev.relation.clone(),
                            });
                        }

                        // Neighbor depends on direction.
                        let neighbor_id = match dir {
                            EdgeDir::Forward => ev.dst.0.clone(),
                            EdgeDir::Backward => ev.src.0.clone(),
                        };

                        if !visited.contains_key(&neighbor_id) {
                            if (visited.len() as u32) < max_nodes {
                                visited.insert(neighbor_id.clone(), current_depth + 1);
                                next_frontier.push(neighbor_id);
                            } else {
                                truncated = true;
                            }
                        }
                    }

                    cursor = next_cursor;
                    if cursor.is_none() {
                        break;
                    }
                }
            }

            frontier = next_frontier;
        }

        // 3. Build edges: keep only those where both endpoints are in visited.
        let closed_edges: Vec<NeighborhoodEdge> = edge_list
            .into_iter()
            .filter(|e| visited.contains_key(&e.src.0) && visited.contains_key(&e.dst.0))
            .collect();

        // 4. Build nodes.
        let mut nodes: Vec<NeighborhoodNode> = Vec::new();
        for (id, node_depth) in &visited {
            let canonical = CanonicalId(id.clone());
            let (kind, attrs) = match self.meta.read_node(&canonical).await? {
                None => (NodeKind::Work, serde_json::Value::Object(serde_json::Map::new())),
                Some((k, assertions, _aliases)) => {
                    let (merged_attrs, _prov) = merge_assertions(&assertions);
                    (k, merged_attrs)
                }
            };

            let in_degree = closed_edges
                .iter()
                .filter(|e| e.dst.0 == *id)
                .count() as u32;

            nodes.push(NeighborhoodNode {
                canonical_id: canonical,
                kind,
                depth: *node_depth,
                in_degree,
                attrs,
            });
        }

        // 5. Sort nodes by in_degree desc, then canonical_id asc.
        nodes.sort_by(|a, b| {
            b.in_degree
                .cmp(&a.in_degree)
                .then(a.canonical_id.0.cmp(&b.canonical_id.0))
        });

        Ok(Neighborhood {
            nodes,
            edges: closed_edges,
            truncated,
        })
    }

    /// Aggregate counts over the whole metadata network.
    pub async fn stats(&self) -> Result<GraphStats, DomainError> {
        self.meta.stats().await
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    async fn resolve_alias_to_live(&self, alias: &Alias) -> Result<CanonicalId, DomainError> {
        match self.meta.get_alias(alias).await? {
            None => Err(DomainError::NotFound),
            Some(raw) => self.meta.resolve_live(&raw).await,
        }
    }

    /// Resolve an alias to its live node, or mint a stub Work node if unknown.
    async fn resolve_or_mint_stub(&self, alias: &Alias) -> Result<CanonicalId, DomainError> {
        if let Some(raw) = self.meta.get_alias(alias).await? {
            return self.meta.resolve_live(&raw).await;
        }
        // Mint stub: NodeKind::Work is the default for unknown references.
        let new_id = self.id_gen.new_guid();
        self.meta
            .mint_node(&new_id, NodeKind::Work, &self.clock.now_rfc3339())
            .await?;
        self.meta.get_or_create_alias(alias, &new_id).await?;
        Ok(new_id)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Alias namespace priority (lower = stronger; used for coordinator lock key).
fn namespace_priority(ns: &str) -> u8 {
    match ns {
        "doi" => 0,
        "pmid" => 1,
        "pmcid" => 2,
        "openalex" => 3,
        "semantic_scholar" => 4,
        "arxiv" => 5,
        _ => 255,
    }
}

/// Return a string key for the strongest alias in the bundle.
fn strongest_alias_key(aliases: &[Alias]) -> String {
    aliases
        .iter()
        .min_by_key(|a| (namespace_priority(&a.namespace), a.value.as_str()))
        .map(|a| format!("{}:{}", a.namespace, a.value))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Merge node assertions into a single `attrs` map with per-field `provenance`.
///
/// Winning source per field = newest `fetched_at`.
/// Tie-break: lexicographically smallest source name.
fn merge_assertions(
    assertions: &[(String, serde_json::Value, String)],
) -> (serde_json::Value, serde_json::Value) {
    // Sort: newest fetched_at first; tie-break: lex-smallest source first.
    let mut sorted = assertions.to_vec();
    sorted.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));

    let mut attrs = serde_json::Map::new();
    let mut provenance = serde_json::Map::new();

    for (source, assertion_attrs, fetched_at) in &sorted {
        if let serde_json::Value::Object(map) = assertion_attrs {
            for (key, value) in map {
                if !attrs.contains_key(key) {
                    attrs.insert(key.clone(), value.clone());
                    provenance.insert(
                        key.clone(),
                        serde_json::json!({ "source": source, "fetched_at": fetched_at }),
                    );
                }
            }
        }
    }

    (
        serde_json::Value::Object(attrs),
        serde_json::Value::Object(provenance),
    )
}

/// FNV-1a 64-bit hash used for `content_hash` in `put_content`.
///
/// Must match the algorithm in `braincrawl-blob-mem` so that the hash stored in the
/// `PayloadDescriptor` equals the hash returned by `BlobStore::get`.
fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a:{:016x}", h)
}

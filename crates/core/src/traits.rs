use async_trait::async_trait;

use crate::types::{
    Alias, CanonicalId, DeletePlan, DomainError, EdgeDir, EdgeView, ExportEdgeAssertion,
    ExportNode, GraphStats, Job, JobId, JobKind, JobSpec, NodeKind, Artifact, ArtifactRole,
    StoredBlob, WorkSearchFilter,
};

/// Opaque key → bytes. Knows nothing of `kind`/`version`.
#[async_trait(?Send)]
pub trait BlobStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError>;
    async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError>;
    async fn delete(&self, key: &str) -> Result<(), DomainError>;
}

#[async_trait(?Send)]
pub trait ArtifactStore {
    async fn current_artifact(
        &self,
        id: &CanonicalId,
        kind: ArtifactRole,
    ) -> Result<Option<Artifact>, DomainError>;
    async fn next_version(&self, id: &CanonicalId, kind: ArtifactRole) -> Result<u32, DomainError>;
    async fn record(&self, descriptor: &Artifact) -> Result<(), DomainError>;

    /// Every artifact descriptor held for a work, newest role/version first.
    /// `role` restricts to a single role; `all_versions` includes superseded
    /// versions rather than only the current one per role.
    async fn list_artifacts(
        &self,
        id: &CanonicalId,
        role: Option<ArtifactRole>,
        all_versions: bool,
    ) -> Result<Vec<Artifact>, DomainError>;

    /// Enumerate current artifact descriptors store-wide, keyset-paginated by
    /// (canonical_id, role). `cursor` is the opaque cursor from the previous
    /// page; `None` starts from the beginning. Descriptors only, never bytes.
    async fn export_artifacts(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<Artifact>, Option<String>), DomainError>;
}

/// The queryable graph facts.
#[async_trait(?Send)]
pub trait MetadataStore {
    async fn get_alias(&self, alias: &Alias) -> Result<Option<CanonicalId>, DomainError>;

    /// Get-or-create on the UNIQUE(scheme,value) constraint.
    /// INSERT … ON CONFLICT DO NOTHING then SELECT; returns the winner's id.
    async fn get_or_create_alias(
        &self,
        alias: &Alias,
        candidate: &CanonicalId,
    ) -> Result<CanonicalId, DomainError>;

    async fn create_node(
        &self,
        id: &CanonicalId,
        kind: NodeKind,
        created_at: &str,
    ) -> Result<(), DomainError>;

    async fn upsert_node_assertion(
        &self,
        id: &CanonicalId,
        source: &str,
        attrs: &serde_json::Value,
        fetched_at: &str,
    ) -> Result<(), DomainError>;

    /// Follow the `merged_into` chain to the live representative (path-compressed).
    async fn resolve_live(&self, id: &CanonicalId) -> Result<CanonicalId, DomainError>;

    /// Repoint alias/node_assertion/edge/edge_assertion/artifacts, fold PK collisions,
    /// record a merge redirect for the loser.
    async fn merge(
        &self,
        survivor: &CanonicalId,
        loser: &CanonicalId,
    ) -> Result<(), DomainError>;

    /// Compute the blast radius of deleting the live work `id` without mutating:
    /// the redirect network (the node plus every merge-redirect forwarding into
    /// it) and counts of the aliases/artifacts/edges it owns, with the blob keys.
    async fn plan_delete_work(&self, id: &CanonicalId) -> Result<DeletePlan, DomainError>;

    /// Hard-delete a work's entire redirect network and everything it owns
    /// (aliases, node assertions, artifacts, edges) in one transaction. Returns
    /// the blob keys removed, for the caller to delete from the blob store.
    async fn delete_work_network(&self, id: &CanonicalId) -> Result<Vec<String>, DomainError>;

    /// Returns (kind, [(source, attrs, fetched_at)], aliases) for read-time merge.
    async fn read_node(
        &self,
        id: &CanonicalId,
    ) -> Result<
        Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>,
        DomainError,
    >;

    async fn put_edge(
        &self,
        src: &CanonicalId,
        dst: &CanonicalId,
        relation: &str,
        source: &str,
        attrs: Option<&serde_json::Value>,
        fetched_at: &str,
    ) -> Result<(), DomainError>;

    async fn read_edges(
        &self,
        id: &CanonicalId,
        dir: EdgeDir,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<EdgeView>, Option<String>), DomainError>;

    /// Returns which of the given aliases are already known (for `have` queries).
    async fn present_aliases(&self, aliases: &[Alias]) -> Result<Vec<Alias>, DomainError>;

    /// Live work nodes whose assertions match every set field of `filter`
    /// (author/title as case-insensitive substrings, year exact), ordered by
    /// canonical_id and capped at `limit`. An empty filter matches every live
    /// work (the use-case layer decides whether to allow that).
    async fn search_work_ids(
        &self,
        filter: &WorkSearchFilter,
        limit: u32,
    ) -> Result<Vec<CanonicalId>, DomainError>;

    /// Aggregate counts over the whole network (works, nodes, edges, sources).
    async fn stats(&self) -> Result<GraphStats, DomainError>;

    /// Names of migrations applied in this database, for drift detection against
    /// the set compiled into the running binary.
    async fn applied_migrations(&self) -> Result<Vec<String>, DomainError>;

    /// Enumerate live nodes (with aliases and assertions), keyset-paginated by
    /// canonical_id. Merge redirects are excluded — a sync replay only needs the
    /// live representative; the destination re-merges by alias on its own.
    async fn export_nodes(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ExportNode>, Option<String>), DomainError>;

    /// Enumerate edge assertions store-wide, keyset-paginated by the full
    /// (src_id, dst_id, relation, source) primary key.
    async fn export_edge_assertions(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ExportEdgeAssertion>, Option<String>), DomainError>;
}

/// KV projection cache for id resolution.
#[async_trait(?Send)]
pub trait IdResolver {
    async fn resolve(
        &self,
        scheme: &str,
        value: &str,
    ) -> Result<Option<CanonicalId>, DomainError>;
    async fn remember(
        &self,
        canonical: &CanonicalId,
        scheme: &str,
        value: &str,
    ) -> Result<(), DomainError>;
}

/// Provides the current wall-clock time as an RFC 3339 string.
pub trait Clock {
    fn now_rfc3339(&self) -> String;
}

/// Generates new braincrawl GUIDs.
pub trait IdGen {
    fn new_guid(&self) -> CanonicalId;
}

/// Per-work single-writer seam (doc02.01.02 §Concurrency, doc02.02.00).
/// A no-op impl is trivially writable.
#[async_trait(?Send)]
pub trait Coordinator {
    async fn with_lock(&self, key: &str) -> Result<LockGuard, DomainError>;
}

/// Returned by `Coordinator::with_lock`; dropping releases the lock.
pub struct LockGuard;

// ── Queue traits ──────────────────────────────────────────────────────────────

/// Producer seam (write-only). Idempotent on `(kind, target_id)` while active.
#[async_trait(?Send)]
pub trait JobEnqueuer {
    async fn enqueue(&self, spec: JobSpec) -> Result<JobId, DomainError>;
}

/// Worker lifecycle seam.
#[async_trait(?Send)]
pub trait JobQueue {
    async fn claim(&self, limit: u32, now: &str) -> Result<Vec<Job>, DomainError>;
    async fn complete(&self, id: &JobId) -> Result<(), DomainError>;
    async fn retry(&self, id: &JobId, run_after: &str, err: &str) -> Result<(), DomainError>;
    async fn fail(&self, id: &JobId, err: &str) -> Result<(), DomainError>;
}

/// Per-kind unit of work; the only seam that knows an upstream.
#[async_trait(?Send)]
pub trait FetchHandler {
    fn kind(&self) -> JobKind;
    async fn handle(&self, job: &Job) -> Result<(), DomainError>;
}

use async_trait::async_trait;

use crate::types::{DomainError, Kind, PayloadDescriptor, StoredBlob, WorkId};

/// Opaque key → bytes. Knows nothing of `kind`/`version`.
#[async_trait(?Send)]
pub trait BlobStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError>;
    async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError>;
    async fn delete(&self, key: &str) -> Result<(), DomainError>;
}

#[async_trait(?Send)]
pub trait PayloadsRepo {
    async fn current_payload(
        &self,
        id: &WorkId,
        kind: Kind,
    ) -> Result<Option<PayloadDescriptor>, DomainError>;
    async fn next_version(&self, id: &WorkId, kind: Kind) -> Result<u32, DomainError>;
    async fn record(&self, descriptor: &PayloadDescriptor) -> Result<(), DomainError>;
}

/// The queryable graph facts; expands in a later task.
#[async_trait(?Send)]
pub trait MetadataStore {
    async fn upsert_stub(&self, id: &WorkId) -> Result<(), DomainError>;
    async fn edges_out(&self, id: &WorkId) -> Result<Vec<WorkId>, DomainError>;
}

#[async_trait(?Send)]
pub trait IdResolver {
    async fn resolve(&self, namespace: &str, value: &str) -> Result<Option<WorkId>, DomainError>;
    async fn remember(
        &self,
        canonical: &WorkId,
        namespace: &str,
        value: &str,
    ) -> Result<(), DomainError>;
}

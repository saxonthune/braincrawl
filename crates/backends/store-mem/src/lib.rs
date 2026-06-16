//! `PayloadsRepo` + `MetadataStore` backed by in-memory storage (for tests).

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    traits::{MetadataStore, PayloadsRepo},
    types::{DomainError, Kind, PayloadDescriptor, WorkId},
};

pub struct MemStore;

#[async_trait(?Send)]
impl PayloadsRepo for MemStore {
    async fn current_payload(
        &self,
        _id: &WorkId,
        _kind: Kind,
    ) -> Result<Option<PayloadDescriptor>, DomainError> {
        todo!("implement against in-memory (tests)")
    }

    async fn next_version(&self, _id: &WorkId, _kind: Kind) -> Result<u32, DomainError> {
        todo!("implement against in-memory (tests)")
    }

    async fn record(&self, _descriptor: &PayloadDescriptor) -> Result<(), DomainError> {
        todo!("implement against in-memory (tests)")
    }
}

#[async_trait(?Send)]
impl MetadataStore for MemStore {
    async fn upsert_stub(&self, _id: &WorkId) -> Result<(), DomainError> {
        todo!("implement against in-memory (tests)")
    }

    async fn edges_out(&self, _id: &WorkId) -> Result<Vec<WorkId>, DomainError> {
        todo!("implement against in-memory (tests)")
    }
}

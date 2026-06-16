//! `PayloadsRepo` + `MetadataStore` backed by local SQLite.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    ports::{MetadataStore, PayloadsRepo},
    types::{DomainError, Kind, PayloadDescriptor, WorkId},
};

pub struct SqliteStore;

#[async_trait(?Send)]
impl PayloadsRepo for SqliteStore {
    async fn current_payload(
        &self,
        _id: &WorkId,
        _kind: Kind,
    ) -> Result<Option<PayloadDescriptor>, DomainError> {
        todo!("implement against local SQLite")
    }

    async fn next_version(&self, _id: &WorkId, _kind: Kind) -> Result<u32, DomainError> {
        todo!("implement against local SQLite")
    }

    async fn record(&self, _descriptor: &PayloadDescriptor) -> Result<(), DomainError> {
        todo!("implement against local SQLite")
    }
}

#[async_trait(?Send)]
impl MetadataStore for SqliteStore {
    async fn upsert_stub(&self, _id: &WorkId) -> Result<(), DomainError> {
        todo!("implement against local SQLite")
    }

    async fn edges_out(&self, _id: &WorkId) -> Result<Vec<WorkId>, DomainError> {
        todo!("implement against local SQLite")
    }
}

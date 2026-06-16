//! `PayloadsRepo` + `MetadataStore` backed by Cloudflare D1.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    ports::{MetadataStore, PayloadsRepo},
    types::{DomainError, Kind, PayloadDescriptor, WorkId},
};

pub struct D1Store;

#[async_trait(?Send)]
impl PayloadsRepo for D1Store {
    async fn current_payload(
        &self,
        _id: &WorkId,
        _kind: Kind,
    ) -> Result<Option<PayloadDescriptor>, DomainError> {
        todo!("implement against Cloudflare D1")
    }

    async fn next_version(&self, _id: &WorkId, _kind: Kind) -> Result<u32, DomainError> {
        todo!("implement against Cloudflare D1")
    }

    async fn record(&self, _descriptor: &PayloadDescriptor) -> Result<(), DomainError> {
        todo!("implement against Cloudflare D1")
    }
}

#[async_trait(?Send)]
impl MetadataStore for D1Store {
    async fn upsert_stub(&self, _id: &WorkId) -> Result<(), DomainError> {
        todo!("implement against Cloudflare D1")
    }

    async fn edges_out(&self, _id: &WorkId) -> Result<Vec<WorkId>, DomainError> {
        todo!("implement against Cloudflare D1")
    }
}

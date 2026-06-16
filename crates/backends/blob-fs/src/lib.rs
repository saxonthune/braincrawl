//! `BlobStore` backed by the local filesystem.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    traits::BlobStore,
    types::{DomainError, StoredBlob},
};

pub struct FsBlobStore;

#[async_trait(?Send)]
impl BlobStore for FsBlobStore {
    async fn put(&self, _key: &str, _bytes: Vec<u8>, _mime: &str) -> Result<(), DomainError> {
        todo!("implement against local filesystem")
    }

    async fn get(&self, _key: &str) -> Result<Option<StoredBlob>, DomainError> {
        todo!("implement against local filesystem")
    }

    async fn delete(&self, _key: &str) -> Result<(), DomainError> {
        todo!("implement against local filesystem")
    }
}

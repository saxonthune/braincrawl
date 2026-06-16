//! `BlobStore` backed by in-memory storage (for tests).

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    ports::BlobStore,
    types::{DomainError, StoredBlob},
};

pub struct MemBlobStore;

#[async_trait(?Send)]
impl BlobStore for MemBlobStore {
    async fn put(&self, _key: &str, _bytes: Vec<u8>, _mime: &str) -> Result<(), DomainError> {
        todo!("implement against in-memory (tests)")
    }

    async fn get(&self, _key: &str) -> Result<Option<StoredBlob>, DomainError> {
        todo!("implement against in-memory (tests)")
    }

    async fn delete(&self, _key: &str) -> Result<(), DomainError> {
        todo!("implement against in-memory (tests)")
    }
}

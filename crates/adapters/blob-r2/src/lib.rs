//! `BlobStore` backed by Cloudflare R2.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    ports::BlobStore,
    types::{DomainError, StoredBlob},
};

pub struct R2BlobStore;

#[async_trait(?Send)]
impl BlobStore for R2BlobStore {
    async fn put(&self, _key: &str, _bytes: Vec<u8>, _mime: &str) -> Result<(), DomainError> {
        todo!("implement against Cloudflare R2")
    }

    async fn get(&self, _key: &str) -> Result<Option<StoredBlob>, DomainError> {
        todo!("implement against Cloudflare R2")
    }

    async fn delete(&self, _key: &str) -> Result<(), DomainError> {
        todo!("implement against Cloudflare R2")
    }
}

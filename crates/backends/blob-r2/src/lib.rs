//! `BlobStore` backed by Cloudflare R2.
//!
//! Without the `cloudflare` feature, this crate exposes a unit-struct stub that
//! returns `DomainError::Backend` on every call (host compilation / test builds).
//! With the `cloudflare` feature (enabled by `apps/worker`), the real R2 impl
//! is compiled using `workers-rs`.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    traits::BlobStore,
    types::{DomainError, StoredBlob},
};

// FNV-1a 64-bit — matches the hash used by blob-fs and core use-cases.
fn fnv1a(data: &[u8]) -> u64 {
    let mut h = 14695981039346656037u64;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn be(e: impl std::fmt::Display) -> DomainError {
    DomainError::Backend(e.to_string())
}

// ── stub (no `cloudflare` feature) ───────────────────────────────────────────

#[cfg(not(feature = "cloudflare"))]
pub struct R2BlobStore;

#[cfg(not(feature = "cloudflare"))]
#[async_trait(?Send)]
impl BlobStore for R2BlobStore {
    async fn put(&self, _key: &str, _bytes: Vec<u8>, _mime: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("R2BlobStore: cloudflare feature not enabled".into()))
    }

    async fn get(&self, _key: &str) -> Result<Option<StoredBlob>, DomainError> {
        Err(DomainError::Backend("R2BlobStore: cloudflare feature not enabled".into()))
    }

    async fn delete(&self, _key: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("R2BlobStore: cloudflare feature not enabled".into()))
    }
}

// ── real impl (cloudflare feature — wasm32 only) ─────────────────────────────

#[cfg(feature = "cloudflare")]
pub struct R2BlobStore {
    bucket: worker::Bucket,
}

#[cfg(feature = "cloudflare")]
impl R2BlobStore {
    pub fn new(bucket: worker::Bucket) -> Self {
        Self { bucket }
    }
}

#[cfg(feature = "cloudflare")]
#[async_trait(?Send)]
impl BlobStore for R2BlobStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError> {
        use worker::HttpMetadata;
        self.bucket
            .put(key, bytes)
            .http_metadata(HttpMetadata {
                content_type: Some(mime.to_string()),
                ..HttpMetadata::default()
            })
            .execute()
            .await
            .map_err(be)?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError> {
        let obj = self.bucket.get(key).execute().await.map_err(be)?;
        match obj {
            None => Ok(None),
            Some(obj) => {
                let mime = obj
                    .http_metadata()
                    .content_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let body = obj
                    .body()
                    .ok_or_else(|| DomainError::Backend("R2 object body already consumed".into()))?;
                let bytes = body.bytes().await.map_err(be)?;
                let content_hash = format!("{:016x}", fnv1a(&bytes));
                Ok(Some(StoredBlob { bytes, mime, content_hash }))
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<(), DomainError> {
        self.bucket.delete(key).await.map_err(be)?;
        Ok(())
    }
}

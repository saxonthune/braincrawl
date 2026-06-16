//! `BlobStore` backed by an in-memory `HashMap` (for tests).
//!
//! Content hashing uses FNV-1a 64-bit (no external deps, stable, deterministic).
//! The hash is formatted as `"fnv1a:<16-hex-digits>"`.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;

use async_trait::async_trait;
use braincrawl_core::{
    traits::BlobStore,
    types::{DomainError, StoredBlob},
};

pub struct MemBlobStore {
    /// key → (bytes, mime, content_hash)
    inner: RefCell<HashMap<String, (Vec<u8>, String, String)>>,
}

impl MemBlobStore {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(HashMap::new()),
        }
    }
}

impl Default for MemBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

/// FNV-1a 64-bit hash, formatted as `"fnv1a:<16-hex-digits>"`.
pub fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a:{:016x}", h)
}

#[async_trait(?Send)]
impl BlobStore for MemBlobStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError> {
        let hash = fnv1a_hash(&bytes);
        self.inner
            .borrow_mut()
            .insert(key.to_string(), (bytes, mime.to_string(), hash));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError> {
        Ok(self.inner.borrow().get(key).map(|(bytes, mime, hash)| StoredBlob {
            bytes: bytes.clone(),
            mime: mime.clone(),
            content_hash: hash.clone(),
        }))
    }

    async fn delete(&self, key: &str) -> Result<(), DomainError> {
        self.inner.borrow_mut().remove(key);
        Ok(())
    }
}

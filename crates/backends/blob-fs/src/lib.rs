//! `BlobStore` backed by the local filesystem.
//!
//! Blobs are stored as `{root}/{key}` with a companion `{root}/{key}.mime` sidecar.
//! The key may contain `/` which creates nested directories under root.

#![allow(dead_code)]

use std::path::PathBuf;

use async_trait::async_trait;
use braincrawl_core::{
    traits::BlobStore,
    types::{DomainError, StoredBlob},
};

pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a:{:016x}", h)
}

fn mime_sidecar(blob_path: &std::path::Path) -> PathBuf {
    let mut p = blob_path.as_os_str().to_owned();
    p.push(".mime");
    PathBuf::from(p)
}

#[async_trait(?Send)]
impl BlobStore for FsBlobStore {
    async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DomainError::Backend(e.to_string()))?;
        }
        std::fs::write(&path, &bytes).map_err(|e| DomainError::Backend(e.to_string()))?;
        std::fs::write(mime_sidecar(&path), mime)
            .map_err(|e| DomainError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError> {
        let path = self.root.join(key);
        match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DomainError::Backend(e.to_string())),
            Ok(bytes) => {
                let mime = std::fs::read_to_string(mime_sidecar(&path))
                    .unwrap_or_else(|_| "application/octet-stream".to_string());
                Ok(Some(StoredBlob {
                    content_hash: fnv1a_hash(&bytes),
                    bytes,
                    mime,
                }))
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<(), DomainError> {
        let path = self.root.join(key);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                let _ = std::fs::remove_file(mime_sidecar(&path));
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DomainError::Backend(e.to_string())),
        }
    }
}
